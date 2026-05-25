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
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use crate::{config, extract, lint, ontology, parser};

pub mod sidecar;

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

/// Why an incremental update fell back to a full rebuild, if at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FallbackReason {
    /// `.naming/index.json` does not exist.
    IndexMissing,
    /// On-disk index could not be parsed.
    IndexUnreadable(String),
    /// On-disk `schema_version` differs from the current binary.
    SchemaVersionMismatch { found: u32, expected: u32 },
    /// Resolved config fingerprint changed since the last build.
    ConfigFingerprintMismatch,
}

impl FallbackReason {
    /// Short human-readable reason suitable for stderr.
    pub fn as_str(&self) -> String {
        match self {
            FallbackReason::IndexMissing => "index missing".to_string(),
            FallbackReason::IndexUnreadable(m) => format!("index unreadable: {m}"),
            FallbackReason::SchemaVersionMismatch { found, expected } => {
                format!("schema_version mismatch (found {found}, expected {expected})")
            }
            FallbackReason::ConfigFingerprintMismatch => "config_fingerprint mismatch".to_string(),
        }
    }
}

/// Per-source classification produced by [`update_incremental`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceChange {
    /// File present in old index, mtime+size matched → entry copied through.
    Unchanged,
    /// File present in old index, mtime/size changed but the rehashed
    /// content matches → mtime updated, no re-extraction.
    MtimeOnly,
    /// File present in old index but content hash differs → re-extracted.
    Changed,
    /// File not present in old index → extracted fresh.
    Added,
}

/// Summary of an incremental update, used to format the stderr digest.
#[derive(Debug, Clone)]
pub struct UpdateSummary {
    pub changed: usize,
    pub added: usize,
    pub removed: usize,
    pub unchanged: usize,
    pub elapsed_ms: u128,
    /// `Some(_)` if the update fell back to a full rebuild, `None` otherwise.
    pub fallback: Option<FallbackReason>,
}

impl UpdateSummary {
    /// True when the update produced no concrete file mutations: every
    /// source was either `Unchanged` or `MtimeOnly` and nothing was added
    /// or removed. Callers can use this to skip the JSON re-serialize +
    /// rename(2) round-trip; the existing on-disk index is already
    /// byte-equivalent to the recomputed in-memory result modulo
    /// `generated_at`.
    pub fn is_noop(&self) -> bool {
        self.fallback.is_none() && self.changed == 0 && self.added == 0 && self.removed == 0
    }

    /// Pre-extraction snapshot of the update plan: counts of files in each
    /// `SourceChange` bucket. Emitted as the JSONL `update_plan` record.
    pub fn plan_line(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "update_plan",
            "changed": self.changed,
            "added": self.added,
            "removed": self.removed,
            "unchanged": self.unchanged,
        })
    }
}

/// Format the one-line stderr digest emitted after a successful incremental
/// update. Format:
/// `[tagpath] incremental update: 1 changed, 0 added, 0 removed, 998 unchanged (12ms)`
pub fn format_update_digest(summary: &UpdateSummary) -> String {
    format!(
        "[tagpath] incremental update: {} changed, {} added, {} removed, {} unchanged ({}ms)",
        summary.changed, summary.added, summary.removed, summary.unchanged, summary.elapsed_ms
    )
}

/// Format the one-line stderr digest emitted after a full rebuild that was
/// triggered through the `--update` entrypoint (either by user flag or
/// fallback). Mirrors [`format_update_digest`] so consumers can parse one
/// shape regardless of code path.
pub fn format_full_rebuild_digest(idx: &Index, elapsed_ms: u128) -> String {
    format!(
        "[tagpath] full rebuild: {} sources, {} families ({}ms)",
        idx.sources.len(),
        idx.families.len(),
        elapsed_ms
    )
}

/// Result of [`update_incremental`].
#[derive(Debug, Clone)]
pub struct UpdateResult {
    pub index: Index,
    pub summary: UpdateSummary,
}

