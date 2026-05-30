use tagpath::{
    alias, compression, family,
    parser::{self, Convention, ALL_CONVENTIONS},
    prose, query,
};

#[test]
fn parse_via_lib() {
    let parsed = parser::parse("create_user_profile", Convention::SnakeCase);
    assert_eq!(parsed.tags, vec!["create", "user", "profile"]);
    assert_eq!(parsed.convention, Convention::SnakeCase);
    assert_eq!(parsed.role, Some("factory".to_string()));
}

#[test]
fn detect_convention_via_lib() {
    assert_eq!(
        parser::detect_convention("personName"),
        Convention::CamelCase
    );
    assert_eq!(
        parser::detect_convention("PersonName"),
        Convention::PascalCase
    );
    assert_eq!(
        parser::detect_convention("person_name"),
        Convention::SnakeCase
    );
    assert_eq!(
        parser::detect_convention("person-name"),
        Convention::KebabCase
    );
    assert_eq!(
        parser::detect_convention("PERSON_NAME"),
        Convention::UpperSnakeCase
    );
    assert_eq!(
        parser::detect_convention("Person_Name"),
        Convention::AdaCase
    );
}

#[test]
fn join_tags_via_lib() {
    let tags = vec![
        "create".to_string(),
        "user".to_string(),
        "profile".to_string(),
    ];
    assert_eq!(
        parser::join_tags(&tags, Convention::CamelCase),
        "createUserProfile"
    );
    assert_eq!(
        parser::join_tags(&tags, Convention::PascalCase),
        "CreateUserProfile"
    );
    assert_eq!(
        parser::join_tags(&tags, Convention::KebabCase),
        "create-user-profile"
    );
}

#[test]
fn all_conventions_accessible() {
    assert_eq!(ALL_CONVENTIONS.len(), 6);
}

#[test]
fn cross_convention_equivalence_via_lib() {
    let snake = parser::parse("user_name", Convention::SnakeCase);
    let camel = parser::parse("userName", Convention::CamelCase);
    let pascal = parser::parse("UserName", Convention::PascalCase);
    let kebab = parser::parse("user-name", Convention::KebabCase);
    let upper = parser::parse("USER_NAME", Convention::UpperSnakeCase);
    let ada = parser::parse("User_Name", Convention::AdaCase);
    let canonical = vec!["user", "name"];
    assert_eq!(snake.tags, canonical);
    assert_eq!(camel.tags, canonical);
    assert_eq!(pascal.tags, canonical);
    assert_eq!(kebab.tags, canonical);
    assert_eq!(upper.tags, canonical);
    assert_eq!(ada.tags, canonical);
}

#[test]
fn alias_via_lib() {
    let result = tagpath::alias::generate_aliases("user_name", None);
    assert_eq!(result.aliases.len(), 6);
}

#[test]
fn family_via_lib() {
    let result = tagpath::family::generate_family("auth0__user__validate");
    assert_eq!(result.canonical, "auth0_user_validate");
    assert_eq!(result.tags, vec!["auth0", "user", "validate"]);
    assert_eq!(result.dimensions.len(), 3);
    assert_eq!(result.role, Some("validator".to_string()));
    assert_eq!(result.aliases["camelCase"], "auth0UserValidate");
}

#[test]
fn prose_via_lib() {
    let result = tagpath::prose::to_prose("create_user_profile");
    assert!(!result.prose.is_empty());
}

#[test]
fn normalize_query_via_lib() {
    let result = tagpath::query::normalize_query("Find raw_symbol output with validateUser");
    let tags: Vec<&str> = result.tags.iter().map(|tag| tag.tag.as_str()).collect();
    assert_eq!(&tags[..4], &["raw", "symbol", "validate", "user"]);
}

