//! Workspace-level registry for per-project tagpath indexes.
//!
//! `tagpath meta-index <workspace-root>` scans for existing
//! `.naming/index.json` files and writes a top-level
//! `.naming/meta-index.json` registry with workspace-scoped handles.

use crate::{index, parser};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use walkdir::WalkDir;

pub const META_SCHEMA_VERSION: u32 = 1;
pub const META_INDEX_RELATIVE_PATH: &str = ".naming/meta-index.json";

#[derive(Debug, Clone)]
pub struct MetaIndexOptions {
    pub workspace_root: PathBuf,
    pub output_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MetaIndex {
    pub schema_version: u32,
    pub generated_at: String,
    pub tool_version: String,
    pub workspace_root: String,
    pub indexes: Vec<MetaIndexSource>,
    pub families: Vec<MetaFamily>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MetaIndexSource {
    pub project_root: String,
    pub index_path: String,
    pub schema_version: u32,
    pub tool_version: String,
    pub source_count: usize,
    pub family_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MetaFamily {
    pub handle: String,
    pub project_root: String,
    pub project_family_handle: String,
    pub family_id: String,
    pub tags: Vec<String>,
    pub members: Vec<MetaFamilyMember>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MetaFamilyMember {
    pub handle: String,
    pub project_member_handle: String,
    pub name: String,
    pub convention: String,
    pub path: String,
    pub line: usize,
}

pub fn meta_index_path(workspace_root: &Path) -> PathBuf {
    workspace_root.join(META_INDEX_RELATIVE_PATH)
}

pub fn build_meta_index(opts: &MetaIndexOptions) -> Result<MetaIndex, String> {
    let workspace_root = canonical_or_self(&opts.workspace_root);
    let mut index_paths = discover_index_paths(&workspace_root)?;
    index_paths.sort();
    index_paths.dedup();

    let mut sources = Vec::new();
    let mut families = Vec::new();

    for index_path in index_paths {
        let project_root = index_path
            .parent()
            .and_then(Path::parent)
            .ok_or_else(|| format!("invalid index path {}", index_path.display()))?
            .to_path_buf();
        let idx = index::read(&index_path)?;
        let project_rel = relative_path(&project_root, &workspace_root);
        let index_rel = relative_path(&index_path, &workspace_root);

        sources.push(MetaIndexSource {
            project_root: project_rel.clone(),
            index_path: index_rel,
            schema_version: idx.schema_version,
            tool_version: idx.tool_version.clone(),
            source_count: idx.sources.len(),
            family_count: idx.families.len(),
        });

        for family in idx.families {
            let handle = workspace_handle("fam", &project_rel, &family.handle);
            let members = family
                .members
                .into_iter()
                .map(|member| MetaFamilyMember {
                    handle: workspace_handle("mem", &project_rel, &member.handle),
                    project_member_handle: member.handle,
                    name: member.name,
                    convention: member.convention,
                    path: join_workspace_path(&project_rel, &member.path),
                    line: member.line,
                })
                .collect();
            families.push(MetaFamily {
                handle,
                project_root: project_rel.clone(),
                project_family_handle: family.handle,
                family_id: family.family_id,
                tags: family.tags,
                members,
            });
        }
    }

    families.sort_by(|a, b| {
        a.family_id
            .cmp(&b.family_id)
            .then(a.project_root.cmp(&b.project_root))
            .then(a.project_family_handle.cmp(&b.project_family_handle))
    });

    Ok(MetaIndex {
        schema_version: META_SCHEMA_VERSION,
        generated_at: iso8601_utc_now(),
        tool_version: env!("CARGO_PKG_VERSION").to_string(),
        workspace_root: workspace_root.display().to_string(),
        indexes: sources,
        families,
    })
}

pub fn write(meta: &MetaIndex, path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("create_dir_all({}): {e}", parent.display()))?;
    }
    let mut json =
        serde_json::to_string_pretty(meta).map_err(|e| format!("serialize meta-index: {e}"))?;
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

pub fn read(path: &Path) -> Result<MetaIndex, String> {
    let text =
        std::fs::read_to_string(path).map_err(|e| format!("read({}): {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("parse({}): {e}", path.display()))
}

pub fn search_meta_index<'a>(meta: &'a MetaIndex, query: &str) -> Vec<&'a MetaFamily> {
    let conv = parser::detect_convention(query);
    let parsed = parser::parse(query, conv);
    let query_tags: Vec<String> = parsed.tags;
    meta.families
        .iter()
        .filter(|family| {
            query_tags
                .iter()
                .all(|tag| family.tags.iter().any(|candidate| candidate == tag))
        })
        .collect()
}

fn discover_index_paths(workspace_root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut paths = Vec::new();
    for entry in WalkDir::new(workspace_root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| !should_skip(entry.path()))
    {
        let entry = entry.map_err(|e| format!("walk({}): {e}", workspace_root.display()))?;
        if !entry.file_type().is_file() || entry.file_name() != "index.json" {
            continue;
        }
        let path = entry.path();
        if path
            .parent()
            .and_then(Path::file_name)
            .is_some_and(|n| n == ".naming")
        {
            paths.push(path.to_path_buf());
        }
    }
    Ok(paths)
}

fn should_skip(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    matches!(
        name,
        ".git"
            | ".hg"
            | ".svn"
            | ".agent-doc"
            | ".tsift"
            | "target"
            | "node_modules"
            | "__pycache__"
            | ".venv"
            | "vendor"
    )
}

fn workspace_handle(kind: &str, project_rel: &str, project_handle: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(project_rel.as_bytes());
    hasher.update(b"\0");
    hasher.update(project_handle.as_bytes());
    let digest = hasher.finalize();
    format!(
        "meta:{kind}:{:016x}",
        u64::from_be_bytes(digest[0..8].try_into().unwrap())
    )
}

fn canonical_or_self(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn relative_path(path: &Path, root: &Path) -> String {
    let canonical = canonical_or_self(path);
    let relative = canonical.strip_prefix(root).unwrap_or(&canonical);
    if relative.as_os_str().is_empty() {
        ".".to_string()
    } else {
        normalize_path(relative)
    }
}

fn join_workspace_path(project_rel: &str, member_rel: &str) -> String {
    if project_rel == "." {
        member_rel.to_string()
    } else {
        normalize_path(Path::new(project_rel).join(member_rel))
    }
}

fn normalize_path(path: impl AsRef<Path>) -> String {
    path.as_ref()
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn tmp_path_for(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_os_string();
    s.push(".tmp");
    PathBuf::from(s)
}

fn iso8601_utc_now() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format_iso8601_utc(secs)
}

fn format_iso8601_utc(epoch_secs: u64) -> String {
    let days = (epoch_secs / 86_400) as i64;
    let secs_of_day = epoch_secs % 86_400;
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    let year = y + if m <= 2 { 1 } else { 0 };
    let hour = secs_of_day / 3600;
    let min = (secs_of_day % 3600) / 60;
    let sec = secs_of_day % 60;
    format!("{year:04}-{m:02}-{d:02}T{hour:02}:{min:02}:{sec:02}Z")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_handles_include_project_scope() {
        let a = workspace_handle("fam", "crates/a", "fam:1234");
        let b = workspace_handle("fam", "crates/b", "fam:1234");
        assert_ne!(a, b);
        assert!(a.starts_with("meta:fam:"));
    }
}
