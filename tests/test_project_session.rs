#![cfg(feature = "project-session")]

use std::io::Write;
use std::path::{Path, PathBuf};

use tagpath::index::{self, BuildOptions};
use tagpath::project_session::ProjectSession;

fn make_project(label: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("tagpath_test_project_session_{label}"));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(root.join(".naming/tags")).unwrap();
    std::fs::write(
        root.join(".naming.toml"),
        r#"version = 1
name = "project-session"
convention = "snake_case"

[contexts.function]
convention = "snake_case"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join(".naming/tags/user.md"),
        "# user\n\n---\nsummary: User domain tag.\n---\n",
    )
    .unwrap();
    root
}

fn write_source(root: &Path, contents: &str) {
    let mut file = std::fs::File::create(root.join("src/lib.rs")).unwrap();
    file.write_all(contents.as_bytes()).unwrap();
}

#[test]
fn project_session_derives_families_and_search_from_cells() {
    let root = make_project("families_search");
    write_source(&root, "fn create_user() {}\nfn delete_account() {}\n");

    let session = ProjectSession::new(&root);
    assert_eq!(session.source_files().len(), 1);
    assert!(session.config_state().config.is_some());
    assert!(session.family_map().families.contains_key("create_user"));

    session.set_query("user");
    let hits = session.search_hits();
    assert!(
        hits.iter().any(|hit| hit.member.name == "create_user"),
        "expected create_user hit, got {hits:#?}"
    );

    write_source(&root, "fn create_user() {}\nfn delete_user() {}\n");
    session.refresh();
    let hits = session.search_hits();
    assert!(
        hits.iter().any(|hit| hit.member.name == "delete_user"),
        "refresh should invalidate search slot: {hits:#?}"
    );

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn project_session_exposes_lint_and_sidecar_state() {
    let root = make_project("lint_sidecar");
    write_source(&root, "fn createUser() {}\n");
    let session = ProjectSession::new(&root);

    let findings = session.lint_findings();
    let findings = findings.as_ref().as_ref().expect("lint result");
    assert!(
        findings
            .iter()
            .any(|finding| finding.identifier == "createUser"),
        "expected createUser lint finding, got {findings:#?}"
    );
    assert!(!session.sidecar_state().exists);

    let idx = index::build(&BuildOptions {
        project_root: root.clone(),
    })
    .expect("build index");
    index::write(&idx, &index::index_path(&root)).expect("write index");
    session.refresh();
    let sidecar = session.sidecar_state();
    assert!(sidecar.exists, "sidecar should exist after index write");
    assert!(sidecar.len > 0, "sidecar should have bytes");

    let _ = std::fs::remove_dir_all(&root);
}
