use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_tagpath")
}

fn make_project(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tagpath_test_rename_{label}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_naming_toml(root: &Path) {
    let mut f = std::fs::File::create(root.join(".naming.toml")).unwrap();
    f.write_all(
        br#"version = 1
name = "rename-test"
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

#[test]
fn rename_rewrites_indexed_family_across_files_and_conventions() {
    let root = make_project("cross_convention");
    write_naming_toml(&root);
    write_source(
        &root,
        "src/lib.rs",
        "fn create_user_profile() {}\nconst CREATE_USER_PROFILE: i32 = 1;\nstruct CreateUserProfile;\nfn Create_User_Profile() {}\n",
    );
    write_source(
        &root,
        "src/app.ts",
        "function createUserProfile() { return createUserProfile(); }\n",
    );
    write_source(
        &root,
        "src/styles.css",
        ".create-user-profile { color: red; }\n.create-user-profile:hover { color: blue; }\n",
    );

    let output = Command::new(bin())
        .arg("index")
        .arg(&root)
        .output()
        .expect("run tagpath index");
    assert!(
        output.status.success(),
        "index failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let output = Command::new(bin())
        .args([
            "rename",
            "create_user_profile",
            "update_account_record",
            "--format",
            "json",
        ])
        .arg(&root)
        .output()
        .expect("run tagpath rename");
    assert!(
        output.status.success(),
        "rename failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["old_family_id"], "create_user_profile");
    assert_eq!(json["new_family_id"], "update_account_record");
    assert_eq!(json["files_changed"], 3);
    assert_eq!(json["replacements"], 8);

    let rust = std::fs::read_to_string(root.join("src/lib.rs")).unwrap();
    assert!(rust.contains("fn update_account_record() {}"));
    assert!(rust.contains("const UPDATE_ACCOUNT_RECORD: i32 = 1;"));
    assert!(rust.contains("struct UpdateAccountRecord;"));
    assert!(rust.contains("fn Update_Account_Record() {}"));
    assert!(!rust.contains("create_user_profile"));
    assert!(!rust.contains("CREATE_USER_PROFILE"));
    assert!(!rust.contains("CreateUserProfile"));
    assert!(!rust.contains("Create_User_Profile"));

    let ts = std::fs::read_to_string(root.join("src/app.ts")).unwrap();
    assert!(ts.contains("function updateAccountRecord() { return updateAccountRecord(); }"));
    assert!(!ts.contains("createUserProfile"));

    let css = std::fs::read_to_string(root.join("src/styles.css")).unwrap();
    assert!(css.contains(".update-account-record { color: red; }"));
    assert!(css.contains(".update-account-record:hover { color: blue; }"));
    assert!(!css.contains("create-user-profile"));

    let output = Command::new(bin())
        .args(["index", "--check"])
        .arg(&root)
        .output()
        .expect("run tagpath index --check");
    assert!(
        output.status.success(),
        "index check failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn rename_dry_run_does_not_write_sources_or_index() {
    let root = make_project("dry_run");
    write_naming_toml(&root);
    write_source(&root, "src/lib.rs", "fn create_user_profile() {}\n");

    let output = Command::new(bin())
        .args([
            "rename",
            "create_user_profile",
            "update_account_record",
            "--dry-run",
            "--format",
            "json",
        ])
        .arg(&root)
        .output()
        .expect("run tagpath rename --dry-run");
    assert!(
        output.status.success(),
        "dry-run failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["dry_run"], true);
    assert_eq!(json["replacements"], 1);

    let rust = std::fs::read_to_string(root.join("src/lib.rs")).unwrap();
    assert_eq!(rust, "fn create_user_profile() {}\n");
    assert!(!root.join(".naming/index.json").exists());

    let _ = std::fs::remove_dir_all(&root);
}
