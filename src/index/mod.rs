//! Persistent project index for tagpath.
//!
//! Builds a `.naming/index.json` snapshot containing the resolved config
//! fingerprint, scanned source files (with hash/size/mtime), grouped tag
//! families, and ontology references. The index supports cheap freshness
//! checks (`check`) and indexed search lookups.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::{config, extract, lint, ontology, parser};

/// Current on-disk schema version.
///
/// Bumped to `2` in tagpath 0.11.0 when stable `handle` fields were added to
/// `Family` and `FamilyMember`. See `SPEC.md` §15 for the wire contract.
pub const SCHEMA_VERSION: u32 = 2;

/// Default index file location, relative to the project root.
pub const INDEX_RELATIVE_PATH: &str = ".naming/index.json";

/// Full index payload written to disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Index {
    pub schema_version: u32,
    pub generated_at: String,
    pub config_fingerprint: String,
    pub tool_version: String,
    pub sources: Vec<Source>,
    pub families: Vec<Family>,
    pub ontology_refs: Vec<OntologyRef>,
}

/// One indexed source file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Source {
    pub path: String,
    pub hash: String,
    pub mtime: u64,
    pub size: u64,
}

/// Grouped tag family with members.
///
/// `handle` is a content-addressable identifier of the form
/// `fam:<sha256[0..16]>` derived from the canonical project root + sorted
/// tags + sorted ontology refs for this family. It does NOT depend on
/// source paths or member lines, so adding a member or moving a definition
/// inside a file does not change the handle. Renaming/retagging breaks the
/// handle on purpose. See `SPEC.md` §15.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Family {
    pub handle: String,
    pub family_id: String,
    pub tags: Vec<String>,
    pub members: Vec<FamilyMember>,
}

/// One identifier occurrence under a family.
///
/// `handle` is a content-addressable identifier of the form
/// `mem:<sha256[0..16]>` derived from the parent family handle + member
/// name + path-relative-to-project-root. Line numbers are intentionally
/// excluded so insertions above a symbol do not rot citations. Renames
/// break this on purpose. See `SPEC.md` §15.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FamilyMember {
    pub handle: String,
    pub family_handle: String,
    pub name: String,
    pub convention: String,
    pub path: String,
    pub line: usize,
}

/// Ontology reference loaded from `.naming/tags/*.md`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OntologyRef {
    pub tag: String,
    pub path: String,
    pub hash: String,
}

/// Options for [`build`].
#[derive(Debug, Clone)]
pub struct BuildOptions {
    /// Project root (must contain `.naming.toml`).
    pub project_root: PathBuf,
}

/// Result of `check`.
#[derive(Debug, Clone, Serialize)]
pub struct CheckReport {
    pub fresh: bool,
    pub index_path: PathBuf,
    pub stale_reasons: Vec<StaleReason>,
}

/// Why a freshness check failed.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StaleReason {
    IndexMissing,
    IndexUnreadable {
        message: String,
    },
    SchemaVersion {
        found: u32,
        expected: u32,
    },
    /// On-disk index uses an older schema. Treated like `SchemaVersion`
    /// but signals that the rebuild is a silent migration, not a real
    /// mismatch — consumers should rebuild without surfacing an error.
    SchemaChanged {
        found: u32,
        expected: u32,
    },
    ConfigChanged,
    ToolVersion {
        found: String,
        expected: String,
    },
    SourceAdded {
        path: String,
    },
    SourceRemoved {
        path: String,
    },
    SourceModified {
        path: String,
    },
}

/// Locate the project root: walk up from `start` until a `.naming.toml` is found.
pub fn find_project_root(start: &Path) -> Option<PathBuf> {
    let config = lint::find_config(start)?;
    config.parent().map(Path::to_path_buf)
}

/// Compute the on-disk index path for a project root.
pub fn index_path(project_root: &Path) -> PathBuf {
    project_root.join(INDEX_RELATIVE_PATH)
}

