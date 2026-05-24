//! Native test coverage for the logic that backs the wasm-bindgen surface.
//!
//! The actual `#[wasm_bindgen]` wrappers live in `src/wasm/mod.rs` and are only
//! compiled for `wasm32`. These tests exercise the same underlying functions
//! against the same input shapes so we catch signature/shape regressions even
//! when `wasm-pack` is not installed in the build environment.

use serde::Deserialize;
use serde_json::Value;
use std::str::FromStr;
use tagpath::parser::{self, Convention};
use tagpath::{alias, parser as parser_mod, prose, query};

#[test]
fn test_parse_dispatch_roundtrip() {
    let name = "createUserProfile";
    let conv = parser_mod::detect_convention(name);
    assert_eq!(conv, Convention::CamelCase);
    let parsed = parser_mod::parse(name, conv);
    let json = serde_json::to_value(&parsed).expect("ParsedName serializes");
    assert_eq!(json["original"], "createUserProfile");
    assert_eq!(json["convention"], "camel_case");
    let tags: Vec<String> = json["tags"]
        .as_array()
        .expect("tags array")
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert_eq!(tags, vec!["create", "user", "profile"]);
}

#[test]
fn test_parse_with_explicit_convention_string() {
    // Mirrors the wasm `parse(name, Some(conv))` path: parse the string,
    // fall back to detection on error.
    let conv = Convention::from_str("snake_case").expect("known convention");
    let parsed = parser_mod::parse("user_name", conv);
    assert_eq!(parsed.tags, vec!["user", "name"]);

    let unknown = Convention::from_str("not_a_convention");
    assert!(unknown.is_err());
}

#[test]
fn test_alias_dispatch_returns_multiple_conventions() {
    let result = alias::generate_aliases("create_user_profile", None);
    let json = serde_json::to_value(&result).expect("AliasResult serializes");
    let aliases = json["aliases"].as_object().expect("aliases is an object");
    // All six supported conventions should appear when target is None.
    // Keys use Convention::Display form (snake_case, camelCase, PascalCase, ...).
    assert!(aliases.contains_key("snake_case"));
    assert!(aliases.contains_key("camelCase"));
    assert!(aliases.contains_key("PascalCase"));
    assert!(aliases.contains_key("kebab-case"));
    assert!(aliases.contains_key("UPPER_SNAKE_CASE"));
    assert!(aliases.contains_key("Ada_Case"));
    assert_eq!(aliases["camelCase"], "createUserProfile");
    assert_eq!(aliases["PascalCase"], "CreateUserProfile");
    assert_eq!(aliases["kebab-case"], "create-user-profile");
}

#[test]
fn test_alias_target_convention_filters() {
    let result = alias::generate_aliases("create_user", Some(Convention::PascalCase));
    let json = serde_json::to_value(&result).expect("AliasResult serializes");
    let aliases = json["aliases"].as_object().expect("aliases object");
    // Filtered to a single target.
    assert_eq!(aliases.len(), 1);
    assert_eq!(aliases["PascalCase"], "CreateUser");
}

#[test]
fn test_prose_dispatch() {
    let result = prose::to_prose("create_user_profile");
    assert!(!result.prose.is_empty());
    // The prose string should mention the salient nouns.
    let lc = result.prose.to_lowercase();
    assert!(lc.contains("user"));
    assert!(lc.contains("profile"));
}

#[test]
fn test_normalize_query_dispatch() {
    let normalized = query::normalize_query("create user profile");
    let json = serde_json::to_value(&normalized).expect("NormalizedQuery serializes");
    assert_eq!(json["original"], "create user profile");
    let tags = json["tags"].as_array().expect("tags array");
    let tag_strs: Vec<&str> = tags.iter().filter_map(|t| t["tag"].as_str()).collect();
    assert!(tag_strs.contains(&"create"));
    assert!(tag_strs.contains(&"user"));
    assert!(tag_strs.contains(&"profile"));
}

/// Mirror the wasm `search_over_rows` predicate against a fixture row set
/// (without going through `#[wasm_bindgen]`).
#[derive(Debug, Deserialize)]
struct Row {
    name: String,
    #[allow(dead_code)]
    path: Option<String>,
    #[allow(dead_code)]
    line: Option<u32>,
}

#[test]
fn test_search_over_rows_filters_correctly() {
    let rows_json = r#"[
		{ "name": "createUser", "path": "src/a.ts", "line": 10 },
		{ "name": "deleteUser", "path": "src/a.ts", "line": 20 },
		{ "name": "createPost", "path": "src/b.ts", "line": 30 },
		{ "name": "create_user_profile", "path": "src/c.py", "line": 40 }
	]"#;
    let rows: Vec<Row> = serde_json::from_str(rows_json).expect("rows parse");
    assert_eq!(rows.len(), 4);

    // Query tags come from the prose-aware normalizer so multi-word
    // queries decompose correctly (mirrors the wasm wrapper).
    fn query_tags(q: &str) -> Vec<String> {
        query::normalize_query(q)
            .tags
            .into_iter()
            .map(|t| t.tag)
            .collect()
    }

    // Query: "user" should match every row whose tag set contains `user`.
    let qtags = query_tags("user");
    let matches: Vec<&Row> = rows
        .iter()
        .filter(|row| {
            let conv = parser::detect_convention(&row.name);
            let parsed = parser::parse(&row.name, conv);
            qtags.iter().all(|qt| parsed.tags.iter().any(|it| it == qt))
        })
        .collect();
    let names: Vec<&str> = matches.iter().map(|r| r.name.as_str()).collect();
    assert!(names.contains(&"createUser"));
    assert!(names.contains(&"deleteUser"));
    assert!(names.contains(&"create_user_profile"));
    assert!(!names.contains(&"createPost"));

    // Query: "create user" enforces AND across both tags.
    let q2tags = query_tags("create user");
    assert!(q2tags.iter().any(|t| t == "create"));
    assert!(q2tags.iter().any(|t| t == "user"));
    let matches2: Vec<&str> = rows
        .iter()
        .filter(|row| {
            let conv = parser::detect_convention(&row.name);
            let parsed = parser::parse(&row.name, conv);
            q2tags
                .iter()
                .all(|qt| parsed.tags.iter().any(|it| it == qt))
        })
        .map(|r| r.name.as_str())
        .collect();
    assert!(matches2.contains(&"createUser"));
    assert!(matches2.contains(&"create_user_profile"));
    assert!(!matches2.contains(&"deleteUser"));
    assert!(!matches2.contains(&"createPost"));
}

#[test]
fn test_search_over_rows_invalid_json_handled() {
    // Mirrors the wasm fallback path: invalid JSON → empty result (NULL in wasm).
    let bad = "not-valid-json";
    let result: Result<Vec<Value>, _> = serde_json::from_str(bad);
    assert!(result.is_err());
}
