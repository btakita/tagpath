use crate::config::NamingConfig;
use crate::extract;
use crate::parser::{self, Convention};
use crate::treesitter::IdentifierContext;
use serde::Serialize;
use std::path::{Path, PathBuf};

/// A lint violation found during convention checking
#[derive(Debug, Serialize)]
pub struct LintViolation {
	pub file: PathBuf,
	pub line: usize,
	pub column: usize,
	pub identifier: String,
	pub expected_convention: String,
	pub actual_convention: String,
	pub suggested_fix: Option<String>,
}

/// Find `.naming.toml` by walking up from the given directory
pub fn find_config(start: &Path) -> Option<PathBuf> {
	let mut dir = if start.is_file() {
		start.parent()?.to_path_buf()
	} else {
		start.to_path_buf()
	};
	loop {
		let config = dir.join(".naming.toml");
		if config.exists() {
			return Some(config);
		}
		if !dir.pop() {
			return None;
		}
	}
}

/// Lint identifiers in a path against the given naming config
pub fn lint(
	path: &Path,
	config: &NamingConfig,
) -> Vec<LintViolation> {
	// Extract identifiers with AST context
	let identifiers =
		extract::extract_from_path_with_mode(path, true);
	let contexts = match &config.contexts {
		Some(c) => c,
		None => return vec![], // No context rules = no violations
	};
	let mut violations = Vec::new();
	for ident in &identifiers {
		let ctx = match &ident.context {
			Some(c) => c,
			None => continue, // No context info, skip
		};
		// Map IdentifierContext to config context name
		let ctx_name = match ctx {
			IdentifierContext::Function => "function",
			IdentifierContext::Variable => "variable",
			IdentifierContext::Type => "type",
			IdentifierContext::Constant => "constant",
			IdentifierContext::Import => "import",
			IdentifierContext::Parameter => "parameter",
			IdentifierContext::Field => "field",
			IdentifierContext::Other => continue,
		};
		let ctx_config = match contexts.get(ctx_name) {
			Some(c) => c,
			None => continue, // No rule for this context
		};
		let expected_convention = &ctx_config.convention;
		let expected = match expected_convention.parse::<Convention>()
		{
			Ok(c) => c,
			Err(_) => continue, // Unknown convention name
		};
		let actual =
			parser::detect_convention(&ident.identifier);
		if actual != expected {
			let suggested =
				parser::join_tags(&ident.parsed.tags, expected);
			violations.push(LintViolation {
				file: ident.file.clone(),
				line: ident.line,
				column: ident.column,
				identifier: ident.identifier.clone(),
				expected_convention: expected_convention.clone(),
				actual_convention: format!("{}", actual),
				suggested_fix: Some(suggested),
			});
		}
	}
	violations
}

#[cfg(test)]
mod tests {
	use super::*;
	#[cfg(feature = "lang-rust")]
	use crate::config::ContextConfig;
	#[cfg(feature = "lang-rust")]
	use std::collections::HashMap;
	use std::io::Write;

	#[cfg(feature = "lang-rust")]
	fn make_config(
		contexts: Vec<(&str, &str)>,
	) -> NamingConfig {
		let mut ctx_map = HashMap::new();
		for (name, convention) in contexts {
			ctx_map.insert(
				name.to_string(),
				ContextConfig {
					convention: convention.to_string(),
					prefix: None,
					suffix: None,
				},
			);
		}
		NamingConfig {
			version: 1,
			name: "test".to_string(),
			extends: None,
			convention: "snake_case".to_string(),
			immutable: None,
			singular: None,
			vectors: None,
			patterns: None,
			externals: None,
			packages: None,
			contexts: Some(ctx_map),
			tags: None,
		}
	}

	#[test]
	fn test_suggest_fix_snake_case() {
		let tags =
			vec!["parse".to_string(), "name".to_string()];
		assert_eq!(
			parser::join_tags(&tags, Convention::SnakeCase),
			"parse_name"
		);
	}

	#[test]
	fn test_suggest_fix_pascal_case() {
		let tags =
			vec!["parse".to_string(), "name".to_string()];
		assert_eq!(
			parser::join_tags(&tags, Convention::PascalCase),
			"ParseName"
		);
	}

	#[test]
	fn test_suggest_fix_camel_case() {
		let tags =
			vec!["parse".to_string(), "name".to_string()];
		assert_eq!(
			parser::join_tags(&tags, Convention::CamelCase),
			"parseName"
		);
	}

