use crate::extract;
use crate::parser;
use serde::Serialize;
use std::path::Path;

/// A search result: an identifier whose tags are a superset of the query tags
#[derive(Debug, Serialize)]
pub struct SearchResult {
	pub file: std::path::PathBuf,
	pub line: usize,
	pub column: usize,
	pub identifier: String,
	pub convention: parser::Convention,
	pub tags: Vec<String>,
	pub role: Option<String>,
	pub shape: Option<String>,
}

/// Search for identifiers whose tags contain all of the query's tags (AND semantics)
pub fn search(query: &str, path: &Path) -> Vec<SearchResult> {
	// Parse the query to extract its tags
	let query_convention = parser::detect_convention(query);
	let query_parsed = parser::parse(query, query_convention);
	let query_tags: Vec<&str> =
		query_parsed.tags.iter().map(|s| s.as_str()).collect();
	// Extract all identifiers from the path
	let identifiers = extract::extract_from_path(path);
	// Filter: query_tags must be a subset of identifier tags
	let mut results: Vec<SearchResult> = identifiers
		.into_iter()
		.filter(|ident| {
			query_tags
				.iter()
				.all(|qt| ident.parsed.tags.iter().any(|it| it == qt))
		})
		.map(|ident| SearchResult {
			file: ident.file,
			line: ident.line,
			column: ident.column,
			identifier: ident.identifier,
			convention: ident.parsed.convention,
			tags: ident.parsed.tags,
			role: ident.parsed.role,
			shape: ident.parsed.shape,
		})
		.collect();
	// Sort by file path, then line number
	results.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));
	results
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::io::Write;

	#[test]
	fn test_search_finds_matching_tags() {
		let dir = std::env::temp_dir().join("tagpath_test_search");
		let _ = std::fs::create_dir_all(&dir);
		let file = dir.join("sample.rs");
		{
			let mut f = std::fs::File::create(&file).unwrap();
			writeln!(f, "fn create_user() {{}}").unwrap();
			writeln!(f, "fn delete_user() {{}}").unwrap();
			writeln!(f, "fn create_post() {{}}").unwrap();
		}
		let results = search("user", &dir);
		let idents: Vec<&str> =
			results.iter().map(|r| r.identifier.as_str()).collect();
		assert!(idents.contains(&"create_user"));
		assert!(idents.contains(&"delete_user"));
		assert!(!idents.contains(&"create_post"));
		let _ = std::fs::remove_dir_all(&dir);
	}

	#[test]
	fn test_search_and_semantics() {
		let dir = std::env::temp_dir().join("tagpath_test_search_and");
		let _ = std::fs::create_dir_all(&dir);
		let file = dir.join("sample.rs");
		{
			let mut f = std::fs::File::create(&file).unwrap();
			writeln!(f, "fn create_user() {{}}").unwrap();
			writeln!(f, "fn create_post() {{}}").unwrap();
			writeln!(f, "fn delete_user() {{}}").unwrap();
		}
		// Search for "create_user" — both tags must match
		let results = search("create_user", &dir);
		let idents: Vec<&str> =
			results.iter().map(|r| r.identifier.as_str()).collect();
		assert!(idents.contains(&"create_user"));
		assert!(!idents.contains(&"create_post"));
		assert!(!idents.contains(&"delete_user"));
		let _ = std::fs::remove_dir_all(&dir);
	}

	#[test]
	fn test_cross_language_search() {
		// The killer feature: search for a concept across languages
		// All these identifiers share the tags [validate, user]
		let dir =
			std::env::temp_dir().join("tagpath_test_cross_lang");
		let _ = std::fs::create_dir_all(&dir);

		// Python: snake_case
		{
			let mut f =
				std::fs::File::create(dir.join("auth.py")).unwrap();
			writeln!(f, "def validate_user(name):").unwrap();
			writeln!(f, "    pass").unwrap();
		}
		// TypeScript: camelCase
		{
			let mut f =
				std::fs::File::create(dir.join("auth.ts")).unwrap();
			writeln!(f, "function validateUser(name: string) {{}}")
				.unwrap();
		}
		// Go: PascalCase
		{
			let mut f =
				std::fs::File::create(dir.join("auth.go")).unwrap();
			writeln!(f, "func ValidateUser(name string) error {{}}")
				.unwrap();
		}
		// Rust: snake_case
		{
			let mut f =
				std::fs::File::create(dir.join("auth.rs")).unwrap();
			writeln!(f, "fn validate_user(name: &str) {{}}").unwrap();
		}
		// CSS: kebab-case
		{
			let mut f = std::fs::File::create(dir.join("auth.css"))
				.unwrap();
			writeln!(f, ".validate-user {{ color: red; }}").unwrap();
		}

		// Search using snake_case query — should find ALL conventions
		let results = search("validate_user", &dir);
		let idents: Vec<&str> =
			results.iter().map(|r| r.identifier.as_str()).collect();

		assert!(
			idents.contains(&"validate_user"),
			"should find Python snake_case: {idents:?}"
		);
		assert!(
			idents.contains(&"validateUser"),
			"should find TypeScript camelCase: {idents:?}"
		);
		assert!(
			idents.contains(&"ValidateUser"),
			"should find Go PascalCase: {idents:?}"
		);
		assert!(
			idents.contains(&"validate-user"),
			"should find CSS kebab-case: {idents:?}"
		);

		// Search using camelCase query — same results
		let results2 = search("validateUser", &dir);
		let idents2: Vec<&str> =
			results2.iter().map(|r| r.identifier.as_str()).collect();
		assert!(
			idents2.contains(&"validate_user"),
			"camelCase query should find snake_case: {idents2:?}"
		);
		assert!(
			idents2.contains(&"ValidateUser"),
			"camelCase query should find PascalCase: {idents2:?}"
		);

		// Search using PascalCase query — same results
		let results3 = search("ValidateUser", &dir);
		let idents3: Vec<&str> =
			results3.iter().map(|r| r.identifier.as_str()).collect();
		assert!(
			idents3.contains(&"validate_user"),
			"PascalCase query should find snake_case: {idents3:?}"
		);
		assert!(
			idents3.contains(&"validateUser"),
			"PascalCase query should find camelCase: {idents3:?}"
		);

		let _ = std::fs::remove_dir_all(&dir);
	}

	#[test]
	fn test_cross_language_partial_tag_match() {
		// Search for just "user" should find identifiers containing
		// the user tag across all languages and conventions
		let dir = std::env::temp_dir()
			.join("tagpath_test_cross_lang_partial");
		let _ = std::fs::create_dir_all(&dir);

		// Different languages, different conventions, all contain "user"
		{
			let mut f =
				std::fs::File::create(dir.join("models.py")).unwrap();
			writeln!(f, "def create_user_profile():").unwrap();
			writeln!(f, "    pass").unwrap();
		}
		{
			let mut f =
				std::fs::File::create(dir.join("api.ts")).unwrap();
			writeln!(f, "function fetchUserData() {{}}").unwrap();
			writeln!(f, "function fetchPostData() {{}}").unwrap();
		}
		{
			let mut f =
				std::fs::File::create(dir.join("handler.go")).unwrap();
			writeln!(f, "func GetUserByID() error {{}}").unwrap();
		}
		{
			let mut f = std::fs::File::create(dir.join("style.css"))
				.unwrap();
			writeln!(f, ".user-avatar {{ width: 40px; }}").unwrap();
		}

		let results = search("user", &dir);
		let idents: Vec<&str> =
			results.iter().map(|r| r.identifier.as_str()).collect();

		assert!(
			idents.contains(&"create_user_profile"),
			"should find Python: {idents:?}"
		);
		assert!(
			idents.contains(&"fetchUserData"),
			"should find TS: {idents:?}"
		);
		assert!(
			!idents.contains(&"fetchPostData"),
			"should NOT find unrelated: {idents:?}"
		);
		assert!(
			idents.contains(&"GetUserByID"),
			"should find Go: {idents:?}"
		);
		assert!(
			idents.contains(&"user-avatar"),
			"should find CSS: {idents:?}"
		);

		let _ = std::fs::remove_dir_all(&dir);
	}
}
