//! Persistent project index for tagpath.
//!
//! Builds a `.naming/index.json` snapshot containing the resolved config
//! fingerprint, scanned source files (with hash/size/mtime), grouped tag
//! families, and ontology references. The index supports cheap freshness
//! checks (`check`) and indexed search lookups.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::{config, extract, lint, ontology, parser};

/// Current on-disk schema version.
pub const SCHEMA_VERSION: u32 = 1;

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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Family {
    pub family_id: String,
    pub tags: Vec<String>,
    pub members: Vec<FamilyMember>,
}

/// One identifier occurrence under a family.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FamilyMember {
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
    IndexUnreadable { message: String },
    SchemaVersion { found: u32, expected: u32 },
    ConfigChanged,
    ToolVersion { found: String, expected: String },
    SourceAdded { path: String },
    SourceRemoved { path: String },
    SourceModified { path: String },
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
                family_id: canonical.clone(),
                tags: occ.parsed.tags.clone(),
                members: Vec::new(),
            });
        entry.members.push(FamilyMember {
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
        reasons.push(StaleReason::SchemaVersion {
            found: existing.schema_version,
            expected: SCHEMA_VERSION,
        });
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
