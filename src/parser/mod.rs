use serde::Serialize;
use std::str::FromStr;

/// Detected or specified naming convention
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Convention {
	SnakeCase,
	CamelCase,
	PascalCase,
	KebabCase,
	UpperSnakeCase,
}

impl FromStr for Convention {
	type Err = String;
	fn from_str(s: &str) -> Result<Self, Self::Err> {
		match s {
			"snake_case" | "snake" => Ok(Convention::SnakeCase),
			"camelCase" | "camel" => Ok(Convention::CamelCase),
			"PascalCase" | "pascal" => Ok(Convention::PascalCase),
			"kebab-case" | "kebab" => Ok(Convention::KebabCase),
			"UPPER_SNAKE_CASE" | "upper_snake" | "screaming" => {
				Ok(Convention::UpperSnakeCase)
			}
			_ => Err(format!("unknown convention: {s}")),
		}
	}
}

/// Result of parsing a name into tags
#[derive(Debug, Serialize)]
pub struct ParsedName {
	/// Original input
	pub original: String,
	/// Detected convention
	pub convention: Convention,
	/// All tags (flattened, lowercase)
	pub tags: Vec<String>,
	/// Namespace dimensions (split on __)
	pub namespaces: Vec<Vec<String>>,
	/// Detected role (factory, hook, setter, etc.)
	pub role: Option<String>,
	/// Detected data shape (array, record, map)
	pub shape: Option<String>,
}

/// Detect the naming convention of an identifier
pub fn detect_convention(name: &str) -> Convention {
	if name.contains("__") || name.contains('_') {
		if name == name.to_uppercase() && name.contains('_') {
			Convention::UpperSnakeCase
		} else {
			Convention::SnakeCase
		}
	} else if name.contains('-') {
		Convention::KebabCase
	} else if name.starts_with(|c: char| c.is_uppercase()) {
		Convention::PascalCase
	} else {
		Convention::CamelCase
	}
}

/// Parse a name into its constituent tags
pub fn parse(name: &str, convention: Convention) -> ParsedName {
	let raw_tags = tokenize(name, convention);
	// Split on namespace boundaries (__ was split into tags as a boundary marker)
	let namespaces = extract_namespaces(name, convention);
	// Detect role from prefix/suffix patterns
	let role = detect_role(&raw_tags);
	// Detect data shape from suffix
	let shape = detect_shape(&raw_tags);
	// Normalize all tags to lowercase
	let tags: Vec<String> =
		raw_tags.iter().map(|t| t.to_lowercase()).collect();
	ParsedName {
		original: name.to_string(),
		convention,
		tags,
		namespaces,
		role,
		shape,
	}
}

/// Tokenize a name into individual tags based on convention
fn tokenize(name: &str, convention: Convention) -> Vec<String> {
	// Always split on underscores first, then apply camelCase splitting
	// to each segment. This handles mixed patterns like createContext_auth.
	let normalized = name.replace("__", "\x00");
	let segments: Vec<&str> = normalized
		.split(|c| c == '_' || c == '\x00')
		.filter(|s| !s.is_empty())
		.collect();
	match convention {
		Convention::KebabCase => name
			.split('-')
			.filter(|s| !s.is_empty())
			.map(|s| s.to_string())
			.collect(),
		_ => {
			// For each segment, also split on camelCase boundaries
			segments
				.into_iter()
				.flat_map(|seg| {
					if seg.chars().any(|c| c.is_uppercase())
						&& seg.chars().any(|c| c.is_lowercase())
					{
						split_camel_case(seg)
					} else {
						vec![seg.to_string()]
					}
				})
				.collect()
		}
	}
}

/// Split camelCase/PascalCase into tags, also handling _ joins
fn split_camel_case(name: &str) -> Vec<String> {
	let mut tags = Vec::new();
	let mut current = String::new();
	let chars: Vec<char> = name.chars().collect();
	let len = chars.len();
	for i in 0..len {
		let c = chars[i];
		if c == '_' {
			// Underscore join — flush current tag
			if !current.is_empty() {
				tags.push(current.clone());
				current.clear();
			}
			continue;
		}
		if c.is_uppercase() {
			// Check if this is a case boundary
			if !current.is_empty() {
				// If previous char was lowercase, or next char is lowercase
				// (handles "HTMLElement" → ["HTML", "Element"])
				let prev_lower = i > 0 && chars[i - 1].is_lowercase();
				let next_lower =
					i + 1 < len && chars[i + 1].is_lowercase();
				if prev_lower || (current.len() > 1 && next_lower) {
					tags.push(current.clone());
					current.clear();
				}
			}
		}
		current.push(c);
	}
	if !current.is_empty() {
		tags.push(current);
	}
	tags
}

/// Extract namespace dimensions (__ separated groups)
fn extract_namespaces(
	name: &str,
	convention: Convention,
) -> Vec<Vec<String>> {
	match convention {
		Convention::SnakeCase | Convention::UpperSnakeCase => {
			let dimensions: Vec<&str> = name.split("__").collect();
			if dimensions.len() <= 1 {
				return vec![];
			}
			dimensions
				.iter()
				.map(|dim| {
					dim.split('_')
						.filter(|s| !s.is_empty())
						.map(|s| s.to_lowercase())
						.collect()
				})
				.collect()
		}
		_ => {
			// camelCase/PascalCase don't have __ namespacing by default
			// but could have _ joins creating dimension boundaries
			vec![]
		}
	}
}

