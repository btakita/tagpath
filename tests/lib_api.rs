use tagpath::parser::{self, ALL_CONVENTIONS, Convention};

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
fn config_generate_via_lib() {
    let config = tagpath::config::generate_config(Some("rust"), None);
    assert!(config.contains("rust"));
}
