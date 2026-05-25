//! Tool descriptors advertised by the MCP server.
//!
//! Each descriptor contains a `name`, human-readable `description`, and a
//! minimal JSON Schema for the input shape. Output shapes are documented in
//! the description body; agents discover the wire format from the tool reply.

use serde_json::{Value, json};

/// Build the `tools/list` payload.
pub fn definitions() -> Vec<Value> {
    vec![
        parse_tool(),
        normalize_query_tool(),
        lint_tool(),
        search_tool(),
        ontology_lookup_tool(),
        indexed_project_query_tool(),
        family_by_path_tool(),
        lint_session_doc_tool(),
        index_handle_tool(),
    ]
}

fn parse_tool() -> Value {
    json!({
        "name": "parse",
        "description": "Parse an identifier into its canonical tag form. Returns the convention, tags, namespaces, role, and shape. Mirrors `tagpath parse --format json`.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "name": { "type": "string", "description": "Identifier to parse, e.g. createUser" },
                "convention": { "type": "string", "description": "Optional override; one of snake_case, camelCase, PascalCase, kebab-case, UPPER_SNAKE_CASE, Ada_Case" },
            },
            "required": ["name"],
        },
    })
}

fn normalize_query_tool() -> Value {
    json!({
        "name": "normalize_query",
        "description": "Normalize a free-text query into weighted, ranked tags. Mirrors `tagpath normalize-query --format json`.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Free-text query or agent prompt" },
            },
            "required": ["query"],
        },
    })
}

fn lint_tool() -> Value {
    json!({
        "name": "lint",
        "description": "Lint identifiers in a path against the nearest .naming.toml. Returns the same JSON shape as `tagpath lint --format json`.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "File or directory path" },
            },
            "required": ["path"],
        },
    })
}

fn search_tool() -> Value {
    json!({
        "name": "search",
        "description": "Search for identifiers whose tags contain all of the query's tags. Returns the same JSON shape as `tagpath search --format json`. Set `use_index: true` to read from .naming/index.json (auto-rebuilds when stale).",
        "inputSchema": {
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Tag query, e.g. \"user\" or \"create_user\"" },
                "path": { "type": "string", "description": "File or directory to scan" },
                "use_index": { "type": "boolean", "description": "Read from .naming/index.json instead of rescanning", "default": false },
            },
            "required": ["query", "path"],
        },
    })
}

fn ontology_lookup_tool() -> Value {
    json!({
        "name": "ontology_lookup",
        "description": "Load .naming/tags ontology and return tag definitions. Mirrors `tagpath ontology --format json`. When `tag` is provided, filters to that single tag.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Project path (contains .naming.toml)" },
                "tag": { "type": "string", "description": "Optional single-tag filter" },
            },
            "required": ["path"],
        },
    })
}

fn family_by_path_tool() -> Value {
    json!({
        "name": "family_by_path",
        "description": "Return every indexed family whose members include a record for the given source file. Opens .naming/index.json (auto-builds when missing/stale). Members are filtered to only those matching the input path. Returns `{ families: [...], diagnostic? }`; `diagnostic: \"path_not_in_index\"` when the file is unknown to the index and is not a tracked source.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "absolute or project-relative source file path" },
                "project_root": { "type": "string", "description": "optional project root; auto-detected via .naming.toml if omitted" },
                "auto_build": { "type": "boolean", "default": true, "description": "rebuild index if missing or stale" },
            },
            "required": ["path"],
        },
    })
}

fn lint_session_doc_tool() -> Value {
    json!({
        "name": "lint_session_doc",
        "description": "Lint an agent-doc session-document markdown file. Wraps `lint::agent_doc::lint_agent_doc`. Returns `{ findings: [...], exit_code: 0|1 }`; exit_code mirrors the CLI (0 clean, 1 findings present).",
        "inputSchema": {
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "session-document markdown path" },
                "fs_checks": { "type": "boolean", "default": false, "description": "enable filesystem-dependent rules (#9, #12)" },
                "rules": { "type": "array", "items": { "type": "string" }, "description": "restrict to specific rule IDs" },
            },
            "required": ["path"],
        },
    })
}

fn index_handle_tool() -> Value {
    json!({
        "name": "index_handle",
        "description": "Resolve a `fam:` or `mem:` handle against the current project index. For `fam:` returns the full family record; for `mem:` returns `{ member, family }`. When the handle no longer matches the live index (rename, retag, deletion), returns `{ found: false, diagnostic: \"handle_stale\" }` so consumers can react. Invalid handle formats (no `fam:`/`mem:` prefix) yield an `isError: true` envelope.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "handle": { "type": "string", "description": "fam:... or mem:... handle from a prior index" },
                "project_root": { "type": "string", "description": "optional project root; auto-detected via .naming.toml if omitted" },
                "auto_build": { "type": "boolean", "default": true, "description": "rebuild index if missing or stale" },
            },
            "required": ["handle"],
        },
    })
}

fn indexed_project_query_tool() -> Value {
    json!({
        "name": "indexed_project_query",
        "description": "Query the persistent project index (.naming/index.json), filtering families by tag and members by convention/role/shape. Auto-builds the index when missing and auto-rebuilds when stale (notice logged to stderr).",
        "inputSchema": {
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Project path (contains .naming.toml)" },
                "tag": { "type": "string", "description": "Optional tag to filter families on" },
                "convention": { "type": "string", "description": "Optional member convention filter (snake_case, camel_case, pascal_case, kebab_case, upper_snake_case, ada_case)" },
                "role": { "type": "string", "description": "Optional role filter (e.g. create, update)" },
                "shape": { "type": "string", "description": "Optional shape filter" },
            },
            "required": ["path"],
        },
    })
}