/// Build an `Index` by scanning the project rooted at `opts.project_root`.
pub fn build(opts: &BuildOptions) -> Result<Index, String> {
    let project_root = &opts.project_root;
    let config_path = project_root.join(".naming.toml");
    if !config_path.exists() {
        return Err(format!(
            "no .naming.toml found at {}",
            config_path.display()
        ));
    }
    let resolved = config::resolve(&config_path)?;
    let config_fingerprint = fingerprint_config(&resolved)?;

    let mut sources = Vec::new();
    for abs in extract::list_source_files(project_root) {
        let rel = relative_to(&abs, project_root);
        let meta =
            std::fs::metadata(&abs).map_err(|e| format!("metadata({}): {e}", abs.display()))?;
        let mtime = meta
            .modified()
            .ok()
            .and_then(|m| m.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let size = meta.len();
        let hash = hash_file(&abs)?;
        sources.push(Source {
            path: rel,
            hash,
            mtime,
            size,
        });
    }
    sources.sort_by(|a, b| a.path.cmp(&b.path));

    let occurrences = extract::extract_from_path(project_root);
    let mut family_map: BTreeMap<String, Family> = BTreeMap::new();
    for occ in occurrences {
        let canonical = occ.parsed.tags.join("_");
        if canonical.is_empty() {
            continue;
        }
        let entry = family_map
            .entry(canonical.clone())
            .or_insert_with(|| Family {
                handle: String::new(),
                family_id: canonical.clone(),
                tags: occ.parsed.tags.clone(),
                members: Vec::new(),
            });
        entry.members.push(FamilyMember {
            handle: String::new(),
            family_handle: String::new(),
            name: occ.identifier,
            convention: occ.parsed.convention.to_string(),
            path: relative_to(&occ.file, project_root),
            line: occ.line,
        });
    }
    let mut families: Vec<Family> = family_map.into_values().collect();
    for f in &mut families {
        f.members.sort_by(|a, b| {
            a.path
                .cmp(&b.path)
                .then(a.line.cmp(&b.line))
                .then(a.name.cmp(&b.name))
        });
        // Dedupe identical (path, line, name) occurrences.
        f.members.dedup();
    }
    families.sort_by(|a, b| a.family_id.cmp(&b.family_id));

    let ontology_refs = load_ontology_refs(project_root)?;
    let project_root_canonical = canonical_project_root(project_root);
    assign_handles(&mut families, &project_root_canonical, &ontology_refs);

    Ok(Index {
        schema_version: SCHEMA_VERSION,
        generated_at: iso8601_utc_now(),
        config_fingerprint,
        tool_version: env!("CARGO_PKG_VERSION").to_string(),
        sources,
        families,
        ontology_refs,
    })
}

/// Write an index to disk as pretty JSON, creating the parent directory.
pub fn write(idx: &Index, path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("create_dir_all({}): {e}", parent.display()))?;
    }
    let mut json =
        serde_json::to_string_pretty(idx).map_err(|e| format!("serialize index: {e}"))?;
    json.push('\n');
    std::fs::write(path, json).map_err(|e| format!("write({}): {e}", path.display()))?;
    Ok(())
}