/// Detect the role of a name from prefix/suffix patterns
fn detect_role(tags: &[String]) -> Option<String> {
	if tags.is_empty() {
		return None;
	}
	let first = tags[0].to_lowercase();
	let last = tags.last().unwrap().to_lowercase();
	match first.as_str() {
		"create" | "make" | "new" | "build" => {
			Some("factory".to_string())
		}
		"use" => Some("hook".to_string()),
		"set" => Some("setter".to_string()),
		"get" => Some("getter".to_string()),
		"is" | "has" | "can" | "should" => {
			Some("predicate".to_string())
		}
		"on" => Some("handler".to_string()),
		"validate" | "check" | "verify" => {
			Some("validator".to_string())
		}
		_ => match last.as_str() {
			"validate" | "check" | "verify" => {
				Some("validator".to_string())
			}
			_ => None,
		},
	}
}

/// Detect data shape from suffix tags
fn detect_shape(tags: &[String]) -> Option<String> {
	if tags.is_empty() {
		return None;
	}
	let last = tags.last().unwrap().to_lowercase();
	match last.as_str() {
		"a" | "a1" | "a2" | "a3" | "list" | "array" => {
			Some("array".to_string())
		}
		"r" | "record" => Some("record".to_string()),
		"m" | "map" => Some("map".to_string()),
		"set" => {
			// "set" could be setter or Set data structure — check context
			// If first tag is also "set", it's a setter
			if tags.len() > 1 && tags[0].to_lowercase() == "set" {
				None // setter, not shape
			} else {
				Some("set".to_string())
			}
		}
		_ => {
			// Check for $ suffix (reactive signal)
			if tags
				.last()
				.map_or(false, |t| t.ends_with('$'))
			{
				Some("signal".to_string())
			} else {
				None
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_detect_convention() {
		assert_eq!(
			detect_convention("person_name"),
			Convention::SnakeCase
		);
		assert_eq!(
			detect_convention("personName"),
			Convention::CamelCase
		);
		assert_eq!(
			detect_convention("PersonName"),
			Convention::PascalCase
		);
		assert_eq!(
			detect_convention("person-name"),
			Convention::KebabCase
		);
		assert_eq!(
			detect_convention("PERSON_NAME"),
			Convention::UpperSnakeCase
		);
		assert_eq!(
			detect_convention("auth0__user__validate"),
			Convention::SnakeCase
		);
	}

	#[test]
	fn test_parse_snake_case() {
		let p = parse("person_name", Convention::SnakeCase);
		assert_eq!(p.tags, vec!["person", "name"]);
		assert!(p.namespaces.is_empty());
	}

	#[test]
	fn test_parse_namespace() {
		let p =
			parse("auth0__user__validate", Convention::SnakeCase);
		assert_eq!(p.tags, vec!["auth0", "user", "validate"]);
		assert_eq!(p.namespaces.len(), 3);
		assert_eq!(p.namespaces[0], vec!["auth0"]);
		assert_eq!(p.namespaces[1], vec!["user"]);
		assert_eq!(p.namespaces[2], vec!["validate"]);
		assert_eq!(p.role, Some("validator".to_string()));
	}

	#[test]
	fn test_parse_camel_case() {
		let p = parse("personName", Convention::CamelCase);
		assert_eq!(p.tags, vec!["person", "name"]);
	}

	#[test]
	fn test_parse_pascal_case() {
		let p = parse("PersonName", Convention::PascalCase);
		assert_eq!(p.tags, vec!["person", "name"]);
	}

	#[test]
	fn test_parse_html_element() {
		let p = parse("HTMLElement", Convention::PascalCase);
		assert_eq!(p.tags, vec!["html", "element"]);
	}

	#[test]
	fn test_parse_factory() {
		let p = parse("create_memo", Convention::SnakeCase);
		assert_eq!(p.tags, vec!["create", "memo"]);
		assert_eq!(p.role, Some("factory".to_string()));
	}

	#[test]
	fn test_parse_signal() {
		let p = parse("pathname$", Convention::SnakeCase);
		assert!(p.tags.contains(&"pathname$".to_string()));
	}

	#[test]
	fn test_parse_array_shape() {
		let p = parse("post_a", Convention::SnakeCase);
		assert_eq!(p.shape, Some("array".to_string()));
	}

	#[test]
	fn test_semantic_equivalence() {
		let snake = parse("person_name", Convention::SnakeCase);
		let camel = parse("personName", Convention::CamelCase);
		let pascal = parse("PersonName", Convention::PascalCase);
		// All decompose to the same canonical tags
		assert_eq!(snake.tags, camel.tags);
		assert_eq!(camel.tags, pascal.tags);
		assert_eq!(snake.tags, vec!["person", "name"]);
	}

	#[test]
	fn test_camel_with_underscore_extension() {
		// camelCase_immutable_tag pattern
		let p = parse("createContext_auth", Convention::CamelCase);
		assert_eq!(
			p.tags,
			vec!["create", "context", "auth"]
		);
	}

	#[test]
	fn test_multi_dimension() {
		let p = parse(
			"highest_net_worth__company_person_name",
			Convention::SnakeCase,
		);
		assert_eq!(
			p.tags,
			vec![
				"highest", "net", "worth", "company", "person",
				"name"
			]
		);
		assert_eq!(p.namespaces.len(), 2);
		assert_eq!(
			p.namespaces[0],
			vec!["highest", "net", "worth"]
		);
		assert_eq!(
			p.namespaces[1],
			vec!["company", "person", "name"]
		);
	}
}
