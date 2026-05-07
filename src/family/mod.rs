use serde::Serialize;
use std::collections::BTreeMap;

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
}
