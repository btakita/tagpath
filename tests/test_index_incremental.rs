//! Integration tests for `tagpath::index::update_incremental` (#p5inc).
//!
//! These tests pin the incremental update contract:
//!
//! - byte-identical result vs full rebuild (modulo `generated_at`)
//! - per-change classification (`changed`, `added`, `removed`, `unchanged`,
//!   `MtimeOnly`)
//! - fallback-to-full on schema / config / missing-index guards
//! - atomic writes via `.tmp + rename` (verified by inspecting `tmp_path_for`
//!   layout — the binary always writes through that path).
//! - `update_plan` JSONL record shape

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use tagpath::index::{self, BuildOptions, FallbackReason, Index, UpdateResult};

// ---------- harness ----------

fn make_project(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tagpath_test_inc_{label}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_naming_toml(root: &Path, contents: &str) {
    let mut f = std::fs::File::create(root.join(".naming.toml")).unwrap();
    f.write_all(contents.as_bytes()).unwrap();
}

fn default_naming_toml() -> &'static str {
    r#"version = 1
name = "test-project"
convention = "snake_case"
"#
}

fn write_source(root: &Path, rel: &str, contents: &str) {
    let abs = root.join(rel);
    if let Some(parent) = abs.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    let mut f = std::fs::File::create(&abs).unwrap();
    f.write_all(contents.as_bytes()).unwrap();
}

fn build_at(root: &Path) -> Index {
    let opts = BuildOptions {
        project_root: root.to_path_buf(),
    };
    index::build(&opts).expect("build")
}

fn update_at(root: &Path) -> UpdateResult {
    let opts = BuildOptions {
        project_root: root.to_path_buf(),
    };
    index::update_incremental(&opts).expect("update_incremental")
}

/// Compare two indexes ignoring `generated_at`.
fn assert_index_eq_modulo_time(a: &Index, b: &Index) {
    assert_eq!(a.schema_version, b.schema_version);
    assert_eq!(a.config_fingerprint, b.config_fingerprint);
    assert_eq!(a.tool_version, b.tool_version);
    assert_eq!(a.sources, b.sources, "sources differ");
    assert_eq!(a.families, b.families, "families differ");
    assert_eq!(a.ontology_refs, b.ontology_refs, "ontology_refs differ");
}

/// Set the mtime+atime of a file to `secs` since UNIX_EPOCH using `filetime`-style
/// `set_file_times`. We use plain `utimensat` via `std::fs::File` + `set_modified`
/// (Rust 1.75+, stable).
fn set_mtime(path: &Path, secs: u64) {
    let f = std::fs::OpenOptions::new()
        .write(true)
        .open(path)
        .expect("open for mtime");
    let when = SystemTime::UNIX_EPOCH + Duration::from_secs(secs);
    f.set_modified(when).expect("set_modified");
}

// ---------- tests ----------

