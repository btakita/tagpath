//! Integration tests for `tagpath::index`.

use std::io::Write;
use std::path::{Path, PathBuf};

use tagpath::index::{self, BuildOptions, Index};

/// Build a uniquely-named temp project root, removing any leftover from a prior run.
fn make_project(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tagpath_test_index_{label}"));
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

#[test]
fn build_produces_non_empty_snapshot() {
    let root = make_project("build_nonempty");
    write_naming_toml(&root, default_naming_toml());
    write_source(&root, "src/foo.rs", "fn create_user(name: &str) {}\n");
    write_source(&root, "src/bar.py", "def create_user(name):\n    pass\n");

    let idx = build_at(&root);
    assert_eq!(idx.schema_version, 1);
    assert!(!idx.sources.is_empty(), "sources should not be empty");
    assert!(!idx.families.is_empty(), "families should not be empty");
    assert!(idx.config_fingerprint.starts_with("sha256:"));
    assert_eq!(idx.tool_version, env!("CARGO_PKG_VERSION"));

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn rebuild_is_deterministic_modulo_timestamp() {
    let root = make_project("deterministic");
    write_naming_toml(&root, default_naming_toml());
    write_source(&root, "src/foo.rs", "fn create_user() {}\n");
    write_source(&root, "src/bar.rs", "fn delete_user() {}\n");

    let a = build_at(&root);
    let b = build_at(&root);

    assert_eq!(a.schema_version, b.schema_version);
    assert_eq!(a.config_fingerprint, b.config_fingerprint);
    assert_eq!(a.tool_version, b.tool_version);
    assert_eq!(a.sources, b.sources);
    assert_eq!(a.families, b.families);
    assert_eq!(a.ontology_refs, b.ontology_refs);

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn modifying_a_source_invalidates_index() {
    let root = make_project("source_modified");
    write_naming_toml(&root, default_naming_toml());
    write_source(&root, "src/foo.rs", "fn create_user() {}\n");

    let idx = build_at(&root);
    let idx_path = index::index_path(&root);
    index::write(&idx, &idx_path).unwrap();

    // Fresh check passes.
    let report = index::check(&root).unwrap();
    assert!(
        report.fresh,
        "expected fresh; reasons: {:?}",
        report.stale_reasons
    );

    // Modify the source.
    write_source(&root, "src/foo.rs", "fn create_user_profile() {}\n");

    let report = index::check(&root).unwrap();
    assert!(!report.fresh);
    let has_modified = report
        .stale_reasons
        .iter()
        .any(|r| matches!(r, index::StaleReason::SourceModified { path } if path == "src/foo.rs"));
    assert!(
        has_modified,
        "expected SourceModified for src/foo.rs; got {:?}",
        report.stale_reasons
    );

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn changing_naming_toml_invalidates_via_fingerprint() {
    let root = make_project("config_changed");
    write_naming_toml(&root, default_naming_toml());
    write_source(&root, "src/foo.rs", "fn create_user() {}\n");

    let idx = build_at(&root);
    let idx_path = index::index_path(&root);
    index::write(&idx, &idx_path).unwrap();

    let report = index::check(&root).unwrap();
    assert!(report.fresh);

    // Change the config.
    write_naming_toml(
        &root,
        r#"version = 1
name = "test-project-renamed"
convention = "camelCase"
"#,
    );

    let report = index::check(&root).unwrap();
    assert!(!report.fresh);
    let has_config = report
        .stale_reasons
        .iter()
        .any(|r| matches!(r, index::StaleReason::ConfigChanged));
    assert!(
        has_config,
        "expected ConfigChanged; got {:?}",
        report.stale_reasons
    );

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn force_rebuild_writes_even_when_fresh() {
    let root = make_project("force_rebuild");
    write_naming_toml(&root, default_naming_toml());
    write_source(&root, "src/foo.rs", "fn create_user() {}\n");

    let idx_path = index::index_path(&root);
    let first = build_at(&root);
    index::write(&first, &idx_path).unwrap();
    let first_bytes = std::fs::read(&idx_path).unwrap();

    // Sleep is not required: force rebuild produces a fresh `generated_at`
    // only when seconds tick over. Even so, the rebuild path must succeed
    // and write to disk without erroring, regardless of freshness.
    let report = index::check(&root).unwrap();
    assert!(report.fresh);

    let rebuilt = build_at(&root);
    index::write(&rebuilt, &idx_path).unwrap();
    let after_bytes = std::fs::read(&idx_path).unwrap();

    // Sources/families/fingerprint must match across rebuilds.
    assert_eq!(first.sources, rebuilt.sources);
    assert_eq!(first.families, rebuilt.families);
    assert_eq!(first.config_fingerprint, rebuilt.config_fingerprint);
    // Both writes must produce non-empty output (force never errors).
    assert!(!first_bytes.is_empty());
    assert!(!after_bytes.is_empty());

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn config_fingerprint_is_stable_across_builds_with_hashmap_fields() {
    // Regression: NamingConfig has HashMap-backed fields (patterns, contexts,
    // tags.declared) whose iteration order is non-deterministic. The
    // fingerprint must canonicalize them so successive builds agree.
    let root = make_project("fingerprint_stable");
    write_naming_toml(
        &root,
        r#"version = 1
name = "fp-test"
convention = "snake_case"

[patterns]
factory = "create_{name}"
hook = "use_{name}"
setter = "set_{name}"
signal = "{name}$"

[contexts.function]
convention = "snake_case"

[contexts.type]
convention = "PascalCase"

[contexts.constant]
convention = "UPPER_SNAKE_CASE"

[tags]
open = true

[tags.declared.user]
domain = "auth"

[tags.declared.session]
domain = "agent-doc"
"#,
    );
    write_source(&root, "src/foo.rs", "fn create_user() {}\n");

    let mut fps = std::collections::BTreeSet::new();
    for _ in 0..20 {
        let idx = build_at(&root);
        fps.insert(idx.config_fingerprint);
    }
    assert_eq!(
        fps.len(),
        1,
        "expected stable fingerprint across builds; got {fps:?}"
    );

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn indexed_search_matches_live_search() {
    let root = make_project("indexed_search");
    write_naming_toml(&root, default_naming_toml());
    write_source(
        &root,
        "src/foo.rs",
        "fn create_user() {}\nfn delete_user() {}\nfn create_post() {}\n",
    );

    let idx = build_at(&root);

    let indexed = index::search_index(&idx, "user");
    let indexed_names: std::collections::BTreeSet<String> =
        indexed.iter().map(|m| m.name.clone()).collect();

    let live = tagpath::search::search("user", &root);
    let live_names: std::collections::BTreeSet<String> =
        live.iter().map(|r| r.identifier.clone()).collect();

    assert!(
        indexed_names.contains("create_user"),
        "indexed names: {indexed_names:?}"
    );
    assert!(
        indexed_names.contains("delete_user"),
        "indexed names: {indexed_names:?}"
    );
    assert!(!indexed_names.contains("create_post"));
    // Indexed and live search must agree on the set of matches.
    assert_eq!(indexed_names, live_names);

    let _ = std::fs::remove_dir_all(&root);
}
