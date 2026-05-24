//! Tests for runtime tree-sitter grammar loading (`#p4dy`).
//!
//! These tests cover the error envelopes and the CLI surface. They never
//! actually load a real compiled grammar — that would require building a
//! cdylib in-tree, which is too heavy for this suite.
//!
//! **Known gap:** `LoadError::MissingSymbol` is not exercised end-to-end
//! because doing so cleanly would require shipping or building a real shared
//! object that intentionally omits the symbol. The discriminant is exercised
//! indirectly via `LoadError::DlOpen` paths and via the type system.

#![cfg(feature = "dyn-grammar")]

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;

use tagpath::config;
use tagpath::treesitter::dyn_loader::{self, LoadError};

fn fresh_dir(name: &str) -> PathBuf {
    let mut dir = std::env::temp_dir();
    dir.push(format!("tagpath_dyn_grammar_{name}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn tagpath_bin() -> PathBuf {
    // `CARGO_BIN_EXE_<name>` is set by Cargo for integration tests of binary
    // crates; it points at the freshly built test binary.
    PathBuf::from(env!("CARGO_BIN_EXE_tagpath"))
}

#[test]
fn missing_file_error_includes_path() {
    let dir = fresh_dir("missing");
    let result = dyn_loader::load_path(
        "ghost",
        &dir.join("nope.so"),
        None,
        vec!["ghost".to_string()],
    );
    let err = result.expect_err("expected MissingFile error");
    match err {
        LoadError::MissingFile { ref path } => {
            assert!(path.to_string_lossy().ends_with("nope.so"));
        }
        other => panic!("expected MissingFile, got {other:?}"),
    }
    let msg = err.to_string();
    assert!(msg.contains("file not found"));
    assert!(msg.contains("nope.so"));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn dlopen_error_for_non_shared_object_file() {
    let dir = fresh_dir("dlopen");
    let path = dir.join("definitely-not-a-grammar.so");
    {
        let mut f = fs::File::create(&path).unwrap();
        writeln!(f, "this is plain text, not ELF/Mach-O/PE").unwrap();
    }
    let result = dyn_loader::load_path("fake", &path, None, vec![]);
    let err = result.expect_err("expected DlOpen error");
    assert!(
        matches!(err, LoadError::DlOpen { .. }),
        "expected DlOpen, got {err:?}"
    );
    let msg = err.to_string();
    assert!(msg.contains("failed to open"));
    assert!(msg.contains("definitely-not-a-grammar.so"));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn discover_returns_candidates_for_each_platform_suffix() {
    let dir = fresh_dir("discover");
    for name in &[
        "libtree-sitter-alpha.so",
        "tree-sitter-beta.dylib",
        "tree_sitter_gamma.dll",
        "ignored.txt",
        "no-extension",
    ] {
        fs::File::create(dir.join(name)).unwrap();
    }
    let discovered = dyn_loader::discover(std::slice::from_ref(&dir));
    let mut languages: Vec<_> = discovered.iter().map(|d| d.language.clone()).collect();
    languages.sort();
    assert_eq!(languages, vec!["alpha", "beta", "gamma"]);
    let alpha = discovered.iter().find(|d| d.language == "alpha").unwrap();
    assert_eq!(alpha.symbol, "tree_sitter_alpha");
    assert!(alpha.path.is_absolute() || alpha.path.exists());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn discover_skips_missing_dirs() {
    let dir = fresh_dir("discover_missing");
    let _ = fs::remove_dir_all(&dir);
    // The directory no longer exists; discover should silently skip.
    let discovered = dyn_loader::discover(&[dir.clone(), std::env::temp_dir()]);
    // The temp dir may or may not contain shared libs; just confirm no panic.
    let _ = discovered;
}

#[test]
fn grammars_config_path_expansion() {
    let base = std::env::temp_dir();
    let absolute = config::expand_grammar_path(&PathBuf::from("/etc/foo.so"), &base);
    assert_eq!(absolute, PathBuf::from("/etc/foo.so"));
    let relative = config::expand_grammar_path(&PathBuf::from("./g/foo.so"), &base);
    assert_eq!(relative, base.join("./g/foo.so"));
    if let Some(home) = std::env::var_os("HOME") {
        let expanded = config::expand_grammar_path(&PathBuf::from("~/g/foo.so"), &base);
        let mut want = PathBuf::from(home);
        want.push("g/foo.so");
        assert_eq!(expanded, want);
    }
}

#[test]
fn grammars_list_cli_prints_configured_entries() {
    let dir = fresh_dir("cli_list");
    let bin = tagpath_bin();
    if !bin.exists() {
        eprintln!("skip: tagpath binary not built yet at {}", bin.display());
        return;
    }
    fs::create_dir_all(dir.join("grammars")).unwrap();
    // Sham grammar file — the load will fail, but `list` should still print
    // the configured entry (with an error status).
    fs::File::create(dir.join("grammars/tree-sitter-fake.so")).unwrap();
    let mut f = fs::File::create(dir.join(".naming.toml")).unwrap();
    write!(
        f,
        r#"version = 1
name = "cli-list"
convention = "snake_case"

[grammars]
load_dirs = ["./grammars"]

[grammars.languages.fake]
path = "./grammars/tree-sitter-fake.so"
extensions = ["fke"]
"#
    )
    .unwrap();
    drop(f);
    let output = Command::new(&bin)
        .args(["grammars", "list"])
        .current_dir(&dir)
        .output()
        .expect("failed to invoke tagpath binary");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "expected success; stdout={stdout}\nstderr={stderr}"
    );
    assert!(
        stdout.contains("Configured grammars:"),
        "stdout missing header: {stdout}"
    );
    assert!(
        stdout.contains("fake"),
        "stdout missing fake language: {stdout}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn grammars_check_cli_exits_nonzero_on_missing_file() {
    let dir = fresh_dir("cli_check");
    let bin = tagpath_bin();
    if !bin.exists() {
        eprintln!("skip: tagpath binary not built yet at {}", bin.display());
        return;
    }
    let mut f = fs::File::create(dir.join(".naming.toml")).unwrap();
    write!(
        f,
        r#"version = 1
name = "cli-check"
convention = "snake_case"

[grammars.languages.missing]
path = "./definitely/missing.so"
extensions = ["mis"]
"#
    )
    .unwrap();
    drop(f);
    let output = Command::new(&bin)
        .args(["grammars", "check"])
        .current_dir(&dir)
        .output()
        .expect("failed to invoke tagpath binary");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "expected non-zero exit; stderr={stderr}"
    );
    assert!(stderr.contains("file not found"), "stderr: {stderr}");
    let _ = fs::remove_dir_all(&dir);
}
