//! MCP (Model Context Protocol) server over stdio.
//!
//! Implements JSON-RPC 2.0 framing (one JSON object per `\n`-terminated line)
//! and exposes a small set of tagpath tools so coding agents can call into
//! the library.
//!
//! Implementation is hand-rolled on top of `serde_json` — the MCP wire surface
//! we need (`initialize`, `tools/list`, `tools/call`) is small enough that
//! pulling an external SDK is not worth the dep tree.

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

use crate::{config, index, lint, ontology, parser, query, search};

mod tools;

/// MCP protocol version this server speaks.
pub const PROTOCOL_VERSION: &str = "2024-11-05";

/// JSON-RPC 2.0 error: invalid JSON received.
const ERROR_PARSE: i64 = -32700;
/// JSON-RPC 2.0 error: invalid Request object.
const ERROR_INVALID_REQUEST: i64 = -32600;
/// JSON-RPC 2.0 error: method not found.
const ERROR_METHOD_NOT_FOUND: i64 = -32601;
/// JSON-RPC 2.0 error: invalid params.
const ERROR_INVALID_PARAMS: i64 = -32602;

/// Entry point for `tagpath mcp`. Reads JSON-RPC requests from stdin one
/// line at a time, dispatches them, and writes responses (also one line each)
/// to stdout. Returns when stdin closes.
pub fn run() -> std::io::Result<()> {
    let stdin = std::io::stdin();
    let mut stdin = BufReader::new(stdin.lock());
    let stdout = std::io::stdout();
    let mut stdout = stdout.lock();

    let mut line = String::new();
    loop {
        line.clear();
        let read = stdin.read_line(&mut line)?;
        if read == 0 {
            break;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let response_opt = handle_line(trimmed);
        if let Some(response) = response_opt {
            let mut payload = serde_json::to_string(&response)
                .unwrap_or_else(|_| r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32603,"message":"failed to serialize response"}}"#.to_string());
            payload.push('\n');
            stdout.write_all(payload.as_bytes())?;
            stdout.flush()?;
        }
    }
    Ok(())
}

/// Parse a single JSON-RPC line and produce the response value (or `None`
/// for notifications that require no reply).
fn handle_line(line: &str) -> Option<Value> {
    let parsed: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => {
            return Some(error_response(
                Value::Null,
                ERROR_PARSE,
                format!("parse error: {e}"),
            ));
        }
    };
    let obj = match parsed.as_object() {
        Some(o) => o,
        None => {
            return Some(error_response(
                Value::Null,
                ERROR_INVALID_REQUEST,
                "request must be a JSON object".to_string(),
            ));
        }
    };
    let id = obj.get("id").cloned().unwrap_or(Value::Null);
    let method = match obj.get("method").and_then(Value::as_str) {
        Some(m) => m.to_string(),
        None => {
            return Some(error_response(
                id,
                ERROR_INVALID_REQUEST,
                "missing method".to_string(),
            ));
        }
    };
    // Notifications: no id field present. JSON-RPC says no response.
    let is_notification = !obj.contains_key("id");
    let params = obj.get("params").cloned().unwrap_or(Value::Null);

    let outcome = dispatch(&method, &params);
    if is_notification {
        return None;
    }
    Some(match outcome {
        Ok(result) => success_response(id, result),
        Err((code, message)) => error_response(id, code, message),
    })
}

/// Dispatch a request method to its handler.
fn dispatch(method: &str, params: &Value) -> Result<Value, (i64, String)> {
    match method {
        "initialize" => Ok(json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": { "tools": {} },
            "serverInfo": {
                "name": "tagpath",
                "version": env!("CARGO_PKG_VERSION"),
            },
        })),
        "tools/list" => Ok(json!({ "tools": tools::definitions() })),
        "tools/call" => handle_tools_call(params),
        // Generic ping support is common in MCP harnesses.
        "ping" => Ok(json!({})),
        // Notifications/* are accepted silently (caller distinguishes via id).
        m if m.starts_with("notifications/") => Ok(Value::Null),
        other => Err((ERROR_METHOD_NOT_FOUND, format!("method not found: {other}"))),
    }
}