/// Load an index from disk.
pub fn read(path: &Path) -> Result<Index, String> {
    let text =
        std::fs::read_to_string(path).map_err(|e| format!("read({}): {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("parse({}): {e}", path.display()))
}

/// Check freshness of the on-disk index at `project_root/.naming/index.json`.
///
/// Returns `fresh: true` if the on-disk index matches a freshly computed
/// fingerprint and per-source hashes; otherwise `fresh: false` with the
/// reasons collected.
pub fn check(project_root: &Path) -> Result<CheckReport, String> {
    let idx_path = index_path(project_root);
    if !idx_path.exists() {
        return Ok(CheckReport {
            fresh: false,
            index_path: idx_path,
            stale_reasons: vec![StaleReason::IndexMissing],
        });
    }
    let existing = match read(&idx_path) {
        Ok(i) => i,
        Err(message) => {
            return Ok(CheckReport {
                fresh: false,
                index_path: idx_path,
                stale_reasons: vec![StaleReason::IndexUnreadable { message }],
            });
        }
    };
    let mut reasons = Vec::new();
    if existing.schema_version != SCHEMA_VERSION {
        // Older on-disk schema is a silent migration trigger, not an
        // error. Future schema versions higher than ours still produce
        // a hard `SchemaVersion` mismatch (we cannot upgrade upward).
        if existing.schema_version < SCHEMA_VERSION {
            reasons.push(StaleReason::SchemaChanged {
                found: existing.schema_version,
                expected: SCHEMA_VERSION,
            });
        } else {
            reasons.push(StaleReason::SchemaVersion {
                found: existing.schema_version,
                expected: SCHEMA_VERSION,
            });
        }
    }
    let tool = env!("CARGO_PKG_VERSION");
    if existing.tool_version != tool {
        reasons.push(StaleReason::ToolVersion {
            found: existing.tool_version.clone(),
            expected: tool.to_string(),
        });
    }
    let config_path = project_root.join(".naming.toml");
    let resolved = config::resolve(&config_path)?;
    let fingerprint = fingerprint_config(&resolved)?;
    if existing.config_fingerprint != fingerprint {
        reasons.push(StaleReason::ConfigChanged);
    }
    // Recompute sources and compare.
    let mut current: BTreeMap<String, (String, u64, u64)> = BTreeMap::new();
    for abs in extract::list_source_files(project_root) {
        let rel = relative_to(&abs, project_root);
        let meta =
            std::fs::metadata(&abs).map_err(|e| format!("metadata({}): {e}", abs.display()))?;
        let mtime = meta
            .modified()
            .ok()
            .and_then(|m| m.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let size = meta.len();
        let hash = hash_file(&abs)?;
        current.insert(rel, (hash, mtime, size));
    }
    let existing_paths: BTreeSet<&str> = existing.sources.iter().map(|s| s.path.as_str()).collect();
    let current_paths: BTreeSet<&str> = current.keys().map(String::as_str).collect();
    for added in current_paths.difference(&existing_paths) {
        reasons.push(StaleReason::SourceAdded {
            path: (*added).to_string(),
        });
    }
    for removed in existing_paths.difference(&current_paths) {
        reasons.push(StaleReason::SourceRemoved {
            path: (*removed).to_string(),
        });
    }
    for src in &existing.sources {
        if let Some((hash, _mtime, size)) = current.get(&src.path)
            && (hash != &src.hash || size != &src.size)
        {
            reasons.push(StaleReason::SourceModified {
                path: src.path.clone(),
            });
        }
    }
    Ok(CheckReport {
        fresh: reasons.is_empty(),
        index_path: idx_path,
        stale_reasons: reasons,
    })
}

/// Indexed search: filter family members by tag-subset semantics.
pub fn search_index(idx: &Index, query: &str) -> Vec<FamilyMember> {
    let conv = parser::detect_convention(query);
    let parsed = parser::parse(query, conv);
    let query_tags: BTreeSet<&str> = parsed.tags.iter().map(String::as_str).collect();
    let mut out = Vec::new();
    for fam in &idx.families {
        let fam_tags: BTreeSet<&str> = fam.tags.iter().map(String::as_str).collect();
        if query_tags.iter().all(|t| fam_tags.contains(t)) {
            out.extend(fam.members.iter().cloned());
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path).then(a.line.cmp(&b.line)));
    out
}

// ---------- helpers ----------

fn fingerprint_config(cfg: &config::NamingConfig) -> Result<String, String> {
    // Serialize via serde_json with sorted keys (BTreeMap collection) to avoid
    // non-determinism from HashMap iteration order inside NamingConfig.
    let value: serde_json::Value = serde_json::to_value(cfg)
        .map_err(|e| format!("serialize resolved config for fingerprint: {e}"))?;
    let canonical = canonicalize_json(&value);
    let text = serde_json::to_string(&canonical)
        .map_err(|e| format!("canonicalize resolved config: {e}"))?;
    Ok(format!("sha256:{}", sha256_hex(text.as_bytes())))
}

/// Re-emit a JSON value with all object keys ordered (recursively).
fn canonicalize_json(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut sorted: BTreeMap<String, serde_json::Value> = BTreeMap::new();
            for (k, v) in map {
                sorted.insert(k.clone(), canonicalize_json(v));
            }
            let mut out = serde_json::Map::new();
            for (k, v) in sorted {
                out.insert(k, v);
            }
            serde_json::Value::Object(out)
        }
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.iter().map(canonicalize_json).collect())
        }
        _ => value.clone(),
    }
}

fn hash_file(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("read({}): {e}", path.display()))?;
    Ok(format!("sha256:{}", sha256_hex(&bytes)))
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(digest.len() * 2);
    for b in digest {
        hex.push_str(&format!("{b:02x}"));
    }
    hex
}

fn relative_to(path: &Path, root: &Path) -> String {
    let stripped = path.strip_prefix(root).unwrap_or(path);
    stripped.to_string_lossy().replace('\\', "/")
}

/// Canonicalize the project root path for handle derivation.
///
/// Falls back to the lexical path if canonicalization fails (e.g. the
/// directory was just removed). Always uses forward slashes so handles
/// stay portable across operating systems.
fn canonical_project_root(project_root: &Path) -> String {
    let resolved = project_root
        .canonicalize()
        .unwrap_or_else(|_| project_root.to_path_buf());
    resolved.to_string_lossy().replace('\\', "/")
}