/// Output shape requested from [`update_incremental_with`].
///
/// Internal knob (CLI-only). The library entrypoint
/// [`update_incremental`] always asks for [`Full`] so callers see the
/// complete `Index`; the CLI fast path can opt into [`LazyTail`] to
/// skip the relatively expensive bincode decode of the
/// `families`/`ontology_refs` tail when an update is a true no-op.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexShape {
    /// Always return a fully-populated `Index`. Required for callers
    /// that consume `families`/`ontology_refs` (NDJSON stream, library
    /// callers, tests).
    Full,
    /// On a true no-op cycle, leave `families` and `ontology_refs` as
    /// empty `Vec`s and skip the tail decode entirely. On any non-noop
    /// cycle this behaves like [`Full`] because the caller is about to
    /// rebuild families anyway. The CLI uses this in the bare
    /// `tagpath index --update` path because it only consults the
    /// summary; the discarded `Index` saves ~7-10ms per cycle on a
    /// 1000-source repo.
    LazyTail,
}

/// Incrementally update an on-disk index by reusing cached source entries
/// and re-extracting only the files whose content actually changed.
///
/// Falls back to a full rebuild — and returns the same shape — if any of
/// the guards trip:
///
/// - `.naming/index.json` is missing or unreadable
/// - on-disk `schema_version` differs from the current binary
/// - resolved `config_fingerprint` differs from the on-disk value
///
/// When fallback fires, `summary.fallback` is `Some(_)` and `summary.changed
/// / added / removed / unchanged` reflect the file counts of the rebuilt
/// index (added = sources, the rest zero) so the digest stays meaningful.
pub fn update_incremental(opts: &BuildOptions) -> Result<UpdateResult, String> {
    update_incremental_with(opts, IndexShape::Full)
}