#[test]
fn compression_report_via_lib() {
    let rows = vec![
        tagpath::compression::RawSymbolRow {
            identifier: "raw_symbol".to_string(),
            file: "src/search.rs".into(),
            line: 42,
            column: 0,
            context: Some("field".to_string()),
        },
        tagpath::compression::RawSymbolRow {
            identifier: "rawSymbol".to_string(),
            file: "src/search.ts".into(),
            line: 7,
            column: 0,
            context: Some("field".to_string()),
        },
    ];
    let report = tagpath::compression::build_report(&rows);
    assert_eq!(report.raw_symbol_count, 2);
    assert_eq!(report.family_count, 1);
    assert_eq!(report.families[0].canonical, "raw_symbol");
    assert_eq!(report.families[0].aliases["kebab-case"], "raw-symbol");
    assert!(report.metrics.token_savings_percent > 0.0);
}

#[test]
fn facade_exports_core_public_types_by_their_old_paths() {
    let parsed: parser::ParsedName = parser::parse("create_user_profile", Convention::SnakeCase);
    assert_eq!(parsed.tags, vec!["create", "user", "profile"]);

    let aliases: alias::AliasResult = alias::generate_aliases("create_user_profile", None);
    assert_eq!(aliases.aliases["PascalCase"], "CreateUserProfile");

    let tag_family: family::TagFamily = family::generate_family("auth0__user__validate");
    let dimension: &family::TagDimension = &tag_family.dimensions[0];
    let surface: &family::SurfaceExample = &tag_family.examples[0];
    assert_eq!(dimension.canonical, "auth0");
    assert_eq!(surface.convention, "snake_case");

    let occurrence = family::FamilyOccurrence {
        identifier: "create_user".to_string(),
        file: "src/lib.rs".into(),
        line: 1,
        column: 0,
        convention: "snake_case".to_string(),
        tags: vec!["create".to_string(), "user".to_string()],
        role: Some("factory".to_string()),
        shape: None,
        context: Some("function".to_string()),
    };
    let summaries: Vec<family::TagFamilySummary> = family::summarize_occurrences([occurrence], 1);
    let summary_example: &family::FamilySummaryExample = &summaries[0].examples[0];
    assert_eq!(summary_example.context.as_deref(), Some("function"));

    let prose: prose::ProseResult = prose::to_prose("is_valid_email");
    assert_eq!(prose.prose, "Checks if email is valid");

    let normalized: query::NormalizedQuery = query::normalize_query("rawSymbol output");
    let first_tag: &query::QueryTag = &normalized.tags[0];
    assert_eq!(first_tag.tag, "raw");

    let rows = vec![compression::RawSymbolRow {
        identifier: "rawSymbol".to_string(),
        file: "src/search.ts".into(),
        line: 7,
        column: 0,
        context: Some("field".to_string()),
    }];
    let report: compression::CompressionReport = compression::build_report(&rows);
    let metrics: compression::CompressionMetrics = report.metrics.clone();
    let family_preview: &compression::CompressionFamilyPreview = &report.families[0];
    let family_example: &compression::CompressionFamilyExample = &family_preview.examples[0];
    assert_eq!(metrics.token_estimate, "ceil(utf8_bytes / 4)");
    assert_eq!(family_example.identifier, "rawSymbol");
}

#[test]
fn ontology_via_lib() {
    let dir = std::env::temp_dir().join("tagpath_lib_api_ontology");
    let _ = std::fs::remove_dir_all(&dir);
    let tags_dir = dir.join(".naming").join("tags");
    std::fs::create_dir_all(&tags_dir).unwrap();
    std::fs::write(
        tags_dir.join("session.md"),
        "+++\ntitle = \"Session\"\nsummary = \"Stable session tag.\"\n+++\n",
    )
    .unwrap();
    let ontology = tagpath::ontology::load_dir(&tags_dir).unwrap();
    let refs = ontology.references_for_tags(&["session".to_string()]);
    assert_eq!(refs[0].summary.as_deref(), Some("Stable session tag."));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn config_generate_via_lib() {
    let config = tagpath::config::generate_config(Some("rust"), None);
    assert!(config.contains("rust"));
}
