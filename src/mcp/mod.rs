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
use std::path::{Path, PathBuf};

use crate::{config, extract, index, lint, ontology, parser, query, search};

pub mod install;
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

#[derive(Default)]
struct McpRuntimeState {
    #[cfg(all(feature = "project-session", not(target_arch = "wasm32")))]
    project_sessions: std::collections::BTreeMap<PathBuf, crate::project_session::ProjectSession>,
}

#[cfg(all(feature = "project-session", not(target_arch = "wasm32")))]
impl McpRuntimeState {
    fn project_session(&mut self, project_root: &Path) -> &crate::project_session::ProjectSession {
        let key = project_root
            .canonicalize()
            .unwrap_or_else(|_| project_root.to_path_buf());
        self.project_sessions
            .entry(key.clone())
            .or_insert_with(|| crate::project_session::ProjectSession::new(key))
    }
}

/// Entry point for `tagpath mcp`. Reads JSON-RPC requests from stdin one
/// line at a time, dispatches them, and writes responses (also one line each)
/// to stdout. Returns when stdin closes.
pub fn run() -> std::io::Result<()> {
    let stdin = std::io::stdin();
    let mut stdin = BufReader::new(stdin.lock());
    let stdout = std::io::stdout();
    let mut stdout = stdout.lock();
    let mut state = McpRuntimeState::default();

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
        let response_opt = handle_line_with_state(trimmed, &mut state);
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
#[cfg(test)]
fn handle_line(line: &str) -> Option<Value> {
    let mut state = McpRuntimeState::default();
    handle_line_with_state(line, &mut state)
}

fn handle_line_with_state(line: &str, state: &mut McpRuntimeState) -> Option<Value> {
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

    let outcome = dispatch(&method, &params, state);
    if is_notification {
        return None;
    }
    Some(match outcome {
        Ok(result) => success_response(id, result),
        Err((code, message)) => error_response(id, code, message),
    })
}

/// Dispatch a request method to its handler.
fn dispatch(
    method: &str,
    params: &Value,
    state: &mut McpRuntimeState,
) -> Result<Value, (i64, String)> {
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
        "tools/call" => handle_tools_call(params, state),
        // Generic ping support is common in MCP harnesses.
        "ping" => Ok(json!({})),
        // Notifications/* are accepted silently (caller distinguishes via id).
        m if m.starts_with("notifications/") => Ok(Value::Null),
        other => Err((ERROR_METHOD_NOT_FOUND, format!("method not found: {other}"))),
    }
}