/// Like [`update_incremental`] but lets the caller opt into the
/// CLI-fast-path tail-skip behaviour. See [`IndexShape`].
pub fn update_incremental_with(
    opts: &BuildOptions,
    shape: IndexShape,
) -> Result<UpdateResult, String> {
    let started = Instant::now();
    let project_root = &opts.project_root;
    let idx_path = index_path(project_root);

    // Fallback path: full rebuild + tag summary with fallback reason.
    let full_rebuild = |reason: Option<FallbackReason>| -> Result<UpdateResult, String> {
        let idx = build(opts)?;
        let elapsed_ms = started.elapsed().as_millis();
        let summary = UpdateSummary {
            changed: 0,
            added: idx.sources.len(),
            removed: 0,
            unchanged: 0,
            elapsed_ms,
            fallback: reason,
        };
        Ok(UpdateResult {
            index: idx,
            summary,
        })
    };

    if !idx_path.exists() {
        return full_rebuild(Some(FallbackReason::IndexMissing));
    }
    // Try the binary sidecar first. The two-payload framing lets us
    // decode the cheap head (schema/config/sources, a few hundred KB on
    // a 1000-source repo) without ever touching the much heavier tail
    // (families/ontology_refs, ~4-5 MB). The tail is only decoded on
    // the non-noop path further down. On any framing failure (missing,
    // corrupt, schema skew, hash mismatch, …) we silently fall back to
    // the JSON read path, then regenerate the sidecar before returning
    // so the next cycle hits the fast path again.
    let side_path = sidecar::sidecar_path_for(&idx_path);
    // `sidecar_bytes` holds the whole buffer when we got it from the
    // sidecar so the non-noop branch can decode the tail without a
    // second disk read.
    let mut sidecar_bytes: Option<Vec<u8>> = None;
    let mut existing_head: Option<sidecar::SidecarHead> = None;
    match sidecar::read_bytes(&side_path) {
        Ok(bytes) => match sidecar::decode_head(&bytes) {
            Ok(head) => {
                sidecar::debug_log("hit");
                existing_head = Some(head);
                sidecar_bytes = Some(bytes);
            }
            Err(reason) => {
                sidecar::debug_log(&format!("miss:{}", reason.as_str()));
            }
        },
        Err(reason) => {
            sidecar::debug_log(&format!("miss:{}", reason.as_str()));
        }
    };
    // Always reserve a JSON fallback closure: if the sidecar failed
    // (existing_head still None) we read JSON now; otherwise we defer
    // the JSON read until the non-noop path actually needs the tail
    // we don't yet have. Captured here because the existing JSON
    // payload is also what we'll re-emit into the sidecar when the
    // fast path was unavailable.
    let read_json_existing = || -> Result<Index, String> { read(&idx_path) };

    let mut existing_full: Option<Index> = None;
    let sidecar_was_authoritative = existing_head.is_some();
    if existing_head.is_none() {
        let idx = match read_json_existing() {
            Ok(i) => i,
            Err(message) => {
                return full_rebuild(Some(FallbackReason::IndexUnreadable(message)));
            }
        };
        existing_head = Some(sidecar::SidecarHead {
            generated_at: idx.generated_at.clone(),
            config_fingerprint: idx.config_fingerprint.clone(),
            tool_version: idx.tool_version.clone(),
            sources: idx.sources.clone(),
        });
        existing_full = Some(idx);
    }
    // SAFETY: existing_head is Some on every path above.
    let existing_head = existing_head.unwrap();
    // We do not encode `schema_version` in the head itself — the
    // frame header check already enforced it on the sidecar path, and
    // the JSON read path validates it below using existing_full.
    if let Some(ref full) = existing_full
        && full.schema_version != SCHEMA_VERSION
    {
        return full_rebuild(Some(FallbackReason::SchemaVersionMismatch {
            found: full.schema_version,
            expected: SCHEMA_VERSION,
        }));
    }

    // Resolve config + fingerprint.
    let config_path = project_root.join(".naming.toml");
    if !config_path.exists() {
        return Err(format!(
            "no .naming.toml found at {}",
            config_path.display()
        ));
    }
    let resolved = config::resolve(&config_path)?;
    let config_fingerprint = fingerprint_config(&resolved)?;
    if existing_head.config_fingerprint != config_fingerprint {
        return full_rebuild(Some(FallbackReason::ConfigFingerprintMismatch));
    }

    // Index existing sources for quick lookup.
    let existing_by_path: BTreeMap<&str, &Source> = existing_head
        .sources
        .iter()
        .map(|s| (s.path.as_str(), s))
        .collect();

    // Walk current source files and classify each.
    let mut new_sources: Vec<Source> = Vec::new();
    let mut classifications: BTreeMap<String, SourceChange> = BTreeMap::new();
    let mut paths_to_extract: Vec<PathBuf> = Vec::new();

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

        match existing_by_path.get(rel.as_str()) {
            Some(prev) if prev.mtime == mtime && prev.size == size => {
                // Fast path: trust the cached hash.
                new_sources.push(Source {
                    path: rel.clone(),
                    hash: prev.hash.clone(),
                    mtime,
                    size,
                });
                classifications.insert(rel, SourceChange::Unchanged);
            }
            Some(prev) => {
                // Slow path: rehash and decide.
                let hash = hash_file(&abs)?;
                if hash == prev.hash {
                    new_sources.push(Source {
                        path: rel.clone(),
                        hash,
                        mtime,
                        size,
                    });
                    classifications.insert(rel, SourceChange::MtimeOnly);
                } else {
                    new_sources.push(Source {
                        path: rel.clone(),
                        hash,
                        mtime,
                        size,
                    });
                    classifications.insert(rel.clone(), SourceChange::Changed);
                    paths_to_extract.push(abs);
                }
            }
            None => {
                let hash = hash_file(&abs)?;
                new_sources.push(Source {
                    path: rel.clone(),
                    hash,
                    mtime,
                    size,
                });
                classifications.insert(rel.clone(), SourceChange::Added);
                paths_to_extract.push(abs);
            }
        }
    }
    new_sources.sort_by(|a, b| a.path.cmp(&b.path));

    // Detect removals.
    let current_paths: BTreeSet<&str> = classifications.keys().map(String::as_str).collect();
    let removed: Vec<String> = existing_head
        .sources
        .iter()
        .filter(|s| !current_paths.contains(s.path.as_str()))
        .map(|s| s.path.clone())
        .collect();

    let mut changed_count = 0usize;
    let mut added_count = 0usize;
    let mut unchanged_count = 0usize;
    for c in classifications.values() {
        match c {
            SourceChange::Changed => changed_count += 1,
            SourceChange::Added => added_count += 1,
            SourceChange::Unchanged | SourceChange::MtimeOnly => unchanged_count += 1,
        }
    }

    // Fast no-op path: when no file content changed, the existing
    // families + ontology_refs are still authoritative. Return them as-is
    // with the (possibly refreshed) source mtimes baked in. Avoids
    // rebuilding the family map (~30ms on a 1000-file repo) and lets the
    // CLI also skip the JSON serialize + atomic rename.
    if changed_count == 0 && added_count == 0 && removed.is_empty() {
        // Fast no-op path. Two sub-modes controlled by `shape`:
        //
        //   - `IndexShape::Full` (library callers, NDJSON CLI path,
        //     tests): materialise the full `Index` by decoding the
        //     sidecar tail (or falling back to JSON read).
        //   - `IndexShape::LazyTail` (bare CLI fast path): leave
        //     `families`/`ontology_refs` empty. Saves ~7-10ms by
        //     skipping the bincode decode of ~15k families. Also lets
        //     us skip regenerating the sidecar tail when the head was
        //     authoritative — the on-disk tail is still byte-correct,
        //     we just chose not to read it.
        let need_tail_for_index = matches!(shape, IndexShape::Full);
        // On the head-only fast path we still want to *verify* the
        // sidecar's tail when we read from sidecar, so a silent
        // corruption on the tail does not survive across no-op cycles.
        // The cost of `decode_tail` itself is mostly the bincode
        // allocate; we get most of the win by skipping the deserialize
        // when the caller doesn't need the data. So when LazyTail is
        // set we trust the sidecar tail as-is and let a future
        // non-noop / full read catch corruption then.
        let mut tail_came_from_json = false;
        let tail: sidecar::SidecarTail = if need_tail_for_index {
            if let Some(ref full) = existing_full {
                sidecar::SidecarTail {
                    families: full.families.clone(),
                    ontology_refs: full.ontology_refs.clone(),
                }
            } else if let Some(ref bytes) = sidecar_bytes {
                match sidecar::decode_tail(bytes) {
                    Ok(t) => t,
                    Err(reason) => {
                        sidecar::debug_log(&format!("tail_miss:{}", reason.as_str()));
                        tail_came_from_json = true;
                        match read(&idx_path) {
                            Ok(full) => sidecar::SidecarTail {
                                families: full.families,
                                ontology_refs: full.ontology_refs,
                            },
                            Err(message) => {
                                return full_rebuild(Some(FallbackReason::IndexUnreadable(
                                    message,
                                )));
                            }
                        }
                    }
                }
            } else {
                return full_rebuild(Some(FallbackReason::IndexMissing));
            }
        } else {
            sidecar::debug_log("lazy_tail_skipped");
            sidecar::SidecarTail {
                families: Vec::new(),
                ontology_refs: Vec::new(),
            }
        };

        let idx = Index {
            schema_version: SCHEMA_VERSION,
            generated_at: iso8601_utc_now(),
            config_fingerprint: config_fingerprint.clone(),
            tool_version: env!("CARGO_PKG_VERSION").to_string(),
            sources: new_sources,
            families: tail.families.clone(),
            ontology_refs: tail.ontology_refs.clone(),
        };

        // Regenerate the sidecar whenever we did not trust the on-disk
        // copy end-to-end — either the head fell back to JSON, or the
        // tail did. On the LazyTail path we did not decode the tail,
        // so we can only regenerate when we have a tail in hand
        // (sidecar head was good); skip regen otherwise and let a
        // future Full cycle repair it. We do not touch the JSON file
        // because no-op cycles intentionally skip the rename(2) to
        // preserve mtime semantics.
        if need_tail_for_index && (!sidecar_was_authoritative || tail_came_from_json) {
            let existing_for_sidecar = Index {
                schema_version: SCHEMA_VERSION,
                generated_at: existing_head.generated_at.clone(),
                config_fingerprint: existing_head.config_fingerprint.clone(),
                tool_version: existing_head.tool_version.clone(),
                sources: existing_head.sources.clone(),
                families: tail.families,
                ontology_refs: tail.ontology_refs,
            };
            if let Err(e) = sidecar::write(&existing_for_sidecar, &side_path) {
                sidecar::debug_log(&format!("regen_failed:{e}"));
            } else {
                sidecar::debug_log("regen");
            }
        }
        let summary = UpdateSummary {
            changed: 0,
            added: 0,
            removed: 0,
            unchanged: unchanged_count,
            elapsed_ms: started.elapsed().as_millis(),
            fallback: None,
        };
        return Ok(UpdateResult {
            index: idx,
            summary,
        });
    }

    // Non-noop: we need families now. If we came in through the
    // sidecar fast path, decode the tail from the bytes we already
    // have; otherwise existing_full already carries them. On tail
    // decode failure, silently fall back to JSON read.
    let existing_tail: sidecar::SidecarTail = if let Some(ref full) = existing_full {
        sidecar::SidecarTail {
            families: full.families.clone(),
            ontology_refs: full.ontology_refs.clone(),
        }
    } else if let Some(ref bytes) = sidecar_bytes {
        match sidecar::decode_tail(bytes) {
            Ok(t) => t,
            Err(reason) => {
                sidecar::debug_log(&format!("tail_miss:{}", reason.as_str()));
                match read(&idx_path) {
                    Ok(full) => sidecar::SidecarTail {
                        families: full.families,
                        ontology_refs: full.ontology_refs,
                    },
                    Err(message) => {
                        return full_rebuild(Some(FallbackReason::IndexUnreadable(message)));
                    }
                }
            }
        }
    } else {
        return full_rebuild(Some(FallbackReason::IndexMissing));
    };

    // Collect preserved members from unchanged + mtime-only files.
    let preserved_paths: BTreeSet<&str> = classifications
        .iter()
        .filter(|(_, c)| matches!(c, SourceChange::Unchanged | SourceChange::MtimeOnly))
        .map(|(p, _)| p.as_str())
        .collect();

    // (family_id, FamilyMember-without-handle, family tags) accumulator.
    let mut family_map: BTreeMap<String, Family> = BTreeMap::new();
    for fam in &existing_tail.families {
        for m in &fam.members {
            if preserved_paths.contains(m.path.as_str()) {
                let entry = family_map
                    .entry(fam.family_id.clone())
                    .or_insert_with(|| Family {
                        handle: String::new(),
                        family_id: fam.family_id.clone(),
                        tags: fam.tags.clone(),
                        members: Vec::new(),
                    });
                entry.members.push(FamilyMember {
                    handle: String::new(),
                    family_handle: String::new(),
                    name: m.name.clone(),
                    convention: m.convention.clone(),
                    path: m.path.clone(),
                    line: m.line,
                });
            }
        }
    }

    // Extract members for changed + added files.
    for abs in &paths_to_extract {
        for occ in extract::extract_from_file(abs) {
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
    }

    let mut families: Vec<Family> = family_map
        .into_values()
        .filter(|f| !f.members.is_empty())
        .collect();
    for f in &mut families {
        f.members.sort_by(|a, b| {
            a.path
                .cmp(&b.path)
                .then(a.line.cmp(&b.line))
                .then(a.name.cmp(&b.name))
        });
        f.members.dedup();
    }
    families.sort_by(|a, b| a.family_id.cmp(&b.family_id));

    let ontology_refs = load_ontology_refs(project_root)?;
    let project_root_canonical = canonical_project_root(project_root);
    assign_handles(&mut families, &project_root_canonical, &ontology_refs);

    let idx = Index {
        schema_version: SCHEMA_VERSION,
        generated_at: iso8601_utc_now(),
        config_fingerprint,
        tool_version: env!("CARGO_PKG_VERSION").to_string(),
        sources: new_sources,
        families,
        ontology_refs,
    };

    let summary = UpdateSummary {
        changed: changed_count,
        added: added_count,
        removed: removed.len(),
        unchanged: unchanged_count,
        elapsed_ms: started.elapsed().as_millis(),
        fallback: None,
    };

    Ok(UpdateResult {
        index: idx,
        summary,
    })
}