/// Compute the stable family handle from the canonical project root,
/// sorted family tags, and the ontology refs that match those tags.
///
/// Inputs are intentionally limited to family identity. Member paths and
/// line numbers are excluded so adding a definition (or moving one inside
/// a file) does not rot the handle.
pub fn family_handle(
    project_root_canonical: &str,
    tags: &[String],
    ontology_refs: &[OntologyRef],
) -> String {
    let mut sorted_tags: Vec<&str> = tags.iter().map(String::as_str).collect();
    sorted_tags.sort_unstable();
    let tag_set: BTreeSet<&str> = sorted_tags.iter().copied().collect();
    let mut matching_refs: Vec<&str> = ontology_refs
        .iter()
        .filter(|r| tag_set.contains(r.tag.as_str()))
        .map(|r| r.tag.as_str())
        .collect();
    matching_refs.sort_unstable();
    matching_refs.dedup();
    let payload = format!(
        "{project_root_canonical}\n{}\n{}",
        sorted_tags.join(","),
        matching_refs.join(","),
    );
    let hex = sha256_hex(payload.as_bytes());
    format!("fam:{}", &hex[..16])
}

/// Compute the stable member handle from the parent family handle, the
/// member name, and the path relative to the project root. Line numbers
/// are intentionally excluded.
pub fn member_handle(family_handle: &str, name: &str, path: &str) -> String {
    let payload = format!("{family_handle}\n{name}\n{path}");
    let hex = sha256_hex(payload.as_bytes());
    format!("mem:{}", &hex[..16])
}

fn assign_handles(
    families: &mut [Family],
    project_root_canonical: &str,
    ontology_refs: &[OntologyRef],
) {
    for fam in families.iter_mut() {
        let h = family_handle(project_root_canonical, &fam.tags, ontology_refs);
        fam.handle = h.clone();
        for member in &mut fam.members {
            let mh = member_handle(&h, &member.name, &member.path);
            member.handle = mh;
            member.family_handle = h.clone();
        }
    }
}

/// Options for [`emit_jsonl`].
#[derive(Debug, Clone, Default)]
pub struct EmitJsonlOptions {
    /// When true, emit `{"type":"stale", ...}` records derived from a check
    /// report instead of header/source/family/member/footer records.
    pub check_mode: bool,
}

/// Stream an `Index` as NDJSON to `out`. One JSON object per line, in this
/// order: `header`, `source`*, `family`*, `member`*, `footer`. The footer's
/// counts must match the number of records emitted. The on-disk JSON shape
/// is unchanged — JSONL is purely an output transport for streaming
/// consumers (tsift, ad-hoc CLI pipelines).
pub fn emit_jsonl<W: Write>(idx: &Index, out: &mut W) -> Result<(), String> {
    let header = serde_json::json!({
        "type": "header",
        "schema_version": idx.schema_version,
        "tool_version": idx.tool_version,
        "config_fingerprint": idx.config_fingerprint,
        "generated_at": idx.generated_at,
    });
    writeln_json(out, &header)?;

    let mut source_count = 0usize;
    for src in &idx.sources {
        let rec = serde_json::json!({
            "type": "source",
            "path": src.path,
            "hash": src.hash,
            "mtime": src.mtime,
            "size": src.size,
        });
        writeln_json(out, &rec)?;
        source_count += 1;
    }

    let mut family_count = 0usize;
    let mut member_count = 0usize;
    // Emit all families first, then all members. Tsift wants stable
    // ordering: families form the citation table, members the rows.
    for fam in &idx.families {
        let ontology_refs_for_family: Vec<&str> = idx
            .ontology_refs
            .iter()
            .filter(|r| fam.tags.iter().any(|t| t == &r.tag))
            .map(|r| r.tag.as_str())
            .collect();
        let rec = serde_json::json!({
            "type": "family",
            "handle": fam.handle,
            "family_id": fam.family_id,
            "tags": fam.tags,
            "ontology_refs": ontology_refs_for_family,
        });
        writeln_json(out, &rec)?;
        family_count += 1;
    }
    for fam in &idx.families {
        for m in &fam.members {
            let rec = serde_json::json!({
                "type": "member",
                "handle": m.handle,
                "family_handle": m.family_handle,
                "name": m.name,
                "convention": m.convention,
                "path": m.path,
                "line": m.line,
            });
            writeln_json(out, &rec)?;
            member_count += 1;
        }
    }

    let footer = serde_json::json!({
        "type": "footer",
        "counts": {
            "sources": source_count,
            "families": family_count,
            "members": member_count,
            "ontology_refs": idx.ontology_refs.len(),
        },
    });
    writeln_json(out, &footer)?;
    Ok(())
}