fn handle_tools_call(params: &Value, state: &mut McpRuntimeState) -> Result<Value, (i64, String)> {
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
        "indexed_project_query" => tool_indexed_project_query(&args, state),
        "family_by_path" => tool_family_by_path(&args, state),
        "lint_session_doc" => tool_lint_session_doc(&args),
        "index_handle" => tool_index_handle(&args),
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

fn wants_project_session_runtime(args: &Value) -> Result<bool, String> {
    match args.get("runtime").and_then(Value::as_str) {
        None | Some("index") => Ok(false),
        Some("project_session") => wants_project_session_runtime_enabled(),
        Some(other) => Err(format!(
            "unsupported runtime: {other} (expected index or project_session)"
        )),
    }
}

#[cfg(all(feature = "project-session", not(target_arch = "wasm32")))]
fn wants_project_session_runtime_enabled() -> Result<bool, String> {
    Ok(true)
}

#[cfg(not(all(feature = "project-session", not(target_arch = "wasm32"))))]
fn wants_project_session_runtime_enabled() -> Result<bool, String> {
    Err("project_session runtime requires the `project-session` Cargo feature".to_string())
}

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

fn tool_indexed_project_query(args: &Value, state: &mut McpRuntimeState) -> Result<Value, String> {
    #[cfg(not(all(feature = "project-session", not(target_arch = "wasm32"))))]
    let _ = state;

    if wants_project_session_runtime(args)? {
        #[cfg(all(feature = "project-session", not(target_arch = "wasm32")))]
        {
            return tool_indexed_project_query_project_session(args, state);
        }
        #[cfg(not(all(feature = "project-session", not(target_arch = "wasm32"))))]
        {
            unreachable!("wants_project_session_runtime returns an error without the feature");
        }
    }

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

fn tool_family_by_path(args: &Value, state: &mut McpRuntimeState) -> Result<Value, String> {
    #[cfg(not(all(feature = "project-session", not(target_arch = "wasm32"))))]
    let _ = state;

    if wants_project_session_runtime(args)? {
        #[cfg(all(feature = "project-session", not(target_arch = "wasm32")))]
        {
            return tool_family_by_path_project_session(args, state);
        }
        #[cfg(not(all(feature = "project-session", not(target_arch = "wasm32"))))]
        {
            unreachable!("wants_project_session_runtime returns an error without the feature");
        }
    }

    let path_str = args
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| "missing argument: path".to_string())?;
    let input_path = PathBuf::from(path_str);
    let auto_build = args
        .get("auto_build")
        .and_then(Value::as_bool)
        .unwrap_or(true);

    // Resolve project_root: explicit > auto-detect via .naming.toml.
    let project_root = if let Some(pr) = args.get("project_root").and_then(Value::as_str) {
        PathBuf::from(pr)
    } else {
        // Auto-detect anchored on the input path (or its parent if it doesn't exist).
        let anchor = if input_path.exists() {
            input_path.clone()
        } else {
            input_path
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from("."))
        };
        index::find_project_root(&anchor).ok_or_else(|| {
            format!(
                "no .naming.toml found from {} (pass project_root)",
                anchor.display()
            )
        })?
    };

    let idx_path = index::index_path(&project_root);
    if !idx_path.exists() {
        if auto_build {
            eprintln!(
                "notice: auto-building index at {} for family_by_path",
                idx_path.display()
            );
            build_index(&project_root)?;
        } else {
            return Err(format!(
                "index missing at {} (set auto_build: true)",
                idx_path.display()
            ));
        }
    } else if auto_build {
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

    // Canonicalize input path to a project-relative path with forward slashes.
    let abs_input = input_path
        .canonicalize()
        .unwrap_or_else(|_| input_path.clone());
    let canonical_root = project_root
        .canonicalize()
        .unwrap_or_else(|_| project_root.clone());
    let rel_input = abs_input
        .strip_prefix(&canonical_root)
        .unwrap_or(&abs_input)
        .to_string_lossy()
        .replace('\\', "/");

    let mut matched: Vec<Value> = Vec::new();
    for fam in &idx.families {
        let members: Vec<&index::FamilyMember> =
            fam.members.iter().filter(|m| m.path == rel_input).collect();
        if members.is_empty() {
            continue;
        }
        let mut ontology_refs: Vec<&index::OntologyRef> = idx
            .ontology_refs
            .iter()
            .filter(|o| fam.tags.iter().any(|t| t == &o.tag))
            .collect();
        ontology_refs.sort_by(|a, b| a.tag.cmp(&b.tag));
        matched.push(serde_json::json!({
            "family_handle": fam.handle,
            "family_id": fam.family_id,
            "tags": fam.tags,
            "ontology_refs": ontology_refs,
            "members": members
                .into_iter()
                .map(|m| serde_json::json!({
                    "name": m.name,
                    "convention": m.convention,
                    "line": m.line,
                    "member_handle": m.handle,
                }))
                .collect::<Vec<_>>(),
        }));
    }

    if matched.is_empty() {
        let tracked = extract::list_source_files(&project_root)
            .into_iter()
            .any(|p| {
                let abs = p.canonicalize().unwrap_or(p.clone());
                abs == abs_input
            });
        if !tracked {
            return Ok(serde_json::json!({
                "families": [],
                "diagnostic": "path_not_in_index",
            }));
        }
    }

    Ok(serde_json::json!({ "families": matched }))
}

#[cfg(all(feature = "project-session", not(target_arch = "wasm32")))]
fn tool_indexed_project_query_project_session(
    args: &Value,
    state: &mut McpRuntimeState,
) -> Result<Value, String> {
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

    let canonical_root = canonical_project_root_for_handles(&project_root);
    let session = state.project_session(&project_root);
    session.refresh();
    let ontology_refs = project_session_ontology_refs(session, &project_root);
    let sidecar = project_session_sidecar_json(session);
    let family_map = session.family_map();
    let mut families = Vec::new();

    for (family_id, family) in &family_map.families {
        if let Some(tag) = tag
            && !family.tags.iter().any(|candidate| candidate == tag)
        {
            continue;
        }
        let family_handle = index::family_handle(&canonical_root, &family.tags, &ontology_refs);
        let members: Vec<Value> = family
            .members
            .iter()
            .filter(|member| project_session_member_matches(member, convention, role, shape))
            .map(|member| {
                let rel_path = project_relative_path(&member.path, &project_root);
                let member_handle = index::member_handle(&family_handle, &member.name, &rel_path);
                json!({
                    "handle": member_handle,
                    "family_handle": family_handle,
                    "name": member.name,
                    "convention": member.convention,
                    "path": rel_path,
                    "line": member.line,
                })
            })
            .collect();
        if members.is_empty() {
            continue;
        }
        families.push(json!({
            "handle": family_handle,
            "family_id": family_id,
            "tags": family.tags,
            "members": members,
        }));
    }

    Ok(json!({
        "runtime": "project_session",
        "project_root": project_root.display().to_string(),
        "family_count": families.len(),
        "families": families,
        "sidecar": sidecar,
    }))
}

#[cfg(all(feature = "project-session", not(target_arch = "wasm32")))]
fn tool_family_by_path_project_session(
    args: &Value,
    state: &mut McpRuntimeState,
) -> Result<Value, String> {
    let path_str = args
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| "missing argument: path".to_string())?;
    let input_path = PathBuf::from(path_str);
    let project_root = if let Some(pr) = args.get("project_root").and_then(Value::as_str) {
        PathBuf::from(pr)
    } else {
        let anchor = if input_path.exists() {
            input_path.clone()
        } else {
            input_path
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from("."))
        };
        index::find_project_root(&anchor).ok_or_else(|| {
            format!(
                "no .naming.toml found from {} (pass project_root)",
                anchor.display()
            )
        })?
    };

    let canonical_root = canonical_project_root_for_handles(&project_root);
    let rel_input = project_relative_path(&input_path, &project_root);
    let session = state.project_session(&project_root);
    session.refresh();
    let sidecar = project_session_sidecar_json(session);
    let tracked = session
        .source_files()
        .iter()
        .any(|path| project_relative_path(path, &project_root) == rel_input);
    if !tracked {
        return Ok(json!({
            "runtime": "project_session",
            "project_root": project_root.display().to_string(),
            "sidecar": sidecar,
            "families": [],
            "diagnostic": "path_not_in_index",
        }));
    }

    let ontology_refs = project_session_ontology_refs(session, &project_root);
    let family_map = session.family_map();
    let mut matched = Vec::new();
    for (family_id, family) in &family_map.families {
        let members: Vec<Value> = family
            .members
            .iter()
            .filter(|member| project_relative_path(&member.path, &project_root) == rel_input)
            .map(|member| {
                let rel_path = project_relative_path(&member.path, &project_root);
                let family_handle =
                    index::family_handle(&canonical_root, &family.tags, &ontology_refs);
                let member_handle = index::member_handle(&family_handle, &member.name, &rel_path);
                json!({
                    "name": member.name,
                    "convention": member.convention,
                    "line": member.line,
                    "member_handle": member_handle,
                })
            })
            .collect();
        if members.is_empty() {
            continue;
        }
        let family_handle = index::family_handle(&canonical_root, &family.tags, &ontology_refs);
        let mut matching_refs: Vec<&index::OntologyRef> = ontology_refs
            .iter()
            .filter(|ontology_ref| family.tags.iter().any(|tag| tag == &ontology_ref.tag))
            .collect();
        matching_refs.sort_by(|a, b| a.tag.cmp(&b.tag));
        matched.push(json!({
            "family_handle": family_handle,
            "family_id": family_id,
            "tags": family.tags,
            "ontology_refs": matching_refs,
            "members": members,
        }));
    }

    Ok(json!({
        "runtime": "project_session",
        "project_root": project_root.display().to_string(),
        "sidecar": sidecar,
        "families": matched,
    }))
}

