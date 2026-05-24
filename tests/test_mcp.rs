//! Integration tests for `tagpath mcp` (stdio JSON-RPC server).
//!
//! Each test spawns the compiled `tagpath` binary with the `mcp` subcommand,
//! writes one or more request lines to stdin, closes stdin, waits for the
//! child to exit, then parses every line of stdout as JSON-RPC responses.

#![cfg(feature = "mcp")]

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::Value;

/// Build a uniquely-named temp project root, removing any leftover from a prior run.
fn make_project(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tagpath_test_mcp_{label}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_naming_toml(root: &Path) {
    let mut f = std::fs::File::create(root.join(".naming.toml")).unwrap();
    f.write_all(
        br#"version = 1
name = "mcp-fixture"
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

/// Spawn the MCP server, send `requests` (one JSON object per line), then
/// return the parsed response lines.
fn run_mcp(requests: &[&str]) -> Vec<Value> {
    let bin = env!("CARGO_BIN_EXE_tagpath");
    let mut child = Command::new(bin)
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn tagpath mcp");

    {
        let stdin = child.stdin.as_mut().expect("stdin");
        for req in requests {
            stdin.write_all(req.as_bytes()).unwrap();
            stdin.write_all(b"\n").unwrap();
        }
    }
    // Drop stdin to close it.
    drop(child.stdin.take());

    let output = child.wait_with_output().expect("wait_with_output");
    assert!(
        output.status.success(),
        "tagpath mcp exited non-zero: {:?}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );

    String::from_utf8(output.stdout)
        .expect("stdout utf8")
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str::<Value>(l).expect("response is JSON"))
        .collect()
}

#[test]
fn initialize_returns_expected_protocol_version() {
    let responses = run_mcp(&[r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#]);
    assert_eq!(responses.len(), 1);
    let result = &responses[0]["result"];
    assert_eq!(result["protocolVersion"], "2024-11-05");
    assert!(result["capabilities"]["tools"].is_object());
    assert_eq!(result["serverInfo"]["name"], "tagpath");
    assert!(result["serverInfo"]["version"].is_string());
}

#[test]
fn tools_list_returns_all_six_tools_with_schemas() {
    let responses = run_mcp(&[r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#]);
    assert_eq!(responses.len(), 1);
    let tools = responses[0]["result"]["tools"]
        .as_array()
        .expect("tools array");
    assert_eq!(tools.len(), 6, "expected 6 tools, got {}", tools.len());
    let expected = [
        "parse",
        "normalize_query",
        "lint",
        "search",
        "ontology_lookup",
        "indexed_project_query",
    ];
    for tool in tools {
        let name = tool["name"].as_str().unwrap();
        assert!(expected.contains(&name), "unexpected tool {name}");
        assert!(
            tool["description"].as_str().is_some_and(|s| !s.is_empty()),
            "tool {name} has empty description"
        );
        let schema = &tool["inputSchema"];
        assert_eq!(schema["type"], "object");
        assert!(
            schema["properties"]
                .as_object()
                .is_some_and(|p| !p.is_empty()),
            "tool {name} has empty properties"
        );
    }
}

#[test]
fn tools_call_parse_returns_camel_case_create_user() {
    let req = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"parse","arguments":{"name":"createUser"}}}"#;
    let responses = run_mcp(&[req]);
    let content = &responses[0]["result"]["content"][0]["text"];
    let payload: Value = serde_json::from_str(content.as_str().unwrap()).unwrap();
    assert_eq!(payload["convention"], "camel_case");
    let tags = payload["tags"].as_array().unwrap();
    assert_eq!(tags, &vec![Value::from("create"), Value::from("user")]);
    assert_eq!(responses[0]["result"]["isError"], false);
}

#[test]
fn tools_call_search_returns_hits_for_fixture() {
    let root = make_project("search_hits");
    write_naming_toml(&root);
    write_source(
        &root,
        "src/foo.rs",
        "fn create_user() {}\nfn delete_user() {}\n",
    );

    let req = format!(
        r#"{{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{{"name":"search","arguments":{{"query":"user","path":"{}"}}}}}}"#,
        root.display()
    );
    let responses = run_mcp(&[req.as_str()]);
    let content = &responses[0]["result"]["content"][0]["text"];
    let hits: Value = serde_json::from_str(content.as_str().unwrap()).unwrap();
    let arr = hits.as_array().unwrap();
    assert!(
        arr.len() >= 2,
        "expected at least 2 hits for 'user', got {}",
        arr.len()
    );

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn indexed_project_query_auto_builds_missing_index() {
    let root = make_project("auto_build_index");
    write_naming_toml(&root);
    write_source(
        &root,
        "src/foo.rs",
        "fn create_user() {}\nfn delete_user() {}\n",
    );

    // No .naming/index.json exists yet.
    let req = format!(
        r#"{{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{{"name":"indexed_project_query","arguments":{{"path":"{}"}}}}}}"#,
        root.display()
    );
    let responses = run_mcp(&[req.as_str()]);
    assert_eq!(responses[0]["result"]["isError"], false);
    let content = &responses[0]["result"]["content"][0]["text"];
    let payload: Value = serde_json::from_str(content.as_str().unwrap()).unwrap();
    let families = payload["families"].as_array().unwrap();
    assert!(!families.is_empty(), "expected non-empty families");

    // Index file must now exist on disk.
    assert!(root.join(".naming/index.json").exists());

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn unknown_method_returns_method_not_found() {
    let responses = run_mcp(&[r#"{"jsonrpc":"2.0","id":1,"method":"does/not/exist"}"#]);
    assert_eq!(responses[0]["error"]["code"], -32601);
}

#[test]
fn malformed_json_returns_parse_error() {
    let responses = run_mcp(&["not json"]);
    assert_eq!(responses[0]["error"]["code"], -32700);
}
