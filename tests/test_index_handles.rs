//! Integration tests for stable family/member handles and JSONL emit
//! (tagpath 0.11.0 `#p5tsi`).

use std::io::Write;
use std::path::{Path, PathBuf};

use tagpath::index::{self, BuildOptions, Index};

fn make_project(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tagpath_test_handles_{label}"));
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

fn family_by_id<'a>(idx: &'a Index, family_id: &str) -> &'a index::Family {
    idx.families
        .iter()
        .find(|f| f.family_id == family_id)
        .unwrap_or_else(|| panic!("no family with family_id={family_id}"))
}

#[test]
fn family_handle_is_deterministic_across_two_builds() {
    let root = make_project("det");
    write_naming_toml(&root, default_naming_toml());
    write_source(&root, "src/foo.rs", "fn create_user() {}\n");

    let a = build_at(&root);
    let b = build_at(&root);

    for fam_a in &a.families {
        let fam_b = family_by_id(&b, &fam_a.family_id);
        assert_eq!(fam_a.handle, fam_b.handle);
        assert!(fam_a.handle.starts_with("fam:"));
        assert_eq!(fam_a.handle.len(), "fam:".len() + 16);
    }

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn family_handle_is_stable_when_member_is_added() {
    // Adding a NEW member to a different file must NOT change the
    // existing family's handle. The handle derivation must exclude
    // source paths and member counts.
    let root = make_project("stable_member_add");
    write_naming_toml(&root, default_naming_toml());
    write_source(&root, "src/foo.rs", "fn create_user() {}\n");

    let before = build_at(&root);
    let before_handle = family_by_id(&before, "create_user").handle.clone();

    // Add another file that adds a new member to the same family.
    write_source(&root, "src/bar.rs", "fn create_user() {}\n");

    let after = build_at(&root);
    let after_fam = family_by_id(&after, "create_user");
    assert_eq!(after_fam.handle, before_handle);
    assert!(after_fam.members.len() >= 2);

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn family_handle_changes_when_sorted_tags_change() {
    let root_a = make_project("tags_a");
    write_naming_toml(&root_a, default_naming_toml());
    write_source(&root_a, "src/foo.rs", "fn create_user() {}\n");
    let idx_a = build_at(&root_a);
    let handle_a = family_by_id(&idx_a, "create_user").handle.clone();

    // Use a different project at a different path; build a family whose
    // canonical id and sorted-tag set actually differ from `create_user`.
    let root_b = make_project("tags_b");
    write_naming_toml(&root_b, default_naming_toml());
    write_source(&root_b, "src/foo.rs", "fn create_user_profile() {}\n");
    let idx_b = build_at(&root_b);
    let handle_b = family_by_id(&idx_b, "create_user_profile").handle.clone();

    assert_ne!(handle_a, handle_b);

    let _ = std::fs::remove_dir_all(&root_a);
    let _ = std::fs::remove_dir_all(&root_b);
}

#[test]
fn member_handle_is_stable_across_line_moves_in_same_file() {
    let root = make_project("member_lines");
    write_naming_toml(&root, default_naming_toml());
    write_source(&root, "src/foo.rs", "fn create_user() {}\n");

    let before = build_at(&root);
    let before_member_handle = family_by_id(&before, "create_user")
        .members
        .iter()
        .find(|m| m.name == "create_user" && m.path == "src/foo.rs")
        .map(|m| m.handle.clone())
        .expect("member exists");

    // Push the definition down several lines without changing its name
    // or path. The line number changes; the handle must not.
    write_source(
        &root,
        "src/foo.rs",
        "// header comment\n// another header line\n// and another\nfn create_user() {}\n",
    );
    let after = build_at(&root);
    let after_member = family_by_id(&after, "create_user")
        .members
        .iter()
        .find(|m| m.name == "create_user" && m.path == "src/foo.rs")
        .expect("member exists after edit");
    assert!(after_member.line > 1, "definition was moved down");
    assert_eq!(after_member.handle, before_member_handle);

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn member_handle_changes_on_rename() {
    let root = make_project("member_rename");
    write_naming_toml(&root, default_naming_toml());
    write_source(&root, "src/foo.rs", "fn create_user() {}\n");

    let before = build_at(&root);
    let h_before = family_by_id(&before, "create_user")
        .members
        .iter()
        .find(|m| m.name == "create_user" && m.path == "src/foo.rs")
        .map(|m| m.handle.clone())
        .unwrap();

    write_source(&root, "src/foo.rs", "fn create_user_profile() {}\n");
    let after = build_at(&root);
    let h_after = family_by_id(&after, "create_user_profile")
        .members
        .iter()
        .find(|m| m.name == "create_user_profile" && m.path == "src/foo.rs")
        .map(|m| m.handle.clone())
        .unwrap();

    assert_ne!(h_before, h_after);

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn jsonl_emit_has_header_sources_families_members_footer_in_order() {
    let root = make_project("jsonl_order");
    write_naming_toml(&root, default_naming_toml());
    write_source(
        &root,
        "src/foo.rs",
        "fn create_user() {}\nfn delete_user() {}\n",
    );

    let idx = build_at(&root);
    let mut buf: Vec<u8> = Vec::new();
    index::emit_jsonl(&idx, &mut buf).expect("emit_jsonl");
    let text = String::from_utf8(buf).expect("utf8");

    let mut records: Vec<serde_json::Value> = Vec::new();
    for line in text.lines() {
        assert!(!line.is_empty(), "no blank lines allowed in NDJSON");
        let v: serde_json::Value = serde_json::from_str(line).expect("valid JSON per line");
        records.push(v);
    }

    let types: Vec<String> = records
        .iter()
        .map(|r| r["type"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(types.first().map(String::as_str), Some("header"));
    assert_eq!(types.last().map(String::as_str), Some("footer"));

    // All sources come before any family; all families before any member.
    let mut last_seen: u8 = 0; // 0=header, 1=source, 2=family, 3=member, 4=footer
    let order = |t: &str| -> u8 {
        match t {
            "header" => 0,
            "source" => 1,
            "family" => 2,
            "member" => 3,
            "footer" => 4,
            other => panic!("unexpected record type: {other}"),
        }
    };
    for t in &types {
        let r = order(t);
        assert!(
            r >= last_seen,
            "records out of order: saw {t} after stage {last_seen}",
        );
        last_seen = r;
    }

    let footer = records.last().unwrap();
    let counts = &footer["counts"];
    let n_sources = types.iter().filter(|t| t.as_str() == "source").count();
    let n_families = types.iter().filter(|t| t.as_str() == "family").count();
    let n_members = types.iter().filter(|t| t.as_str() == "member").count();
    assert_eq!(counts["sources"].as_u64().unwrap() as usize, n_sources);
    assert_eq!(counts["families"].as_u64().unwrap() as usize, n_families);
    assert_eq!(counts["members"].as_u64().unwrap() as usize, n_members);

    let header = &records[0];
    assert_eq!(header["schema_version"].as_u64().unwrap(), 2);

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn jsonl_check_emits_stale_records() {
    let root = make_project("jsonl_check_stale");
    write_naming_toml(&root, default_naming_toml());
    write_source(&root, "src/foo.rs", "fn create_user() {}\n");

    // Write a fresh index, then modify the source so the next check is stale.
    let idx = build_at(&root);
    index::write(&idx, &index::index_path(&root)).unwrap();
    write_source(&root, "src/foo.rs", "fn create_user_profile() {}\n");

    let report = index::check(&root).unwrap();
    assert!(!report.fresh);

    let mut buf: Vec<u8> = Vec::new();
    index::emit_jsonl_stale(&root, &report, &mut buf).expect("emit_jsonl_stale");
    let text = String::from_utf8(buf).unwrap();

    let mut types = Vec::new();
    for line in text.lines() {
        let v: serde_json::Value = serde_json::from_str(line).unwrap();
        types.push(v["type"].as_str().unwrap().to_string());
    }
    assert!(types.contains(&"header".to_string()));
    assert!(types.contains(&"stale".to_string()));
    assert!(types.contains(&"footer".to_string()));

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn schema_version_cli_prints_just_the_integer() {
    // Use the installed-from-target binary so we don't need a release build.
    let exe = env!("CARGO_BIN_EXE_tagpath");
    let output = std::process::Command::new(exe)
        .args(["index", "--schema-version"])
        .output()
        .expect("run tagpath");
    assert!(output.status.success(), "non-zero exit");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(stdout, "2\n");
}
