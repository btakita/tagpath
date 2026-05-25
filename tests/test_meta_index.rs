//! Integration tests for workspace meta-index aggregation.

use std::io::Write;
use std::path::{Path, PathBuf};

use tagpath::index::{self, BuildOptions};
use tagpath::meta_index::{self, MetaIndexOptions};

fn make_workspace(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tagpath_test_meta_index_{label}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_naming_toml(root: &Path) {
    let mut f = std::fs::File::create(root.join(".naming.toml")).unwrap();
    f.write_all(
        br#"version = 1
name = "test-project"
convention = "snake_case"
"#,
    )
    .unwrap();
}

fn write_source(root: &Path, rel: &str, contents: &str) {
    let abs = root.join(rel);
    if let Some(parent) = abs.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    let mut f = std::fs::File::create(&abs).unwrap();
    f.write_all(contents.as_bytes()).unwrap();
}

fn build_project_index(root: &Path) {
    write_naming_toml(root);
    let idx = index::build(&BuildOptions {
        project_root: root.to_path_buf(),
    })
    .unwrap();
    index::write(&idx, &index::index_path(root)).unwrap();
}

#[test]
fn aggregates_sibling_project_indexes() {
    let workspace = make_workspace("siblings");
    let a = workspace.join("crates/a");
    let b = workspace.join("crates/b");
    std::fs::create_dir_all(&a).unwrap();
    std::fs::create_dir_all(&b).unwrap();
    write_source(&a, "src/lib.rs", "fn create_user() {}\n");
    write_source(&b, "src/lib.rs", "fn create_user() {}\n");
    build_project_index(&a);
    build_project_index(&b);

    let meta = meta_index::build_meta_index(&MetaIndexOptions {
        workspace_root: workspace.clone(),
        output_path: None,
    })
    .unwrap();

    assert_eq!(meta.schema_version, 1);
    assert_eq!(meta.indexes.len(), 2);
    assert_eq!(meta.families.len(), 2);

    let hits = meta_index::search_meta_index(&meta, "create_user");
    assert_eq!(hits.len(), 2);
    assert!(hits.iter().any(|family| family.project_root == "crates/a"));
    assert!(hits.iter().any(|family| family.project_root == "crates/b"));
    assert_ne!(hits[0].handle, hits[1].handle);
    assert!(
        hits.iter()
            .all(|family| family.members[0].path.ends_with("src/lib.rs"))
    );

    let _ = std::fs::remove_dir_all(&workspace);
}

#[test]
fn writes_and_reads_default_meta_index_path() {
    let workspace = make_workspace("write_read");
    let project = workspace.join("packages/app");
    std::fs::create_dir_all(&project).unwrap();
    write_source(&project, "src/main.py", "def delete_user():\n    pass\n");
    build_project_index(&project);

    let meta = meta_index::build_meta_index(&MetaIndexOptions {
        workspace_root: workspace.clone(),
        output_path: None,
    })
    .unwrap();
    let path = meta_index::meta_index_path(&workspace);
    meta_index::write(&meta, &path).unwrap();
    let reloaded = meta_index::read(&path).unwrap();

    assert_eq!(reloaded.indexes.len(), 1);
    let hits = meta_index::search_meta_index(&reloaded, "delete_user");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].project_root, "packages/app");
    assert_eq!(hits[0].members[0].path, "packages/app/src/main.py");

    let _ = std::fs::remove_dir_all(&workspace);
}