fn handle_tools_call(params: &Value) -> Result<Value, (i64, String)> {
    let obj = params.as_object().ok_or((
        ERROR_INVALID_PARAMS,
        "tools/call params must be an object".to_string(),
    ))?;
    let name = obj
        .get("name")
        .and_then(Value::as_str)
        .ok_or((ERROR_INVALID_PARAMS, "missing tool name".to_string()))?;
    let args = obj.get("arguments").cloned().unwrap_or(json!({}));
    let result = match name {
        "parse" => tool_parse(&args),
        "normalize_query" => tool_normalize_query(&args),
        "lint" => tool_lint(&args),
        "search" => tool_search(&args),
        "ontology_lookup" => tool_ontology_lookup(&args),
        "indexed_project_query" => tool_indexed_project_query(&args),
        other => {
            return Ok(error_content(format!("unknown tool: {other}")));
        }
    };
    Ok(match result {
        Ok(value) => content_value(value),
        Err(message) => error_content(message),
    })
}

fn success_response(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error_response(id: Value, code: i64, message: String) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message },
    })
}

fn content_value(value: Value) -> Value {
    let text = serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string());
    json!({
        "content": [{ "type": "text", "text": text }],
        "isError": false,
    })
}

fn error_content(message: String) -> Value {
    json!({
        "content": [{ "type": "text", "text": message }],
        "isError": true,
    })
}

// ---------- tool handlers ----------

fn tool_parse(args: &Value) -> Result<Value, String> {
    let name = args
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| "missing argument: name".to_string())?;
    let convention = args.get("convention").and_then(Value::as_str);
    let conv = convention
        .and_then(|c| c.parse::<parser::Convention>().ok())
        .unwrap_or_else(|| parser::detect_convention(name));
    let parsed = parser::parse(name, conv);
    serde_json::to_value(&parsed).map_err(|e| format!("serialize: {e}"))
}

fn tool_normalize_query(args: &Value) -> Result<Value, String> {
    let q = args
        .get("query")
        .and_then(Value::as_str)
        .ok_or_else(|| "missing argument: query".to_string())?;
    let result = query::normalize_query(q);
    serde_json::to_value(&result).map_err(|e| format!("serialize: {e}"))
}

fn tool_lint(args: &Value) -> Result<Value, String> {
    let path_str = args
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| "missing argument: path".to_string())?;
    let path = PathBuf::from(path_str);
    let config_path = lint::find_config(&path)
        .ok_or_else(|| format!("no .naming.toml found from {}", path.display()))?;
    let naming_config = config::resolve(&config_path)?;
    let violations = lint::lint(&path, &naming_config);
    serde_json::to_value(&violations).map_err(|e| format!("serialize: {e}"))
}

fn tool_search(args: &Value) -> Result<Value, String> {
    let q = args
        .get("query")
        .and_then(Value::as_str)
        .ok_or_else(|| "missing argument: query".to_string())?;
    let path_str = args
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| "missing argument: path".to_string())?;
    let path = PathBuf::from(path_str);
    let use_index = args
        .get("use_index")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if use_index {
        let project_root = index::find_project_root(&path)
            .ok_or_else(|| format!("no .naming.toml found from {}", path.display()))?;
        let idx_path = index::index_path(&project_root);
        if !idx_path.exists() {
            eprintln!(
                "notice: auto-building index at {} for search",
                idx_path.display()
            );
            build_index(&project_root)?;
        } else {
            let report = index::check(&project_root)?;
            if !report.fresh {
                eprintln!(
                    "notice: index at {} is stale, rebuilding",
                    idx_path.display()
                );
                build_index(&project_root)?;
            }
        }
        let idx = index::read(&idx_path)?;
        let hits = index::search_index(&idx, q);
        serde_json::to_value(&hits).map_err(|e| format!("serialize: {e}"))
    } else {
        let results = search::search(q, &path);
        serde_json::to_value(&results).map_err(|e| format!("serialize: {e}"))
    }
}

fn tool_ontology_lookup(args: &Value) -> Result<Value, String> {
    let path_str = args
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| "missing argument: path".to_string())?;
    let tag_filter = args.get("tag").and_then(Value::as_str);
    let path = PathBuf::from(path_str);
    let mut report = ontology::load_project(&path)?;
    if let Some(filter) = tag_filter {
        let needle = filter.to_string();
        report.tags.retain(|t| t.tag == needle);
    }
    serde_json::to_value(&report).map_err(|e| format!("serialize: {e}"))
}