#[cfg(all(feature = "project-session", not(target_arch = "wasm32")))]
fn project_session_member_matches(
    member: &crate::project_session::ProjectFamilyMember,
    convention: Option<&str>,
    role: Option<&str>,
    shape: Option<&str>,
) -> bool {
    if let Some(convention) = convention
        && member.convention != convention
    {
        return false;
    }
    if role.is_none() && shape.is_none() {
        return true;
    }
    let conv = member
        .convention
        .parse::<parser::Convention>()
        .unwrap_or_else(|_| parser::detect_convention(&member.name));
    let parsed = parser::parse(&member.name, conv);
    if let Some(role) = role
        && parsed.role.as_deref() != Some(role)
    {
        return false;
    }
    if let Some(shape) = shape
        && parsed.shape.as_deref() != Some(shape)
    {
        return false;
    }
    true
}

#[cfg(all(feature = "project-session", not(target_arch = "wasm32")))]
fn project_session_ontology_refs(
    session: &crate::project_session::ProjectSession,
    project_root: &Path,
) -> Vec<index::OntologyRef> {
    let ontology = session.ontology_state();
    let Some(report) = ontology.report.as_ref() else {
        return Vec::new();
    };
    let mut refs: Vec<index::OntologyRef> = report
        .tags
        .iter()
        .map(|tag| index::OntologyRef {
            tag: tag.tag.clone(),
            path: project_relative_path(&tag.path, project_root),
            hash: sha256_file(&tag.path).unwrap_or_default(),
        })
        .collect();
    refs.sort_by(|a, b| a.tag.cmp(&b.tag));
    refs
}

