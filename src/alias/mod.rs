use serde::Serialize;
use std::collections::BTreeMap;

use crate::parser::{
	self, Convention, ALL_CONVENTIONS,
};

/// A single alias entry: convention name → generated identifier
#[derive(Debug, Serialize)]
pub struct AliasResult {
	/// The canonical tags extracted from the input
	pub tags: Vec<String>,
	/// Map of convention name → generated alias
	pub aliases: BTreeMap<String, String>,
}

/// Generate aliases for a name in all (or a filtered) set of conventions.
///
/// Parses the input identifier, extracts tags, then reconstructs
/// the identifier in each target convention using `join_tags`.
pub fn generate_aliases(
	name: &str,
	target_convention: Option<Convention>,
) -> AliasResult {
	let detected = parser::detect_convention(name);
	let parsed = parser::parse(name, detected);
	let mut aliases = BTreeMap::new();
	match target_convention {
		Some(conv) => {
			aliases.insert(
				conv.to_string(),
				parser::join_tags(&parsed.tags, conv),
			);
		}
		None => {
			for conv in ALL_CONVENTIONS {
				aliases.insert(
					conv.to_string(),
					parser::join_tags(&parsed.tags, conv),
				);
			}
		}
	}
	AliasResult {
		tags: parsed.tags,
		aliases,
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_all_aliases_from_snake_case() {
		let result = generate_aliases("person_name", None);
		assert_eq!(result.tags, vec!["person", "name"]);
		assert_eq!(result.aliases["snake_case"], "person_name");
		assert_eq!(result.aliases["camelCase"], "personName");
		assert_eq!(result.aliases["PascalCase"], "PersonName");
		assert_eq!(result.aliases["kebab-case"], "person-name");
		assert_eq!(
			result.aliases["UPPER_SNAKE_CASE"],
			"PERSON_NAME"
		);
		assert_eq!(result.aliases["Ada_Case"], "Person_Name");
		assert_eq!(result.aliases.len(), 6);
	}

	#[test]
	fn test_all_aliases_from_camel_case() {
		let result = generate_aliases("personName", None);
		assert_eq!(result.tags, vec!["person", "name"]);
		assert_eq!(result.aliases["snake_case"], "person_name");
		assert_eq!(result.aliases["camelCase"], "personName");
		assert_eq!(result.aliases["PascalCase"], "PersonName");
		assert_eq!(result.aliases["kebab-case"], "person-name");
		assert_eq!(
			result.aliases["UPPER_SNAKE_CASE"],
			"PERSON_NAME"
		);
		assert_eq!(result.aliases["Ada_Case"], "Person_Name");
	}

	#[test]
	fn test_filter_single_convention() {
		let result = generate_aliases(
			"person_name",
			Some(Convention::CamelCase),
		);
		assert_eq!(result.aliases.len(), 1);
		assert_eq!(result.aliases["camelCase"], "personName");
	}

	#[test]
	fn test_namespace_separator() {
		let result =
			generate_aliases("auth__user__validate", None);
		assert_eq!(
			result.tags,
			vec!["auth", "user", "validate"]
		);
		assert_eq!(
			result.aliases["snake_case"],
			"auth_user_validate"
		);
		assert_eq!(
			result.aliases["camelCase"],
			"authUserValidate"
		);
		assert_eq!(
			result.aliases["PascalCase"],
			"AuthUserValidate"
		);
		assert_eq!(
			result.aliases["kebab-case"],
			"auth-user-validate"
		);
		assert_eq!(
			result.aliases["UPPER_SNAKE_CASE"],
			"AUTH_USER_VALIDATE"
		);
		assert_eq!(
			result.aliases["Ada_Case"],
			"Auth_User_Validate"
		);
	}

	#[test]
	fn test_role_prefix() {
		let result =
			generate_aliases("create_user_profile", None);
		assert_eq!(
			result.tags,
			vec!["create", "user", "profile"]
		);
		assert_eq!(
			result.aliases["snake_case"],
			"create_user_profile"
		);
		assert_eq!(
			result.aliases["camelCase"],
			"createUserProfile"
		);
		assert_eq!(
			result.aliases["PascalCase"],
			"CreateUserProfile"
		);
		assert_eq!(
			result.aliases["kebab-case"],
			"create-user-profile"
		);
		assert_eq!(
			result.aliases["UPPER_SNAKE_CASE"],
			"CREATE_USER_PROFILE"
		);
		assert_eq!(
			result.aliases["Ada_Case"],
			"Create_User_Profile"
		);
	}
}
