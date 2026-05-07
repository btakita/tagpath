use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use crate::parser::{self, ALL_CONVENTIONS, Convention, ParsedName};

/// Stable semantic family for one identifier.
///
/// This is the compact schema intended for callers that need to collapse
/// convention-specific spellings into one reusable semantic handle.
#[derive(Debug, Serialize)]
pub struct TagFamily {
    /// Original input identifier.
    pub original: String,
    /// Stable canonical handle for the full tag sequence.
    pub canonical: String,
    /// Lowercase canonical tags.
    pub tags: Vec<String>,
    /// Namespace dimensions split from `__`, when present.
    pub dimensions: Vec<TagDimension>,
    /// Detected role, such as factory, hook, setter, or predicate.
    pub role: Option<String>,
    /// Detected data shape, such as array, record, map, set, or signal.
    pub shape: Option<String>,
    /// Convention name to generated surface spelling.
    pub aliases: BTreeMap<String, String>,
    /// Ordered surface spelling examples for display-oriented clients.
    pub examples: Vec<SurfaceExample>,
}

/// One namespace dimension inside a tag family.
#[derive(Debug, Serialize)]
pub struct TagDimension {
    /// Zero-based namespace dimension index.
    pub index: usize,
    /// Tags in this dimension.
    pub tags: Vec<String>,
    /// Stable canonical handle for this dimension.
    pub canonical: String,
}

/// One generated surface spelling for a tag family.
#[derive(Debug, Serialize)]
pub struct SurfaceExample {
    /// Naming convention used to generate the spelling.
    pub convention: String,
    /// Generated identifier spelling.
    pub spelling: String,
}

/// A concrete identifier occurrence to include in a grouped family summary.
#[derive(Debug)]
pub struct FamilyOccurrence {
    pub identifier: String,
    pub file: PathBuf,
    pub line: usize,
    pub column: usize,
    pub convention: String,
    pub tags: Vec<String>,
    pub role: Option<String>,
    pub shape: Option<String>,
    pub context: Option<String>,
}

/// Compact grouped summary for all extracted identifiers with the same tags.
#[derive(Debug, Serialize)]
pub struct TagFamilySummary {
    /// Stable canonical handle for the grouped tag sequence.
    pub canonical: String,
    /// Lowercase canonical tags.
    pub tags: Vec<String>,
    /// Total number of occurrences in this family.
    pub count: usize,
    /// Detected roles present in the family.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub roles: Vec<String>,
    /// Detected shapes present in the family.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub shapes: Vec<String>,
    /// Representative examples, capped by the caller.
    pub examples: Vec<FamilySummaryExample>,
}

/// One representative source occurrence for a compact family summary.
#[derive(Debug, Serialize)]
pub struct FamilySummaryExample {
    pub identifier: String,
    pub file: PathBuf,
    pub line: usize,
    pub column: usize,
    pub convention: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
}

struct SummaryBuilder {
    tags: Vec<String>,
    count: usize,
    roles: BTreeSet<String>,
    shapes: BTreeSet<String>,
    examples: Vec<FamilySummaryExample>,
    example_keys: BTreeSet<String>,
}

/// Build a stable tag family from an identifier, auto-detecting convention.
pub fn generate_family(name: &str) -> TagFamily {
    let convention = parser::detect_convention(name);
    generate_family_with_convention(name, convention)
}

/// Build a stable tag family from an identifier using a known convention.
pub fn generate_family_with_convention(name: &str, convention: Convention) -> TagFamily {
    let parsed = parser::parse(name, convention);
    family_from_parsed(parsed)
}

fn family_from_parsed(parsed: ParsedName) -> TagFamily {
    let canonical = parsed.tags.join("_");
    let dimensions = parsed
        .namespaces
        .iter()
        .enumerate()
        .map(|(index, tags)| TagDimension {
            index,
            tags: tags.clone(),
            canonical: tags.join("_"),
        })
        .collect();
    let mut aliases = BTreeMap::new();
    let mut examples = Vec::new();
    for convention in ALL_CONVENTIONS {
        let spelling = parser::join_tags(&parsed.tags, convention);
        aliases.insert(convention.to_string(), spelling.clone());
        examples.push(SurfaceExample {
            convention: convention.to_string(),
            spelling,
        });
    }
    TagFamily {
        original: parsed.original,
        canonical,
        tags: parsed.tags,
        dimensions,
        role: parsed.role,
        shape: parsed.shape,
        aliases,
        examples,
    }
}

