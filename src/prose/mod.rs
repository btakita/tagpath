use serde::Serialize;

use crate::parser;

/// Result of converting a name to prose
#[derive(Debug, Serialize)]
pub struct ProseResult {
	/// Original input
	pub original: String,
	/// Generated prose description
	pub prose: String,
	/// Canonical tags
	pub tags: Vec<String>,
	/// Detected role (factory, getter, etc.)
	pub role: Option<String>,
	/// Detected shape (array, record, etc.)
	pub shape: Option<String>,
}

/// Role prefix words that get stripped from the core tags
const ROLE_PREFIXES: &[&str] = &[
	"create", "make", "new", "build", // factory
	"use",                            // hook
	"set",                            // setter
	"get",                            // getter
	"is", "has", "can", "should",     // predicate
	"on",                             // handler
	"validate", "check", "verify",    // validator
];

/// Shape suffix words that get stripped from the core tags
const SHAPE_SUFFIXES: &[&str] = &[
	"a", "a1", "a2", "a3", "list", "array", // array
	"r", "record",                           // record
	"m", "map",                              // map
	"set",                                   // set
];

/// Convert a tag-based identifier to a human-readable prose description.
pub fn to_prose(name: &str) -> ProseResult {
	let convention = parser::detect_convention(name);
	let parsed = parser::parse(name, convention);
	let role = parsed.role.clone();
	let shape = parsed.shape.clone();
	let tags = parsed.tags.clone();
	// Build core tags by stripping role prefix and shape suffix
	let mut core = tags.clone();
	// Track the role prefix word for predicate reinsertion
	let role_word = if !core.is_empty()
		&& ROLE_PREFIXES.contains(&core[0].as_str())
		&& role.is_some()
	{
		let w = core.remove(0);
		Some(w)
	} else {
		None
	};
	// Strip shape suffix
	if !core.is_empty() && shape.is_some() {
		let last = core.last().unwrap().as_str();
		if SHAPE_SUFFIXES.contains(&last) {
			core.pop();
		}
	}
	// Build noun phrase from core tags
	let noun = core.join(" ");
	// Build the prose string
	let prose =
		build_prose(&role, role_word.as_deref(), &noun, &shape);
	ProseResult {
		original: name.to_string(),
		prose,
		tags,
		role,
		shape,
	}
}

/// Build the final prose string from role, noun, and shape components.
fn build_prose(
	role: &Option<String>,
	role_word: Option<&str>,
	noun: &str,
	shape: &Option<String>,
) -> String {
	// Apply shape wrapper to noun phrase first
	let shaped_noun = apply_shape(noun, shape.as_deref());
	// Then apply role prefix
	match role.as_deref() {
		Some("factory") => format!("Creates a {shaped_noun}"),
		Some("hook") => format!("Uses {shaped_noun}"),
		Some("setter") => format!("Sets {shaped_noun}"),
		Some("getter") => format!("Gets {shaped_noun}"),
		Some("predicate") => {
			let predicate_phrase =
				build_predicate_phrase(role_word, &shaped_noun);
			format!("Checks if {predicate_phrase}")
		}
		Some("handler") => format!("Handles {shaped_noun}"),
		Some("validator") => format!("Validates {shaped_noun}"),
		_ => {
			// No role — capitalize the shaped noun phrase
			// (apply_shape already handles "Array of ..." casing)
			if shaped_noun.starts_with("Array of ") {
				shaped_noun
			} else {
				capitalize_first(&shaped_noun)
			}
		}
	}
}

/// Apply shape modifier to a noun phrase.
///
/// - array: "Array of {noun}s" (simple pluralization)
/// - record/map/set/signal: "{noun} {shape}"
/// - none: noun unchanged
fn apply_shape(noun: &str, shape: Option<&str>) -> String {
	match shape {
		Some("array") => format!("Array of {noun}s"),
		Some("record") => format!("{noun} record"),
		Some("map") => format!("{noun} map"),
		Some("set") => format!("{noun} set"),
		Some("signal") => format!("{noun} signal"),
		_ => noun.to_string(),
	}
}

/// Build the predicate phrase by rearranging tags for natural English.
///
/// For "is" predicates: "valid email" -> "email is valid"
/// (last word is the subject noun, moved to front, predicate word reinserted)
fn build_predicate_phrase(
	role_word: Option<&str>,
	noun: &str,
) -> String {
	let words: Vec<&str> = noun.split_whitespace().collect();
	match role_word {
		Some(w @ ("is" | "has" | "can" | "should"))
			if words.len() > 1 =>
		{
			// Move the last word (subject noun) to front,
			// reinsert predicate word, keep remaining as modifiers
			let subject = words.last().unwrap();
			let modifiers = &words[..words.len() - 1];
			format!(
				"{} {} {}",
				subject,
				w,
				modifiers.join(" ")
			)
		}
		_ => noun.to_string(),
	}
}

/// Capitalize the first character of a string.
fn capitalize_first(s: &str) -> String {
	let mut chars = s.chars();
	match chars.next() {
		None => String::new(),
		Some(c) => {
			c.to_uppercase().to_string() + chars.as_str()
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_factory_role() {
		let result = to_prose("create_user_profile");
		assert_eq!(result.prose, "Creates a user profile");
		assert_eq!(result.role, Some("factory".to_string()));
	}

	#[test]
	fn test_predicate_role() {
		let result = to_prose("is_valid_email");
		assert_eq!(result.prose, "Checks if email is valid");
		assert_eq!(result.role, Some("predicate".to_string()));
	}

	#[test]
	fn test_getter_role() {
		let result = to_prose("getUserById");
		assert_eq!(result.prose, "Gets user by id");
		assert_eq!(result.role, Some("getter".to_string()));
	}

	#[test]
	fn test_array_shape() {
		let result = to_prose("user_name_a");
		assert_eq!(result.prose, "Array of user names");
		assert_eq!(result.shape, Some("array".to_string()));
	}

	#[test]
	fn test_no_role_no_shape() {
		let result = to_prose("PersonName");
		assert_eq!(result.prose, "Person name");
		assert_eq!(result.role, None);
		assert_eq!(result.shape, None);
	}

	#[test]
	fn test_setter_upper_snake() {
		let result = to_prose("SET_MAX_RETRIES");
		assert_eq!(result.prose, "Sets max retries");
		assert_eq!(result.role, Some("setter".to_string()));
	}
}