#[cfg(all(feature = "project-session", not(target_arch = "wasm32")))]
fn project_session_sidecar_json(session: &crate::project_session::ProjectSession) -> Value {
    let sidecar = session.sidecar_state();
    json!({
        "path": sidecar.path.display().to_string(),
        "exists": sidecar.exists,
        "len": sidecar.len,
        "modified_secs": sidecar.modified_secs,
    })
}

#[cfg(all(feature = "project-session", not(target_arch = "wasm32")))]
fn canonical_project_root_for_handles(project_root: &Path) -> String {
    project_root
        .canonicalize()
        .unwrap_or_else(|_| project_root.to_path_buf())
        .to_string_lossy()
        .replace('\\', "/")
}

#[cfg(all(feature = "project-session", not(target_arch = "wasm32")))]
fn project_relative_path(path: &Path, project_root: &Path) -> String {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        project_root.join(path)
    };
    let absolute = absolute.canonicalize().unwrap_or(absolute);
    let root = project_root
        .canonicalize()
        .unwrap_or_else(|_| project_root.to_path_buf());
    absolute
        .strip_prefix(&root)
        .unwrap_or(&absolute)
        .to_string_lossy()
        .replace('\\', "/")
}

#[cfg(all(feature = "project-session", not(target_arch = "wasm32")))]
fn sha256_file(path: &Path) -> Option<String> {
    use sha2::{Digest, Sha256};

    let bytes = std::fs::read(path).ok()?;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        hex.push_str(&format!("{byte:02x}"));
    }
    Some(format!("sha256:{hex}"))
}

fn tool_lint_session_doc(args: &Value) -> Result<Value, String> {
    let path_str = args
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| "missing argument: path".to_string())?;
    let path = PathBuf::from(path_str);
    let fs_checks = args
        .get("fs_checks")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let rules: Vec<String> = args
        .get("rules")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();

    let text =
        std::fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let opts = lint::AgentDocOptions {
        fs_checks,
        rule_filter: rules,
    };
    let findings = lint::lint_agent_doc(&path, &text, &opts);
    let exit_code: u8 = if findings
        .iter()
        .any(|f| matches!(f.severity, lint::LintSeverity::Error))
    {
        1
    } else {
        0
    };
    Ok(serde_json::json!({
        "findings": findings,
        "exit_code": exit_code,
    }))
}

fn tool_index_handle(args: &Value) -> Result<Value, String> {
    let handle = args
        .get("handle")
        .and_then(Value::as_str)
        .ok_or_else(|| "missing argument: handle".to_string())?
        .to_string();
    let auto_build = args
        .get("auto_build")
        .and_then(Value::as_bool)
        .unwrap_or(true);

    let kind = if handle.starts_with("fam:") {
        "fam"
    } else if handle.starts_with("mem:") {
        "mem"
    } else {
        return Err(format!(
            "invalid handle: {handle} (must start with fam: or mem:)"
        ));
    };

    let project_root = if let Some(pr) = args.get("project_root").and_then(Value::as_str) {
        PathBuf::from(pr)
    } else {
        index::find_project_root(Path::new(".")).ok_or_else(|| {
            "no .naming.toml found from current directory (pass project_root)".to_string()
        })?
    };

    let idx_path = index::index_path(&project_root);
    if !idx_path.exists() {
        if auto_build {
            eprintln!(
                "notice: auto-building index at {} for index_handle",
                idx_path.display()
            );
            build_index(&project_root)?;
        } else {
            return Err(format!(
                "index missing at {} (set auto_build: true)",
                idx_path.display()
            ));
        }
    } else if auto_build {
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

    match kind {
        "fam" => {
            if let Some(fam) = idx.families.iter().find(|f| f.handle == handle) {
                Ok(serde_json::json!({
                    "found": true,
                    "kind": "family",
                    "family": fam,
                }))
            } else {
                Ok(serde_json::json!({
                    "found": false,
                    "diagnostic": "handle_stale",
                }))
            }
        }
        "mem" => {
            for fam in &idx.families {
                if let Some(mem) = fam.members.iter().find(|m| m.handle == handle) {
                    return Ok(serde_json::json!({
                        "found": true,
                        "kind": "member",
                        "member": mem,
                        "family": fam,
                    }));
                }
            }
            Ok(serde_json::json!({
                "found": false,
                "diagnostic": "handle_stale",
            }))
        }
        _ => unreachable!(),
    }
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
            "family_by_path",
            "lint_session_doc",
            "index_handle",
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
