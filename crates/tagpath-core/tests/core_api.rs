use std::path::PathBuf;

use tagpath_core::{alias, compression, family, parser, prose, query};

#[test]
fn core_exports_identifier_semantics() {
    let parsed = parser::parse("create_user_profile", parser::Convention::SnakeCase);
    assert_eq!(parsed.tags, vec!["create", "user", "profile"]);
    assert_eq!(parsed.role.as_deref(), Some("factory"));

    let aliases = alias::generate_aliases("create_user_profile", None);
    assert_eq!(aliases.aliases["camelCase"], "createUserProfile");

    let family = family::generate_family("auth0__user__validate");
    assert_eq!(family.canonical, "auth0_user_validate");
    assert_eq!(family.dimensions.len(), 3);

    let prose = prose::to_prose("is_valid_email");
    assert_eq!(prose.prose, "Checks if email is valid");

    let tags: Vec<String> = query::normalize_query("Find raw_symbol output")
        .tags
        .into_iter()
        .map(|tag| tag.tag)
        .collect();
    assert_eq!(&tags[..2], &["raw", "symbol"]);
}

#[test]
fn core_builds_compression_reports_without_native_inputs() {
    let rows = [
        compression::RawSymbolRow {
            identifier: "raw_symbol".to_string(),
            file: PathBuf::from("src/search.rs"),
            line: 42,
            column: 0,
            context: Some("field".to_string()),
        },
        compression::RawSymbolRow {
            identifier: "rawSymbol".to_string(),
            file: PathBuf::from("src/search.ts"),
            line: 7,
            column: 0,
            context: Some("field".to_string()),
        },
    ];

    let report = compression::build_report(&rows);
    assert_eq!(report.raw_symbol_count, 2);
    assert_eq!(report.family_count, 1);
    assert_eq!(report.families[0].canonical, "raw_symbol");
}