/// Stream a stale report as NDJSON to `out`. One `{"type":"stale", ...}`
/// record per reason, plus a single `header` line carrying schema/tool
/// metadata derived from the on-disk index when available.
pub fn emit_jsonl_stale<W: Write>(
    project_root: &Path,
    report: &CheckReport,
    out: &mut W,
) -> Result<(), String> {
    // Best-effort header: report the active schema and tool versions.
    let header = serde_json::json!({
        "type": "header",
        "schema_version": SCHEMA_VERSION,
        "tool_version": env!("CARGO_PKG_VERSION"),
        "fresh": report.fresh,
        "index_path": report.index_path.display().to_string(),
        "project_root": project_root.display().to_string(),
    });
    writeln_json(out, &header)?;
    for reason in &report.stale_reasons {
        let rec = serde_json::json!({
            "type": "stale",
            "reason": reason,
        });
        writeln_json(out, &rec)?;
    }
    let footer = serde_json::json!({
        "type": "footer",
        "counts": {
            "stale_reasons": report.stale_reasons.len(),
        },
    });
    writeln_json(out, &footer)?;
    Ok(())
}

fn writeln_json<W: Write>(out: &mut W, value: &serde_json::Value) -> Result<(), String> {
    let line = serde_json::to_string(value).map_err(|e| format!("serialize jsonl record: {e}"))?;
    out.write_all(line.as_bytes())
        .map_err(|e| format!("write jsonl: {e}"))?;
    out.write_all(b"\n")
        .map_err(|e| format!("write jsonl newline: {e}"))?;
    Ok(())
}

fn load_ontology_refs(project_root: &Path) -> Result<Vec<OntologyRef>, String> {
    let tags_dir = project_root.join(".naming").join("tags");
    if !tags_dir.exists() {
        return Ok(Vec::new());
    }
    let report = ontology::load_project(project_root)?;
    let mut refs = Vec::new();
    for tag in &report.tags {
        let hash = hash_file(&tag.path)?;
        refs.push(OntologyRef {
            tag: tag.tag.clone(),
            path: relative_to(&tag.path, project_root),
            hash,
        });
    }
    refs.sort_by(|a, b| a.tag.cmp(&b.tag));
    Ok(refs)
}

/// Format current time as ISO-8601 UTC, second precision.
fn iso8601_utc_now() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format_iso8601_utc(secs)
}

/// Format a UNIX epoch second count as an ISO-8601 UTC `YYYY-MM-DDThh:mm:ssZ` string.
fn format_iso8601_utc(epoch_secs: u64) -> String {
    let secs = (epoch_secs % 60) as u32;
    let mins = ((epoch_secs / 60) % 60) as u32;
    let hours = ((epoch_secs / 3600) % 24) as u32;
    let mut days = epoch_secs / 86_400;
    // Days since 1970-01-01.
    let mut year: i64 = 1970;
    loop {
        let leap = is_leap_year(year);
        let year_days: u64 = if leap { 366 } else { 365 };
        if days < year_days {
            break;
        }
        days -= year_days;
        year += 1;
    }
    let days_in_month: [u64; 12] = [
        31,
        if is_leap_year(year) { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut month: usize = 0;
    while month < 12 && days >= days_in_month[month] {
        days -= days_in_month[month];
        month += 1;
    }
    let day = days as u32 + 1;
    format!(
        "{year:04}-{:02}-{day:02}T{hours:02}:{mins:02}:{secs:02}Z",
        month + 1
    )
}

fn is_leap_year(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso8601_format_epoch_zero() {
        assert_eq!(format_iso8601_utc(0), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn iso8601_format_known_value() {
        // 2026-05-24T22:00:00Z
        // days from 1970-01-01 = 20597, +22h
        let secs: u64 = 20597 * 86_400 + 22 * 3600;
        assert_eq!(format_iso8601_utc(secs), "2026-05-24T22:00:00Z");
    }

    #[test]
    fn sha256_hex_known() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
