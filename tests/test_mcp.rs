//! Integration tests for `tagpath mcp` (stdio JSON-RPC server).
//!
//! Each test spawns the compiled `tagpath` binary with the `mcp` subcommand,
//! writes one or more request lines to stdin, closes stdin, waits for the
//! child to exit, then parses every line of stdout as JSON-RPC responses.

#![cfg(feature = "mcp")]

use std::io::Write;
#[cfg(feature = "project-session")]
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::Value;
#[cfg(feature = "project-session")]
use tagpath::index::{self, BuildOptions};

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

#[cfg(feature = "project-session")]
fn run_mcp_with_mid_request_edit(first: &str, edit: impl FnOnce(), second: &str) -> Vec<Value> {
    let bin = env!("CARGO_BIN_EXE_tagpath");
    let mut child = Command::new(bin)
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn tagpath mcp");

    let mut stdin = child.stdin.take().expect("stdin");
    let stdout = child.stdout.take().expect("stdout");
    let mut stderr = child.stderr.take().expect("stderr");
    let mut reader = BufReader::new(stdout);

    stdin.write_all(first.as_bytes()).unwrap();
    stdin.write_all(b"\n").unwrap();
    stdin.flush().unwrap();

    let mut first_line = String::new();
    reader.read_line(&mut first_line).expect("first response");
    assert!(
        !first_line.trim().is_empty(),
        "expected response before edit"
    );

    edit();

    stdin.write_all(second.as_bytes()).unwrap();
    stdin.write_all(b"\n").unwrap();
    stdin.flush().unwrap();
    drop(stdin);

    let mut remaining_stdout = String::new();
    reader
        .read_to_string(&mut remaining_stdout)
        .expect("remaining stdout");

    let status = child.wait().expect("wait");
    let mut stderr_text = String::new();
    stderr.read_to_string(&mut stderr_text).unwrap();
    assert!(
        status.success(),
        "tagpath mcp exited non-zero: {status:?}\nstderr: {stderr_text}"
    );

    let mut lines = vec![first_line];
    lines.extend(
        remaining_stdout
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| {
                let mut owned = line.to_string();
                owned.push('\n');
                owned
            }),
    );
    lines
        .into_iter()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str::<Value>(&line).expect("response is JSON"))
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
fn tools_list_returns_all_nine_tools_with_schemas() {
    let responses = run_mcp(&[r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#]);
    assert_eq!(responses.len(), 1);
    let tools = responses[0]["result"]["tools"]
        .as_array()
        .expect("tools array");
    assert_eq!(tools.len(), 9, "expected 9 tools, got {}", tools.len());
    let expected = [
        "parse",
        "normalize_query",
        "lint",
        "search",
        "ontology_lookup",
        "indexed_project_query",
        "family_by_path",
        "lint_session_doc",
        "index_handle",
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

// ---------- Phase 5: convention helpers (p5mcs) ----------

#[test]
fn family_by_path_returns_matching_family_for_known_source() {
    let root = make_project("family_by_path_known");
    write_naming_toml(&root);
    write_source(
        &root,
        "src/foo.rs",
        "fn create_user() {}\nfn delete_user() {}\n",
    );

    let abs_src = root.canonicalize().unwrap().join("src/foo.rs");
    let req = format!(
        r#"{{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{{"name":"family_by_path","arguments":{{"path":"{}"}}}}}}"#,
        abs_src.display()
    );
    let responses = run_mcp(&[req.as_str()]);
    assert_eq!(
        responses[0]["result"]["isError"], false,
        "unexpected error: {:?}",
        responses[0]["result"]["content"][0]["text"]
    );
    let payload: Value = serde_json::from_str(
        responses[0]["result"]["content"][0]["text"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    let families = payload["families"].as_array().unwrap();
    assert!(
        !families.is_empty(),
        "expected at least one family for src/foo.rs"
    );
    let first = &families[0];
    assert!(
        first["family_handle"]
            .as_str()
            .is_some_and(|s| s.starts_with("fam:"))
    );
    let members = first["members"].as_array().unwrap();
    assert!(!members.is_empty());
    assert!(
        members[0]["member_handle"]
            .as_str()
            .is_some_and(|s| s.starts_with("mem:")),
        "expected mem: handle on member"
    );

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn family_by_path_returns_diagnostic_for_unknown_path() {
    let root = make_project("family_by_path_unknown");
    write_naming_toml(&root);
    write_source(&root, "src/foo.rs", "fn create_user() {}\n");

    // Build index first via indexed_project_query to make sure it exists.
    let prime = format!(
        r#"{{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{{"name":"indexed_project_query","arguments":{{"path":"{}"}}}}}}"#,
        root.display()
    );
    let bogus = root.join("does/not/exist.rs");
    let req = format!(
        r#"{{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{{"name":"family_by_path","arguments":{{"path":"{}","project_root":"{}"}}}}}}"#,
        bogus.display(),
        root.display()
    );
    let responses = run_mcp(&[prime.as_str(), req.as_str()]);
    // The second response is the family_by_path call.
    let r = &responses[1];
    assert_eq!(
        r["result"]["isError"], false,
        "should not be error envelope"
    );
    let payload: Value =
        serde_json::from_str(r["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(payload["diagnostic"], "path_not_in_index");
    assert!(payload["families"].as_array().unwrap().is_empty());

    let _ = std::fs::remove_dir_all(&root);
}

#[cfg(feature = "project-session")]
#[test]
fn family_by_path_project_session_runtime_refreshes_after_edit() {
    let root = make_project("family_by_path_project_session_refresh");
    write_naming_toml(&root);
    write_source(&root, "src/foo.rs", "fn create_user() {}\n");

    let abs_src = root.canonicalize().unwrap().join("src/foo.rs");
    let first = format!(
        r#"{{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{{"name":"family_by_path","arguments":{{"path":"{}","runtime":"project_session"}}}}}}"#,
        abs_src.display()
    );
    let second = format!(
        r#"{{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{{"name":"family_by_path","arguments":{{"path":"{}","runtime":"project_session"}}}}}}"#,
        abs_src.display()
    );

    let responses = run_mcp_with_mid_request_edit(
        &first,
        || {
            write_source(
                &root,
                "src/foo.rs",
                "fn create_user() {}\nfn delete_user() {}\n",
            )
        },
        &second,
    );
    assert_eq!(responses.len(), 2);
    let first_payload: Value = serde_json::from_str(
        responses[0]["result"]["content"][0]["text"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    let second_payload: Value = serde_json::from_str(
        responses[1]["result"]["content"][0]["text"]
            .as_str()
            .unwrap(),
    )
    .unwrap();

    assert_eq!(first_payload["runtime"], "project_session");
    assert_eq!(second_payload["runtime"], "project_session");
    let first_text = first_payload.to_string();
    let second_text = second_payload.to_string();
    assert!(first_text.contains("create_user"));
    assert!(
        !first_text.contains("delete_user"),
        "delete_user should not be visible before the edit: {first_text}"
    );
    assert!(
        second_text.contains("delete_user"),
        "project-session refresh should see same-process edits: {second_text}"
    );
    assert!(
        second_text.contains("mem:"),
        "project-session path should preserve member handles: {second_text}"
    );

    let _ = std::fs::remove_dir_all(&root);
}

#[cfg(feature = "project-session")]
#[test]
fn indexed_project_query_project_session_reports_sidecar_state() {
    let root = make_project("indexed_project_query_project_session_sidecar");
    write_naming_toml(&root);
    write_source(&root, "src/foo.rs", "fn create_user() {}\n");
    let idx = index::build(&BuildOptions {
        project_root: root.clone(),
    })
    .expect("build index");
    index::write(&idx, &index::index_path(&root)).expect("write index");

    let req = format!(
        r#"{{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{{"name":"indexed_project_query","arguments":{{"path":"{}","tag":"user","runtime":"project_session"}}}}}}"#,
        root.display()
    );
    let responses = run_mcp(&[req.as_str()]);
    assert_eq!(responses[0]["result"]["isError"], false);
    let payload: Value = serde_json::from_str(
        responses[0]["result"]["content"][0]["text"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(payload["runtime"], "project_session");
    assert_eq!(payload["sidecar"]["exists"], true);
    assert!(payload["sidecar"]["len"].as_u64().unwrap_or(0) > 0);
    assert!(
        payload["families"].as_array().unwrap()[0]["handle"]
            .as_str()
            .is_some_and(|handle| handle.starts_with("fam:"))
    );

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn lint_session_doc_reports_malformed_findings() {
    let root = make_project("lint_session_malformed");
    let doc_path = root.join("session.md");
    // Missing `=` in agent:done attribute is a classic malformed-attr case.
    std::fs::write(
        &doc_path,
        "# Session\n\n<!-- agent:exchange -->\n<!-- /agent:exchange -->\n\n<!-- agent:done archive PATH -->\n<!-- /agent:done -->\n",
    )
    .unwrap();

    let req = format!(
        r#"{{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{{"name":"lint_session_doc","arguments":{{"path":"{}"}}}}}}"#,
        doc_path.display()
    );
    let responses = run_mcp(&[req.as_str()]);
    assert_eq!(responses[0]["result"]["isError"], false);
    let payload: Value = serde_json::from_str(
        responses[0]["result"]["content"][0]["text"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    let findings = payload["findings"].as_array().unwrap();
    assert!(
        findings
            .iter()
            .any(|f| f["rule"].as_str() == Some("agent-doc/malformed-attr")),
        "expected agent-doc/malformed-attr finding, got {:?}",
        findings
    );
    assert_eq!(payload["exit_code"], 1);

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn lint_session_doc_clean_doc_yields_empty_findings() {
    let root = make_project("lint_session_clean");
    let doc_path = root.join("clean.md");
    std::fs::write(
        &doc_path,
        "# Session\n\n<!-- agent:exchange -->\n<!-- /agent:exchange -->\n",
    )
    .unwrap();
    let req = format!(
        r#"{{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{{"name":"lint_session_doc","arguments":{{"path":"{}"}}}}}}"#,
        doc_path.display()
    );
    let responses = run_mcp(&[req.as_str()]);
    assert_eq!(responses[0]["result"]["isError"], false);
    let payload: Value = serde_json::from_str(
        responses[0]["result"]["content"][0]["text"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert!(payload["findings"].as_array().unwrap().is_empty());
    assert_eq!(payload["exit_code"], 0);

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn index_handle_resolves_known_family_handle() {
    let root = make_project("index_handle_known_fam");
    write_naming_toml(&root);
    write_source(&root, "src/foo.rs", "fn create_user() {}\n");

    // Build the index first.
    let prime = format!(
        r#"{{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{{"name":"indexed_project_query","arguments":{{"path":"{}"}}}}}}"#,
        root.display()
    );
    let prime_resp = run_mcp(&[prime.as_str()]);
    let payload: Value = serde_json::from_str(
        prime_resp[0]["result"]["content"][0]["text"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    let families = payload["families"].as_array().unwrap();
    let fam_handle = families[0]["handle"].as_str().unwrap().to_string();

    let req = format!(
        r#"{{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{{"name":"index_handle","arguments":{{"handle":"{}","project_root":"{}"}}}}}}"#,
        fam_handle,
        root.display()
    );
    let responses = run_mcp(&[req.as_str()]);
    assert_eq!(responses[0]["result"]["isError"], false);
    let out: Value = serde_json::from_str(
        responses[0]["result"]["content"][0]["text"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(out["found"], true);
    assert_eq!(out["kind"], "family");
    assert_eq!(out["family"]["handle"], fam_handle);

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn index_handle_stale_handle_returns_diagnostic() {
    let root = make_project("index_handle_stale");
    write_naming_toml(&root);
    write_source(&root, "src/foo.rs", "fn create_user() {}\n");

    let req = format!(
        r#"{{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{{"name":"index_handle","arguments":{{"handle":"fam:0000000000000000","project_root":"{}"}}}}}}"#,
        root.display()
    );
    let responses = run_mcp(&[req.as_str()]);
    assert_eq!(
        responses[0]["result"]["isError"], false,
        "stale handle should not be MCP error"
    );
    let out: Value = serde_json::from_str(
        responses[0]["result"]["content"][0]["text"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(out["found"], false);
    assert_eq!(out["diagnostic"], "handle_stale");

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn index_handle_invalid_format_returns_error_envelope() {
    let root = make_project("index_handle_invalid");
    write_naming_toml(&root);
    write_source(&root, "src/foo.rs", "fn create_user() {}\n");

    let req = format!(
        r#"{{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{{"name":"index_handle","arguments":{{"handle":"garbage","project_root":"{}"}}}}}}"#,
        root.display()
    );
    let responses = run_mcp(&[req.as_str()]);
    assert_eq!(responses[0]["result"]["isError"], true);
    let txt = responses[0]["result"]["content"][0]["text"]
        .as_str()
        .unwrap();
    assert!(
        txt.contains("invalid handle"),
        "expected invalid-handle message, got {txt}"
    );

    let _ = std::fs::remove_dir_all(&root);
}
