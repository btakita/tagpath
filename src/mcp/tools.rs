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