#[test]
fn no_changes_yields_zero_zero_zero_n() {
    let root = make_project("no_changes");
    write_naming_toml(&root, default_naming_toml());
    write_source(&root, "src/foo.rs", "fn create_user() {}\n");
    write_source(&root, "src/bar.rs", "fn delete_user() {}\n");

    let initial = build_at(&root);
    let idx_path = index::index_path(&root);
    index::write(&initial, &idx_path).unwrap();

    let result = update_at(&root);
    assert!(result.summary.fallback.is_none());
    assert_eq!(result.summary.changed, 0);
    assert_eq!(result.summary.added, 0);
    assert_eq!(result.summary.removed, 0);
    assert_eq!(result.summary.unchanged, initial.sources.len());
    assert_index_eq_modulo_time(&initial, &result.index);

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn one_file_changed_matches_full_rebuild() {
    let root = make_project("one_changed");
    write_naming_toml(&root, default_naming_toml());
    write_source(&root, "src/foo.rs", "fn create_user() {}\n");
    write_source(&root, "src/bar.rs", "fn delete_user() {}\n");
    write_source(&root, "src/baz.rs", "fn fetch_post() {}\n");

    let initial = build_at(&root);
    let idx_path = index::index_path(&root);
    index::write(&initial, &idx_path).unwrap();

    // Modify one file (different content + different size guarantees change).
    write_source(&root, "src/foo.rs", "fn create_user_profile() {}\n");

    let inc = update_at(&root);
    assert!(inc.summary.fallback.is_none());
    assert_eq!(inc.summary.changed, 1, "summary: {:?}", inc.summary);
    assert_eq!(inc.summary.added, 0);
    assert_eq!(inc.summary.removed, 0);
    assert_eq!(inc.summary.unchanged, 2);

    let full = build_at(&root);
    assert_index_eq_modulo_time(&inc.index, &full);

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn one_file_added_matches_full_rebuild() {
    let root = make_project("one_added");
    write_naming_toml(&root, default_naming_toml());
    write_source(&root, "src/foo.rs", "fn create_user() {}\n");

    let initial = build_at(&root);
    let idx_path = index::index_path(&root);
    index::write(&initial, &idx_path).unwrap();

    write_source(&root, "src/bar.rs", "fn delete_user() {}\n");

    let inc = update_at(&root);
    assert!(inc.summary.fallback.is_none());
    assert_eq!(inc.summary.added, 1);
    assert_eq!(inc.summary.changed, 0);
    assert_eq!(inc.summary.removed, 0);
    assert_eq!(inc.summary.unchanged, 1);

    let full = build_at(&root);
    assert_index_eq_modulo_time(&inc.index, &full);

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn one_file_removed_matches_full_rebuild() {
    let root = make_project("one_removed");
    write_naming_toml(&root, default_naming_toml());
    write_source(&root, "src/foo.rs", "fn create_user() {}\n");
    write_source(&root, "src/bar.rs", "fn delete_user() {}\n");

    let initial = build_at(&root);
    let idx_path = index::index_path(&root);
    index::write(&initial, &idx_path).unwrap();

    std::fs::remove_file(root.join("src/bar.rs")).unwrap();

    let inc = update_at(&root);
    assert!(inc.summary.fallback.is_none());
    assert_eq!(inc.summary.removed, 1);
    assert_eq!(inc.summary.changed, 0);
    assert_eq!(inc.summary.added, 0);
    assert_eq!(inc.summary.unchanged, 1);

    let full = build_at(&root);
    assert_index_eq_modulo_time(&inc.index, &full);

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn touch_only_mtime_no_reextract() {
    let root = make_project("touch_mtime");
    write_naming_toml(&root, default_naming_toml());
    write_source(&root, "src/foo.rs", "fn create_user() {}\n");
    write_source(&root, "src/bar.rs", "fn delete_user() {}\n");

    let initial = build_at(&root);
    let idx_path = index::index_path(&root);
    index::write(&initial, &idx_path).unwrap();

    // Force the on-disk file's mtime forward so it differs from the cached
    // entry, but keep contents byte-identical so the hash is unchanged.
    let old = initial
        .sources
        .iter()
        .find(|s| s.path == "src/foo.rs")
        .unwrap()
        .mtime;
    set_mtime(&root.join("src/foo.rs"), old + 1000);

    let inc = update_at(&root);
    assert!(inc.summary.fallback.is_none());
    assert_eq!(inc.summary.changed, 0);
    assert_eq!(inc.summary.added, 0);
    assert_eq!(inc.summary.removed, 0);
    assert_eq!(inc.summary.unchanged, 2);

    // Member content must be identical to a full rebuild.
    let full = build_at(&root);
    assert_eq!(inc.index.families, full.families);

    // mtime on the updated entry must reflect the new on-disk mtime.
    let new_entry = inc
        .index
        .sources
        .iter()
        .find(|s| s.path == "src/foo.rs")
        .unwrap();
    assert!(new_entry.mtime >= old + 1000);

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn config_change_falls_back_to_full() {
    let root = make_project("cfg_fallback");
    write_naming_toml(&root, default_naming_toml());
    write_source(&root, "src/foo.rs", "fn create_user() {}\n");

    let initial = build_at(&root);
    let idx_path = index::index_path(&root);
    index::write(&initial, &idx_path).unwrap();

    write_naming_toml(
        &root,
        r#"version = 1
name = "renamed-project"
convention = "camelCase"
"#,
    );

    let inc = update_at(&root);
    assert!(matches!(
        inc.summary.fallback,
        Some(FallbackReason::ConfigFingerprintMismatch)
    ));
    let full = build_at(&root);
    assert_index_eq_modulo_time(&inc.index, &full);

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn schema_mismatch_falls_back_to_full() {
    let root = make_project("schema_fallback");
    write_naming_toml(&root, default_naming_toml());
    write_source(&root, "src/foo.rs", "fn create_user() {}\n");

    // Write a fake on-disk index with schema_version = 1 (older than 2).
    let idx_path = index::index_path(&root);
    std::fs::create_dir_all(idx_path.parent().unwrap()).unwrap();
    let fake = serde_json::json!({
        "schema_version": 1,
        "generated_at": "1970-01-01T00:00:00Z",
        "config_fingerprint": "sha256:deadbeef",
        "tool_version": "0.0.0",
        "sources": [],
        "families": [],
        "ontology_refs": [],
    });
    std::fs::write(&idx_path, serde_json::to_string_pretty(&fake).unwrap()).unwrap();

    let inc = update_at(&root);
    assert!(
        matches!(
            inc.summary.fallback,
            Some(FallbackReason::SchemaVersionMismatch {
                found: 1,
                expected: 2
            })
        ),
        "got: {:?}",
        inc.summary.fallback
    );
    // Result must equal a full rebuild.
    let full = build_at(&root);
    assert_index_eq_modulo_time(&inc.index, &full);

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn missing_index_falls_back_to_full() {
    let root = make_project("missing_idx");
    write_naming_toml(&root, default_naming_toml());
    write_source(&root, "src/foo.rs", "fn create_user() {}\n");

    let inc = update_at(&root);
    assert!(matches!(
        inc.summary.fallback,
        Some(FallbackReason::IndexMissing)
    ));
    let full = build_at(&root);
    assert_index_eq_modulo_time(&inc.index, &full);

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn atomic_write_via_tmp_then_rename() {
    let root = make_project("atomic_write");
    write_naming_toml(&root, default_naming_toml());
    write_source(&root, "src/foo.rs", "fn create_user() {}\n");

    let idx_path = index::index_path(&root);
    let tmp_path = index::tmp_path_for(&idx_path);

    // tmp_path must live alongside the target so rename(2) stays on the
    // same filesystem.
    assert_eq!(tmp_path.parent(), idx_path.parent());
    assert_eq!(
        tmp_path.file_name().and_then(|f| f.to_str()),
        Some("index.json.tmp")
    );

    // After a successful write, the tmp file must not exist (it was renamed).
    let idx = build_at(&root);
    index::write(&idx, &idx_path).unwrap();
    assert!(idx_path.exists(), "target must exist after write");
    assert!(
        !tmp_path.exists(),
        "tmp file must not linger after successful write"
    );

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn update_plan_jsonl_first_line_has_correct_counts() {
    let root = make_project("jsonl_plan");
    write_naming_toml(&root, default_naming_toml());
    write_source(&root, "src/foo.rs", "fn create_user() {}\n");
    write_source(&root, "src/bar.rs", "fn delete_user() {}\n");

    let initial = build_at(&root);
    let idx_path = index::index_path(&root);
    index::write(&initial, &idx_path).unwrap();

    write_source(&root, "src/foo.rs", "fn create_user_profile() {}\n");

    let inc = update_at(&root);
    let plan = inc.summary.plan_line();
    assert_eq!(plan["type"], "update_plan");
    assert_eq!(plan["changed"], 1);
    assert_eq!(plan["added"], 0);
    assert_eq!(plan["removed"], 0);
    assert_eq!(plan["unchanged"], 1);

    let _ = std::fs::remove_dir_all(&root);
}