/// Write an index to disk as pretty JSON, creating the parent directory.
///
/// Writes are atomic: the JSON is first written to `<path>.tmp` and then
/// renamed over `<path>`. If serialization or the temp write fails, the
/// temp file is best-effort removed and the error is propagated; the
/// existing `<path>` is never partially overwritten.
///
/// The binary sidecar cache (`<dir>/index.bincache`, see [`sidecar`]) is
/// updated in the same call **after** the JSON rename succeeds. The
/// sidecar is a build artifact, not source of truth, so any failure to
/// write it is logged via the optional `TAGPATH_SIDECAR_DEBUG` channel
/// and ignored — the next `--update` cycle will fall back to JSON read
/// and regenerate the sidecar. JSON is renamed first so a crash between
/// the two renames leaves JSON authoritative; a stale sidecar that no
/// longer matches JSON is caught on the next read by the embedded sha256
/// and falls back cleanly.
///
/// Use [`write_json_only`] if a caller deliberately wants to skip the
/// sidecar (e.g. a test simulating mid-write crash where only the JSON
/// rename succeeded).
pub fn write(idx: &Index, path: &Path) -> Result<(), String> {
    write_json_only(idx, path)?;
    let side_path = sidecar::sidecar_path_for(path);
    match sidecar::write(idx, &side_path) {
        Ok(()) => sidecar::debug_log("write"),
        Err(e) => {
            // Best-effort: the JSON is authoritative. Surface only via
            // the debug channel so production behaviour stays quiet.
            sidecar::debug_log(&format!("write_failed:{e}"));
            // Make sure no stale sidecar lingers from a previous run.
            let _ = std::fs::remove_file(&side_path);
        }
    }
    Ok(())
}

/// Write only the JSON index; do **not** update the sidecar cache.
///
/// Public for the rare cases (tests simulating a half-committed write
/// pair) that need to bypass the sidecar. Ordinary production callers
/// should use [`write`] instead.
pub fn write_json_only(idx: &Index, path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("create_dir_all({}): {e}", parent.display()))?;
    }
    let mut json =
        serde_json::to_string_pretty(idx).map_err(|e| format!("serialize index: {e}"))?;
    json.push('\n');
    let tmp_path = tmp_path_for(path);
    if let Err(e) = std::fs::write(&tmp_path, json.as_bytes()) {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(format!("write({}): {e}", tmp_path.display()));
    }
    if let Err(e) = std::fs::rename(&tmp_path, path) {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(format!(
            "rename({} -> {}): {e}",
            tmp_path.display(),
            path.display()
        ));
    }
    Ok(())
}

/// Compute the atomic write tmp-path for a target index path.
///
/// Always lives alongside the target file (same directory) so that
/// `rename(2)` stays on the same filesystem.
pub fn tmp_path_for(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_os_string();
    s.push(".tmp");
    PathBuf::from(s)
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
