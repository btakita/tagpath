//! Integration tests for `tagpath mcp install`.
//!
//! All tests use `--config-dir-override` to redirect path resolution to a
//! temp directory; the user's real Claude Desktop / Codex / etc config is
//! never touched.

#![cfg(feature = "mcp")]

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

fn make_temp(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tagpath_test_mcp_install_{label}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_tagpath")
}

fn run(args: &[&str]) -> (i32, String, String) {
    let output = Command::new(bin())
        .args(args)
        .output()
        .expect("spawn tagpath");
    let code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8(output.stdout).expect("stdout utf8");
    let stderr = String::from_utf8(output.stderr).expect("stderr utf8");
    (code, stdout, stderr)
}

fn run_ok(args: &[&str]) -> String {
    let (code, stdout, stderr) = run(args);
    assert_eq!(code, 0, "tagpath {args:?} failed:\nstderr={stderr}");
    stdout
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

#[test]
fn list_prints_all_five_harnesses() {
    let dir = make_temp("list");
    let stdout = run_ok(&[
        "mcp",
        "install",
        "--list",
        "--config-dir-override",
        dir.to_str().unwrap(),
    ]);
    for name in [
        "claude-desktop",
        "claude-code",
        "codex",
        "opencode",
        "cursor",
    ] {
        assert!(
            stdout.contains(name),
            "expected harness `{name}` in --list output:\n{stdout}"
        );
    }
}

#[test]
fn print_claude_desktop_emits_valid_json() {
    let dir = make_temp("print_claude_desktop");
    let stdout = run_ok(&[
        "mcp",
        "install",
        "--print",
        "claude-desktop",
        "--config-dir-override",
        dir.to_str().unwrap(),
    ]);
    let value: Value = serde_json::from_str(&stdout).expect("valid JSON");
    let entry = value
        .pointer("/mcpServers/tagpath")
        .expect("tagpath entry under mcpServers");
    assert_eq!(
        entry.get("command").and_then(Value::as_str),
        Some("tagpath")
    );
    let args = entry.get("args").and_then(Value::as_array).unwrap();
    assert_eq!(args.len(), 1);
    assert_eq!(args[0].as_str(), Some("mcp"));
}

#[test]
fn print_codex_emits_valid_toml() {
    let dir = make_temp("print_codex");
    let stdout = run_ok(&[
        "mcp",
        "install",
        "--print",
        "codex",
        "--config-dir-override",
        dir.to_str().unwrap(),
    ]);
    let parsed: toml::Table = stdout.parse().expect("valid TOML");
    let entry = parsed
        .get("mcp_servers")
        .and_then(toml::Value::as_table)
        .and_then(|t| t.get("tagpath"))
        .and_then(toml::Value::as_table)
        .expect("[mcp_servers.tagpath]");
    assert_eq!(
        entry.get("command").and_then(toml::Value::as_str),
        Some("tagpath")
    );
    let args = entry.get("args").and_then(toml::Value::as_array).unwrap();
    assert_eq!(args.len(), 1);
    assert_eq!(args[0].as_str(), Some("mcp"));
}

#[test]
fn print_opencode_emits_mcp_tagpath_with_type_local() {
    let dir = make_temp("print_opencode");
    let stdout = run_ok(&[
        "mcp",
        "install",
        "--print",
        "opencode",
        "--config-dir-override",
        dir.to_str().unwrap(),
    ]);
    let value: Value = serde_json::from_str(&stdout).unwrap();
    let entry = value.pointer("/mcp/tagpath").expect("/mcp/tagpath");
    assert_eq!(entry.get("type").and_then(Value::as_str), Some("local"));
    assert_eq!(
        entry.get("command").and_then(Value::as_str),
        Some("tagpath")
    );
}

#[test]
fn apply_writes_file_and_honors_custom_binary() {
    let dir = make_temp("apply_claude_desktop");
    let (code, _stdout, stderr) = run(&[
        "mcp",
        "install",
        "--apply",
        "claude-desktop",
        "--yes",
        "--binary",
        "/custom/path/tagpath",
        "--config-dir-override",
        dir.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "apply failed:\nstderr={stderr}");

    let path = dir.join("Claude").join("claude_desktop_config.json");
    assert!(path.exists(), "expected config at {}", path.display());
    let value: Value = serde_json::from_str(&read(&path)).unwrap();
    assert_eq!(
        value
            .pointer("/mcpServers/tagpath/command")
            .and_then(Value::as_str),
        Some("/custom/path/tagpath")
    );
}

#[test]
fn apply_merges_with_existing_servers_preserving_unrelated_keys() {
    let dir = make_temp("apply_merge");
    let target = dir.join("Claude").join("claude_desktop_config.json");
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();

    let existing = serde_json::json!({
        "version": "1.0",
        "mcpServers": {
            "other-server": {
                "command": "other",
                "args": ["serve"]
            }
        },
        "telemetry": false
    });
    std::fs::write(&target, serde_json::to_string_pretty(&existing).unwrap()).unwrap();

    let (code, _stdout, stderr) = run(&[
        "mcp",
        "install",
        "--apply",
        "claude-desktop",
        "--yes",
        "--config-dir-override",
        dir.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "apply failed:\nstderr={stderr}");

    let value: Value = serde_json::from_str(&read(&target)).unwrap();
    assert_eq!(value.get("version").and_then(Value::as_str), Some("1.0"));
    assert_eq!(value.get("telemetry").and_then(Value::as_bool), Some(false));
    let servers = value
        .get("mcpServers")
        .and_then(Value::as_object)
        .expect("mcpServers map");
    assert!(servers.contains_key("other-server"));
    assert!(servers.contains_key("tagpath"));
    // Other server's args preserved
    assert_eq!(
        value
            .pointer("/mcpServers/other-server/command")
            .and_then(Value::as_str),
        Some("other")
    );
}

#[test]
fn apply_codex_merges_toml_preserving_unrelated_keys() {
    let dir = make_temp("apply_codex_merge");
    let target = dir.join(".codex").join("config.toml");
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();

    let existing = r#"
model = "gpt-5"

[mcp_servers.other-server]
command = "other"
args = ["serve"]
"#;
    std::fs::write(&target, existing).unwrap();

    let (code, _stdout, stderr) = run(&[
        "mcp",
        "install",
        "--apply",
        "codex",
        "--yes",
        "--config-dir-override",
        dir.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "apply codex failed:\nstderr={stderr}");

    let parsed: toml::Table = read(&target).parse().unwrap();
    assert_eq!(
        parsed.get("model").and_then(toml::Value::as_str),
        Some("gpt-5")
    );
    let servers = parsed
        .get("mcp_servers")
        .and_then(toml::Value::as_table)
        .unwrap();
    assert!(servers.contains_key("other-server"));
    assert!(servers.contains_key("tagpath"));
}

#[test]
fn uninstall_removes_tagpath_and_preserves_other_keys() {
    let dir = make_temp("uninstall");
    let target = dir.join("Claude").join("claude_desktop_config.json");
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();

    let seed = serde_json::json!({
        "mcpServers": {
            "tagpath": { "command": "tagpath", "args": ["mcp"] },
            "other-server": { "command": "other" }
        },
        "version": "1.0"
    });
    std::fs::write(&target, serde_json::to_string_pretty(&seed).unwrap()).unwrap();

    let (code, _stdout, stderr) = run(&[
        "mcp",
        "install",
        "--uninstall",
        "claude-desktop",
        "--yes",
        "--config-dir-override",
        dir.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "uninstall failed:\nstderr={stderr}");

    let value: Value = serde_json::from_str(&read(&target)).unwrap();
    let servers = value
        .get("mcpServers")
        .and_then(Value::as_object)
        .expect("mcpServers preserved");
    assert!(!servers.contains_key("tagpath"));
    assert!(servers.contains_key("other-server"));
    assert_eq!(value.get("version").and_then(Value::as_str), Some("1.0"));
}

#[test]
fn apply_twice_is_idempotent() {
    let dir = make_temp("idempotent");
    let args = &[
        "mcp",
        "install",
        "--apply",
        "claude-desktop",
        "--yes",
        "--config-dir-override",
        dir.to_str().unwrap(),
    ];
    let (code1, _, e1) = run(args);
    assert_eq!(code1, 0, "first apply: {e1}");
    let path = dir.join("Claude").join("claude_desktop_config.json");
    let first = read(&path);
    let (code2, _, e2) = run(args);
    assert_eq!(code2, 0, "second apply: {e2}");
    let second = read(&path);
    assert_eq!(first, second, "two applies should produce identical output");

    // And the file should still contain exactly one tagpath entry.
    let value: Value = serde_json::from_str(&first).unwrap();
    let servers = value.get("mcpServers").and_then(Value::as_object).unwrap();
    assert_eq!(
        servers.keys().filter(|k| k.as_str() == "tagpath").count(),
        1
    );
}

#[test]
fn apply_claude_code_project_scope_writes_under_project() {
    let dir = make_temp("claude_code_project");
    let project = dir.join("my-proj");
    std::fs::create_dir_all(&project).unwrap();

    let (code, _stdout, stderr) = run(&[
        "mcp",
        "install",
        "--apply",
        "claude-code",
        "--yes",
        "--project",
        project.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "apply failed:\nstderr={stderr}");

    let target = project.join(".claude").join("settings.json");
    assert!(target.exists(), "expected {}", target.display());
    let value: Value = serde_json::from_str(&read(&target)).unwrap();
    assert_eq!(
        value
            .pointer("/mcpServers/tagpath/command")
            .and_then(Value::as_str),
        Some("tagpath")
    );
}

#[test]
fn apply_without_yes_is_dry_run() {
    let dir = make_temp("dry_run");
    let (code, stdout, stderr) = run(&[
        "mcp",
        "install",
        "--apply",
        "claude-desktop",
        "--config-dir-override",
        dir.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "dry-run failed:\nstderr={stderr}");
    assert!(stderr.contains("dry-run"));
    // Preview JSON should be on stdout.
    let value: Value = serde_json::from_str(&stdout).expect("preview is JSON");
    assert!(value.pointer("/mcpServers/tagpath").is_some());
    // File should NOT have been written.
    let path = dir.join("Claude").join("claude_desktop_config.json");
    assert!(
        !path.exists(),
        "dry-run should not write {}",
        path.display()
    );
}

#[test]
fn unknown_harness_exits_nonzero() {
    let dir = make_temp("unknown");
    let (code, _stdout, stderr) = run(&[
        "mcp",
        "install",
        "--print",
        "ghost-harness",
        "--config-dir-override",
        dir.to_str().unwrap(),
    ]);
    assert_ne!(code, 0);
    assert!(stderr.contains("unknown harness"));
}
