use serde::Serialize;
use std::collections::BTreeMap;

use crate::parser;

/// Normalized, ranked tags extracted from a free-text query.
#[derive(Debug, Serialize)]
pub struct NormalizedQuery {
    /// Original user query.
    pub original: String,
    /// Ordered tags, sorted by weight and then first appearance.
    pub tags: Vec<QueryTag>,
}

/// One weighted tag extracted from a free-text query.
#[derive(Debug, Clone, Serialize)]
pub struct QueryTag {
    /// Lowercase canonical tag.
    pub tag: String,
    /// Aggregate relevance weight. Identifier-shaped tokens contribute more
    /// than ordinary prose terms, and repeated mentions increase the weight.
    pub weight: f32,
    /// Number of token occurrences that contributed this tag.
    pub occurrences: usize,
    /// Zero-based token position of the first contributing source token.
    pub first_position: usize,
    /// Distinct source tokens that contributed this tag, in first-seen order.
    pub sources: Vec<String>,
}

#[derive(Debug)]
struct QueryToken {
    text: String,
    position: usize,
    identifier_like: bool,
}

#[derive(Debug)]
struct QueryTagBuilder {
    tag: String,
    weight: f32,
    occurrences: usize,
    first_position: usize,
    first_order: usize,
    sources: Vec<String>,
}

/// Normalize a free-text query into ordered, weighted canonical tags.
///
/// This is intended for agent-facing callers such as tsift that receive prose
/// prompts first and want a semantic tag signal before falling back to lexical
/// or hybrid search. Identifier-shaped tokens are parsed through the normal
/// tagpath convention logic, while ordinary prose terms are lowercased and
/// filtered through a small stop-word list.
pub fn normalize_query(query: &str) -> NormalizedQuery {
    NormalizedQuery {
        original: query.to_string(),
        tags: normalize_query_tags(query),
    }
}

/// Normalize a free-text query and return only the ordered weighted tags.
pub fn normalize_query_tags(query: &str) -> Vec<QueryTag> {
    let mut tags: BTreeMap<String, QueryTagBuilder> = BTreeMap::new();
    for token in tokenize_query(query) {
        let token_tags = tags_for_token(&token);
        if token_tags.is_empty() {
            continue;
        }
        let weight = if token.identifier_like { 2.0 } else { 1.0 };
        for (tag_index, tag) in token_tags.into_iter().enumerate() {
            let order = token.position * 1024 + tag_index;
            let entry = tags.entry(tag.clone()).or_insert_with(|| QueryTagBuilder {
                tag,
                weight: 0.0,
                occurrences: 0,
                first_position: token.position,
                first_order: order,
                sources: Vec::new(),
            });
            entry.weight += weight;
            entry.occurrences += 1;
            entry.first_position = entry.first_position.min(token.position);
            entry.first_order = entry.first_order.min(order);
            if !entry.sources.iter().any(|source| source == &token.text) {
                entry.sources.push(token.text.clone());
            }
        }
    }
    let mut builders: Vec<QueryTagBuilder> = tags.into_values().collect();
    builders.sort_by(|a, b| {
        b.weight
            .total_cmp(&a.weight)
            .then(a.first_order.cmp(&b.first_order))
            .then(a.tag.cmp(&b.tag))
    });
    builders
        .into_iter()
        .map(|builder| QueryTag {
            tag: builder.tag,
            weight: builder.weight,
            occurrences: builder.occurrences,
            first_position: builder.first_position,
            sources: builder.sources,
        })
        .collect()
}

fn tags_for_token(token: &QueryToken) -> Vec<String> {
    if token.identifier_like {
        let convention = parser::detect_convention(&token.text);
        return parser::parse(&token.text, convention).tags;
    }
    let tag = token.text.to_lowercase();
    if is_stop_word(&tag) || tag.chars().all(|ch| ch.is_ascii_digit()) {
        Vec::new()
    } else {
        vec![tag]
    }
}

fn tokenize_query(query: &str) -> Vec<QueryToken> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    for ch in query.chars() {
        if is_query_token_char(ch) {
            current.push(ch);
        } else {
            push_token(&mut tokens, &mut current);
        }
    }
    push_token(&mut tokens, &mut current);
    tokens
}

fn push_token(tokens: &mut Vec<QueryToken>, current: &mut String) {
    let text = current
        .trim_matches(|ch: char| ch == '-' || ch == '_' || ch == '$')
        .to_string();
    current.clear();
    if text.is_empty() {
        return;
    }
    let identifier_like = is_identifier_like(&text);
    tokens.push(QueryToken {
        text,
        position: tokens.len(),
        identifier_like,
    });
}

fn is_query_token_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '$')
}

fn is_identifier_like(token: &str) -> bool {
    token.contains('_')
        || token.contains('-')
        || token.contains('$')
        || has_case_boundary(token)
        || token.chars().any(|ch| ch.is_ascii_digit())
}

fn has_case_boundary(token: &str) -> bool {
    let convention = parser::detect_convention(token);
    parser::parse(token, convention).tags.len() > 1
}

fn is_stop_word(tag: &str) -> bool {
    matches!(
        tag,
        "a" | "an"
            | "and"
            | "are"
            | "as"
            | "at"
            | "be"
            | "before"
            | "by"
            | "can"
            | "do"
            | "find"
            | "for"
            | "from"
            | "in"
            | "into"
            | "is"
            | "it"
            | "of"
            | "on"
            | "or"
            | "please"
            | "that"
            | "the"
            | "these"
            | "this"
            | "those"
            | "to"
            | "with"
            | "without"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_text_query_promotes_identifier_shaped_tokens() {
        let tags = normalize_query_tags(
            "Find session-review previews for raw_symbol output before hybrid fallback",
        );
        let names: Vec<&str> = tags.iter().map(|tag| tag.tag.as_str()).collect();
        assert_eq!(
            &names[..4],
            &["session", "review", "raw", "symbol"],
            "identifier-shaped terms should outrank ordinary prose: {tags:?}"
        );
        assert!(names.contains(&"hybrid"));
        assert!(names.contains(&"fallback"));
        assert!(!names.contains(&"for"));
        assert!(!names.contains(&"before"));
    }

    #[test]
    fn duplicate_mentions_increase_weight_and_keep_first_position() {
        let tags = normalize_query_tags("validate user validateUser");
        let validate = tags.iter().find(|tag| tag.tag == "validate").unwrap();
        let user = tags.iter().find(|tag| tag.tag == "user").unwrap();
        assert_eq!(validate.weight, 3.0);
        assert_eq!(validate.occurrences, 2);
        assert_eq!(validate.first_position, 0);
        assert_eq!(user.weight, 3.0);
        assert_eq!(user.occurrences, 2);
        assert_eq!(user.first_position, 1);
        assert_eq!(tags[0].tag, "validate");
    }

    #[test]
    fn camel_case_query_tokens_keep_role_tags() {
        let tags = normalize_query_tags("useQuery failed");
        let names: Vec<&str> = tags.iter().map(|tag| tag.tag.as_str()).collect();
        assert!(names.contains(&"use"));
        assert!(names.contains(&"query"));
        assert!(names.contains(&"failed"));
    }
}