fn tool_indexed_project_query(args: &Value) -> Result<Value, String> {
    let path_str = args
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| "missing argument: path".to_string())?;
    let tag = args.get("tag").and_then(Value::as_str);
    let convention = args.get("convention").and_then(Value::as_str);
    let role = args.get("role").and_then(Value::as_str);
    let shape = args.get("shape").and_then(Value::as_str);
    let path = PathBuf::from(path_str);
    let project_root = index::find_project_root(&path)
        .ok_or_else(|| format!("no .naming.toml found from {}", path.display()))?;
    let idx_path = index::index_path(&project_root);
    if !idx_path.exists() {
        eprintln!(
            "notice: auto-building index at {} for indexed_project_query",
            idx_path.display()
        );
        build_index(&project_root)?;
    } else {
        let report = index::check(&project_root)?;
        if !report.fresh {
            eprintln!(
                "notice: index at {} is stale, rebuilding",
                idx_path.display()
            );
            build_index(&project_root)?;
        }
    }
    let idx = index::read(&idx_path)?;
    // Filter families by tag (subset semantics) and members by facet filters.
    let mut families: Vec<index::Family> = idx
        .families
        .iter()
        .filter(|f| match tag {
            Some(t) => f.tags.iter().any(|x| x == t),
            None => true,
        })
        .cloned()
        .collect();
    if convention.is_some() || role.is_some() || shape.is_some() {
        for fam in &mut families {
            fam.members.retain(|m| {
                let conv_ok = match convention {
                    Some(c) => m.convention == c,
                    None => true,
                };
                if !conv_ok {
                    return false;
                }
                // Role/shape live on the parsed identifier — re-parse so we don't
                // need to bake them into the on-disk schema.
                if role.is_some() || shape.is_some() {
                    let conv = m
                        .convention
                        .parse::<parser::Convention>()
                        .unwrap_or_else(|_| parser::detect_convention(&m.name));
                    let parsed = parser::parse(&m.name, conv);
                    if let Some(r) = role
                        && parsed.role.as_deref() != Some(r)
                    {
                        return false;
                    }
                    if let Some(s) = shape
                        && parsed.shape.as_deref() != Some(s)
                    {
                        return false;
                    }
                }
                true
            });
        }
        families.retain(|f| !f.members.is_empty());
    }
    Ok(serde_json::json!({
        "project_root": project_root.display().to_string(),
        "index_path": idx_path.display().to_string(),
        "family_count": families.len(),
        "families": families,
    }))
}

/// Run `index::build` + `index::write` for the given project root.
fn build_index(project_root: &std::path::Path) -> Result<(), String> {
    let opts = index::BuildOptions {
        project_root: project_root.to_path_buf(),
    };
    let idx = index::build(&opts)?;
    let idx_path = index::index_path(project_root);
    index::write(&idx, &idx_path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initialize_returns_protocol_version() {
        let line = r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#;
        let resp = handle_line(line).unwrap();
        assert_eq!(resp["result"]["protocolVersion"], PROTOCOL_VERSION);
        assert_eq!(resp["result"]["serverInfo"]["name"], "tagpath");
    }

    #[test]
    fn tools_list_includes_all_tools() {
        let line = r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#;
        let resp = handle_line(line).unwrap();
        let tools = resp["result"]["tools"].as_array().unwrap();
        let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        for expected in [
            "parse",
            "normalize_query",
            "lint",
            "search",
            "ontology_lookup",
            "indexed_project_query",
        ] {
            assert!(names.contains(&expected), "missing tool {expected}");
        }
    }

    #[test]
    fn parse_tool_returns_camel_case() {
        let line = r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"parse","arguments":{"name":"createUser"}}}"#;
        let resp = handle_line(line).unwrap();
        let content = &resp["result"]["content"][0]["text"];
        let payload: Value = serde_json::from_str(content.as_str().unwrap()).unwrap();
        assert_eq!(payload["convention"], "camel_case");
        let tags = payload["tags"].as_array().unwrap();
        assert_eq!(tags[0], "create");
        assert_eq!(tags[1], "user");
    }

    #[test]
    fn unknown_method_returns_method_not_found() {
        let line = r#"{"jsonrpc":"2.0","id":4,"method":"does/not/exist"}"#;
        let resp = handle_line(line).unwrap();
        assert_eq!(resp["error"]["code"], ERROR_METHOD_NOT_FOUND);
    }

    #[test]
    fn parse_error_on_bad_json() {
        let resp = handle_line("not json").unwrap();
        assert_eq!(resp["error"]["code"], ERROR_PARSE);
    }

    #[test]
    fn notification_returns_none() {
        let line = r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#;
        let resp = handle_line(line);
        assert!(resp.is_none());
    }
}
