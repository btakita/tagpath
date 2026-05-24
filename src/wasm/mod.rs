//! WebAssembly bindings for tagpath.
//!
//! This module is only compiled for `wasm32` targets and exposes a small,
//! filesystem-free subset of the tagpath API. Callers that need to search
//! over source identifiers must extract them on the host (e.g. via ts-morph
//! on the JS side) and pass the row set in to [`search_over_rows`].
#![cfg(target_arch = "wasm32")]

use std::str::FromStr;

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use crate::parser::{self, Convention};
use crate::{alias as alias_mod, prose as prose_mod, query as query_mod};

fn convention_or_detect(name: &str, convention: Option<String>) -> Convention {
    match convention.and_then(|c| Convention::from_str(&c).ok()) {
        Some(c) => c,
        None => parser::detect_convention(name),
    }
}

fn to_js<T: Serialize>(value: &T) -> JsValue {
    serde_wasm_bindgen::to_value(value).unwrap_or(JsValue::NULL)
}

/// Parse a name into a [`parser::ParsedName`] and return it as a JS object.
///
/// If `convention` is omitted, the convention is auto-detected. Unknown
/// convention strings fall back to auto-detection.
#[wasm_bindgen]
pub fn parse(name: &str, convention: Option<String>) -> JsValue {
    let conv = convention_or_detect(name, convention);
    let parsed = parser::parse(name, conv);
    to_js(&parsed)
}

/// Generate cross-convention aliases for a name.
///
/// When `target_convention` is `None`, aliases are generated for every
/// supported convention. Unknown convention strings are treated as `None`.
#[wasm_bindgen]
pub fn alias(name: &str, target_convention: Option<String>) -> JsValue {
    let target = target_convention.and_then(|c| Convention::from_str(&c).ok());
    let result = alias_mod::generate_aliases(name, target);
    to_js(&result)
}

/// Convert a name to a human-readable prose description.
#[wasm_bindgen]
pub fn prose(name: &str) -> String {
    prose_mod::to_prose(name).prose
}

/// Normalize a free-text query into ranked canonical tags.
#[wasm_bindgen]
pub fn normalize_query(query: &str) -> JsValue {
    let normalized = query_mod::normalize_query(query);
    to_js(&normalized)
}

/// Minimal row shape accepted by [`search_over_rows`]. `path` and `line` are
/// optional and are passed through to matching results unchanged.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WasmRow {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
}

/// Match result for a single row, mirroring the input shape and including
/// the parsed tag set so callers can re-rank without re-parsing.
#[derive(Debug, Clone, Serialize)]
pub struct WasmMatch {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    pub convention: parser::Convention,
    pub tags: Vec<String>,
    pub role: Option<String>,
    pub shape: Option<String>,
}

/// Filter a caller-supplied set of rows against a free-text query.
///
/// This is the wasm-friendly substitute for [`crate::search::search`]: the
/// host (JS, ts-morph, the agent harness, ...) extracts identifiers from the
/// filesystem and passes the row set in as JSON. No filesystem access is
/// performed from wasm.
///
/// Rows match when every query tag is present in the parsed identifier's tag
/// set (AND semantics, mirroring the native `search` behavior).
#[wasm_bindgen]
pub fn search_over_rows(query: &str, rows_json: &str) -> JsValue {
    let rows: Vec<WasmRow> = match serde_json::from_str(rows_json) {
        Ok(rows) => rows,
        Err(_) => return JsValue::NULL,
    };
    // Use the prose-aware query normalizer so multi-word inputs like
    // "create user" decompose into ["create", "user"] tags rather than a
    // single token. Single-token identifier queries (e.g. "createUser")
    // also work because the normalizer routes them through the convention
    // parser.
    let normalized = query_mod::normalize_query(query);
    let query_tags: Vec<String> = normalized.tags.into_iter().map(|t| t.tag).collect();
    let matches: Vec<WasmMatch> = rows
        .into_iter()
        .filter_map(|row| {
            let conv = parser::detect_convention(&row.name);
            let parsed = parser::parse(&row.name, conv);
            let all_present = query_tags
                .iter()
                .all(|qt| parsed.tags.iter().any(|it| it == qt));
            if !all_present {
                return None;
            }
            Some(WasmMatch {
                name: row.name,
                path: row.path,
                line: row.line,
                convention: parsed.convention,
                tags: parsed.tags,
                role: parsed.role,
                shape: parsed.shape,
            })
        })
        .collect();
    to_js(&matches)
}