	#[test]
	fn test_suggest_fix_upper_snake() {
		let tags =
			vec!["max".to_string(), "retries".to_string()];
		assert_eq!(
			parser::join_tags(&tags, Convention::UpperSnakeCase),
			"MAX_RETRIES"
		);
	}

	#[cfg(feature = "lang-rust")]
	#[test]
	fn test_lint_finds_violations() {
		let dir = std::env::temp_dir().join("tagpath_test_lint");
		let _ = std::fs::create_dir_all(&dir);
		let file = dir.join("bad.rs");
		{
			let mut f = std::fs::File::create(&file).unwrap();
			// PascalCase function name — should be snake_case
			writeln!(
				f,
				"fn ParseName(user_input: &str) {{}}"
			)
			.unwrap();
		}
		let config = make_config(vec![
			("function", "snake_case"),
			("parameter", "snake_case"),
		]);
		let violations = lint(&file, &config);
		// ParseName should be flagged
		let flagged: Vec<&str> = violations
			.iter()
			.map(|v| v.identifier.as_str())
			.collect();
		assert!(
			flagged.contains(&"ParseName"),
			"Expected ParseName to be flagged, got: {:?}",
			flagged
		);
		// user_input should NOT be flagged (already snake_case)
		assert!(
			!flagged.contains(&"user_input"),
			"user_input should not be flagged"
		);
		// Check suggested fix
		let violation = violations
			.iter()
			.find(|v| v.identifier == "ParseName")
			.unwrap();
		assert_eq!(
			violation.suggested_fix,
			Some("parse_name".to_string())
		);
		let _ = std::fs::remove_dir_all(&dir);
	}

	#[cfg(feature = "lang-rust")]
	#[test]
	fn test_lint_no_violations() {
		let dir =
			std::env::temp_dir().join("tagpath_test_lint_ok");
		let _ = std::fs::create_dir_all(&dir);
		let file = dir.join("good.rs");
		{
			let mut f = std::fs::File::create(&file).unwrap();
			writeln!(
				f,
				"fn parse_name(user_input: &str) {{}}"
			)
			.unwrap();
		}
		let config = make_config(vec![
			("function", "snake_case"),
			("parameter", "snake_case"),
		]);
		let violations = lint(&file, &config);
		assert!(
			violations.is_empty(),
			"Expected no violations, got: {:?}",
			violations
				.iter()
				.map(|v| &v.identifier)
				.collect::<Vec<_>>()
		);
		let _ = std::fs::remove_dir_all(&dir);
	}

	#[test]
	fn test_find_config() {
		let dir = std::env::temp_dir()
			.join("tagpath_test_find_config");
		let _ = std::fs::create_dir_all(&dir);
		let sub = dir.join("src");
		let _ = std::fs::create_dir_all(&sub);
		// Create .naming.toml in parent
		let config_path = dir.join(".naming.toml");
		{
			let mut f =
				std::fs::File::create(&config_path).unwrap();
			writeln!(f, "version = 1\nname = \"test\"")
				.unwrap();
		}
		let found = find_config(&sub);
		assert_eq!(found, Some(config_path));
		let _ = std::fs::remove_dir_all(&dir);
	}

	#[cfg(feature = "lang-rust")]
	#[test]
	fn test_lint_type_convention() {
		let dir = std::env::temp_dir()
			.join("tagpath_test_lint_type");
		let _ = std::fs::create_dir_all(&dir);
		let file = dir.join("bad_type.rs");
		{
			let mut f = std::fs::File::create(&file).unwrap();
			// snake_case struct name — should be PascalCase
			writeln!(f, "struct user_service {{}}").unwrap();
		}
		let config = make_config(vec![
			("type", "PascalCase"),
		]);
		let violations = lint(&file, &config);
		let flagged: Vec<&str> = violations
			.iter()
			.map(|v| v.identifier.as_str())
			.collect();
		assert!(
			flagged.contains(&"user_service"),
			"Expected user_service to be flagged, got: {:?}",
			flagged
		);
		let violation = violations
			.iter()
			.find(|v| v.identifier == "user_service")
			.unwrap();
		assert_eq!(
			violation.suggested_fix,
			Some("UserService".to_string())
		);
		let _ = std::fs::remove_dir_all(&dir);
	}
}