/// Collapse identifier occurrences into canonical tag families.
///
/// `example_limit` caps the representative examples per family while `count`
/// still records every matching occurrence.
pub fn summarize_occurrences<I>(occurrences: I, example_limit: usize) -> Vec<TagFamilySummary>
where
    I: IntoIterator<Item = FamilyOccurrence>,
{
    let mut summaries: BTreeMap<String, SummaryBuilder> = BTreeMap::new();
    for occurrence in occurrences {
        let canonical = occurrence.tags.join("_");
        let entry = summaries
            .entry(canonical)
            .or_insert_with(|| SummaryBuilder {
                tags: occurrence.tags.clone(),
                count: 0,
                roles: BTreeSet::new(),
                shapes: BTreeSet::new(),
                examples: Vec::new(),
                example_keys: BTreeSet::new(),
            });
        entry.count += 1;
        if let Some(role) = occurrence.role {
            entry.roles.insert(role);
        }
        if let Some(shape) = occurrence.shape {
            entry.shapes.insert(shape);
        }
        let example_key = occurrence.identifier.clone();
        if entry.examples.len() < example_limit && entry.example_keys.insert(example_key) {
            entry.examples.push(FamilySummaryExample {
                identifier: occurrence.identifier,
                file: occurrence.file,
                line: occurrence.line,
                column: occurrence.column,
                convention: occurrence.convention,
                context: occurrence.context,
            });
        }
    }
    summaries
        .into_iter()
        .map(|(canonical, summary)| TagFamilySummary {
            canonical,
            tags: summary.tags,
            count: summary.count,
            roles: summary.roles.into_iter().collect(),
            shapes: summary.shapes.into_iter().collect(),
            examples: summary.examples,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn family_collapses_cross_convention_spellings() {
        let snake = generate_family("create_user_profile");
        let camel = generate_family("createUserProfile");
        assert_eq!(snake.canonical, "create_user_profile");
        assert_eq!(camel.canonical, "create_user_profile");
        assert_eq!(snake.tags, camel.tags);
        assert_eq!(snake.role, Some("factory".to_string()));
        assert_eq!(snake.aliases["camelCase"], "createUserProfile");
        assert_eq!(snake.aliases["PascalCase"], "CreateUserProfile");
    }

    #[test]
    fn family_preserves_namespace_dimensions() {
        let family = generate_family("auth0__user__validate");
        assert_eq!(family.canonical, "auth0_user_validate");
        assert_eq!(family.role, Some("validator".to_string()));
        assert_eq!(family.dimensions.len(), 3);
        assert_eq!(family.dimensions[0].canonical, "auth0");
        assert_eq!(family.dimensions[1].tags, vec!["user"]);
        assert_eq!(family.dimensions[2].canonical, "validate");
    }

    #[test]
    fn family_reports_shape_and_ordered_examples() {
        let family = generate_family("post_a");
        assert_eq!(family.shape, Some("array".to_string()));
        assert_eq!(family.examples.len(), ALL_CONVENTIONS.len());
        assert_eq!(family.examples[0].convention, "snake_case");
        assert_eq!(family.examples[0].spelling, "post_a");
    }

    #[test]
    fn summaries_group_by_canonical_tags_and_cap_examples() {
        let summaries = summarize_occurrences(
            [
                FamilyOccurrence {
                    identifier: "create_user".to_string(),
                    file: "a.rs".into(),
                    line: 1,
                    column: 4,
                    convention: "snake_case".to_string(),
                    tags: vec!["create".to_string(), "user".to_string()],
                    role: Some("factory".to_string()),
                    shape: None,
                    context: None,
                },
                FamilyOccurrence {
                    identifier: "createUser".to_string(),
                    file: "b.ts".into(),
                    line: 2,
                    column: 10,
                    convention: "camelCase".to_string(),
                    tags: vec!["create".to_string(), "user".to_string()],
                    role: Some("factory".to_string()),
                    shape: None,
                    context: Some("function".to_string()),
                },
                FamilyOccurrence {
                    identifier: "create_user".to_string(),
                    file: "c.py".into(),
                    line: 3,
                    column: 1,
                    convention: "snake_case".to_string(),
                    tags: vec!["create".to_string(), "user".to_string()],
                    role: Some("factory".to_string()),
                    shape: None,
                    context: None,
                },
            ],
            2,
        );
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].canonical, "create_user");
        assert_eq!(summaries[0].count, 3);
        assert_eq!(summaries[0].roles, vec!["factory"]);
        assert_eq!(summaries[0].examples.len(), 2);
        assert_eq!(summaries[0].examples[0].identifier, "create_user");
        assert_eq!(summaries[0].examples[1].identifier, "createUser");
    }
}
