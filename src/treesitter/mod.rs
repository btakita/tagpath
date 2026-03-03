use serde::Serialize;
use std::path::Path;
use tree_sitter::Parser;

/// The AST context in which an identifier appears
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IdentifierContext {
	/// function/method definition name
	Function,
	/// variable/parameter/field name
	Variable,
	/// type/class/struct/enum definition name
	Type,
	/// constant definition name
	Constant,
	/// import/use statement identifier
	Import,
	/// function parameter name
	Parameter,
	/// struct/class field name
	Field,
	/// unclassified identifier
	#[allow(dead_code)]
	Other,
}

impl std::fmt::Display for IdentifierContext {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			IdentifierContext::Function => write!(f, "function"),
			IdentifierContext::Variable => write!(f, "variable"),
			IdentifierContext::Type => write!(f, "type"),
			IdentifierContext::Constant => write!(f, "constant"),
			IdentifierContext::Import => write!(f, "import"),
			IdentifierContext::Parameter => write!(f, "parameter"),
			IdentifierContext::Field => write!(f, "field"),
			IdentifierContext::Other => write!(f, "other"),
		}
	}
}

/// An identifier extracted from source code with AST context
#[derive(Debug, Clone, Serialize)]
pub struct ContextualIdentifier {
	pub file: std::path::PathBuf,
	pub line: usize,
	pub column: usize,
	pub identifier: String,
	pub context: IdentifierContext,
	pub language: String,
}

/// Supported languages for tree-sitter extraction
#[derive(Debug, Clone, Copy)]
enum Language {
	Rust,
	Python,
	JavaScript,
	TypeScript,
	Tsx,
	Go,
	C,
	Cpp,
}

/// Detect language from file extension
fn detect_language(path: &Path) -> Option<Language> {
	let ext = path.extension()?.to_str()?;
	match ext {
		"rs" => Some(Language::Rust),
		"py" => Some(Language::Python),
		"js" => Some(Language::JavaScript),
		"ts" => Some(Language::TypeScript),
		"tsx" => Some(Language::Tsx),
		"go" => Some(Language::Go),
		"c" | "h" => Some(Language::C),
		"cpp" | "hpp" | "cc" | "cxx" => Some(Language::Cpp),
		_ => None,
	}
}

/// Get the tree-sitter language for a detected language
fn get_ts_language(lang: Language) -> tree_sitter::Language {
	match lang {
		Language::Rust => tree_sitter_rust::LANGUAGE.into(),
		Language::Python => tree_sitter_python::LANGUAGE.into(),
		Language::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
		Language::TypeScript => {
			tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
		}
		Language::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
		Language::Go => tree_sitter_go::LANGUAGE.into(),
		Language::C => tree_sitter_c::LANGUAGE.into(),
		Language::Cpp => tree_sitter_cpp::LANGUAGE.into(),
	}
}

/// Get the language name string
fn language_name(lang: Language) -> &'static str {
	match lang {
		Language::Rust => "rust",
		Language::Python => "python",
		Language::JavaScript => "javascript",
		Language::TypeScript => "typescript",
		Language::Tsx => "tsx",
		Language::Go => "go",
		Language::C => "c",
		Language::Cpp => "cpp",
	}
}

/// Check if a file extension is supported by tree-sitter extraction
pub fn is_supported(path: &Path) -> bool {
	detect_language(path).is_some()
}

/// Extract identifiers with AST context from a source file
pub fn extract_with_context(path: &Path) -> Vec<ContextualIdentifier> {
	let lang = match detect_language(path) {
		Some(l) => l,
		None => return vec![],
	};
	let content = match std::fs::read_to_string(path) {
		Ok(c) => c,
		Err(_) => return vec![],
	};
	let ts_lang = get_ts_language(lang);
	let mut parser = Parser::new();
	if parser.set_language(&ts_lang).is_err() {
		return vec![];
	}
	let tree = match parser.parse(&content, None) {
		Some(t) => t,
		None => return vec![],
	};
	let source = content.as_bytes();
	let lang_name = language_name(lang);
	let mut results = Vec::new();
	// DFS traversal using TreeCursor
	let mut cursor = tree.walk();
	let mut did_visit_children = false;
	loop {
		if !did_visit_children {
			let node = cursor.node();
			if let Some(ctx_ident) =
				classify_node(&node, source, lang, lang_name, path)
			{
				results.push(ctx_ident);
			}
			if cursor.goto_first_child() {
				did_visit_children = false;
				continue;
			}
		}
		if cursor.goto_next_sibling() {
			did_visit_children = false;
		} else if cursor.goto_parent() {
			did_visit_children = true;
		} else {
			break;
		}
	}
	results
}

/// Classify a node and extract its identifier if it's a definition
fn classify_node(
	node: &tree_sitter::Node,
	source: &[u8],
	lang: Language,
	lang_name: &str,
	path: &Path,
) -> Option<ContextualIdentifier> {
	let kind = node.kind();
	// Only process named (non-anonymous) nodes
	if !node.is_named() {
		return None;
	}
	match lang {
		Language::Rust => classify_rust(node, kind, source, lang_name, path),
		Language::Python => {
			classify_python(node, kind, source, lang_name, path)
		}
		Language::JavaScript => {
			classify_js(node, kind, source, lang_name, path)
		}
		Language::TypeScript | Language::Tsx => {
			classify_js(node, kind, source, lang_name, path)
		}
		Language::Go => classify_go(node, kind, source, lang_name, path),
		Language::C | Language::Cpp => {
			classify_c_cpp(node, kind, source, lang_name, path)
		}
	}
}

/// Extract identifier text from a child node by field name or node kind
fn extract_child_text<'a>(
	node: &tree_sitter::Node<'a>,
	source: &'a [u8],
	field_name: &str,
) -> Option<(&'a str, usize, usize)> {
	let child = node.child_by_field_name(field_name)?;
	let text = child.utf8_text(source).ok()?;
	if text.len() <= 1 {
		return None;
	}
	let pos = child.start_position();
	Some((text, pos.row + 1, pos.column + 1))
}

/// Find and extract a named child of a specific kind
fn extract_child_of_kind<'a>(
	node: &tree_sitter::Node<'a>,
	source: &'a [u8],
	child_kind: &str,
) -> Option<(&'a str, usize, usize)> {
	let count = node.child_count() as u32;
	for i in 0..count {
		if let Some(child) = node.child(i)
			&& child.kind() == child_kind
			&& child.is_named()
		{
			let text = child.utf8_text(source).ok()?;
			if text.len() <= 1 {
				return None;
			}
			let pos = child.start_position();
			return Some((text, pos.row + 1, pos.column + 1));
		}
	}
	None
}

fn make_ident(
	text: &str,
	line: usize,
	column: usize,
	context: IdentifierContext,
	lang_name: &str,
	path: &Path,
) -> ContextualIdentifier {
	ContextualIdentifier {
		file: path.to_path_buf(),
		line,
		column,
		identifier: text.to_string(),
		context,
		language: lang_name.to_string(),
	}
}

// ── Rust classification ──────────────────────────────────────────────

fn classify_rust(
	node: &tree_sitter::Node,
	kind: &str,
	source: &[u8],
	lang_name: &str,
	path: &Path,
) -> Option<ContextualIdentifier> {
	match kind {
		"function_item" => {
			let (text, line, col) =
				extract_child_text(node, source, "name")?;
			Some(make_ident(
				text,
				line,
				col,
				IdentifierContext::Function,
				lang_name,
				path,
			))
		}
		"let_declaration" => {
			let (text, line, col) =
				extract_child_text(node, source, "pattern")?;
			Some(make_ident(
				text,
				line,
				col,
				IdentifierContext::Variable,
				lang_name,
				path,
			))
		}
		"struct_item" | "enum_item" | "type_item" => {
			let (text, line, col) =
				extract_child_text(node, source, "name")?;
			Some(make_ident(
				text,
				line,
				col,
				IdentifierContext::Type,
				lang_name,
				path,
			))
		}
		"const_item" | "static_item" => {
			let (text, line, col) =
				extract_child_text(node, source, "name")?;
			Some(make_ident(
				text,
				line,
				col,
				IdentifierContext::Constant,
				lang_name,
				path,
			))
		}
		"use_declaration" => {
			// Walk all identifiers inside the use tree
			// We extract just the leaf identifier
			extract_use_identifiers(node, source, lang_name, path)
		}
		"parameter" => {
			let (text, line, col) =
				extract_child_text(node, source, "pattern")?;
			Some(make_ident(
				text,
				line,
				col,
				IdentifierContext::Parameter,
				lang_name,
				path,
			))
		}
		"field_declaration" => {
			let (text, line, col) =
				extract_child_text(node, source, "name")?;
			Some(make_ident(
				text,
				line,
				col,
				IdentifierContext::Field,
				lang_name,
				path,
			))
		}
		_ => None,
	}
}

/// Extract the last identifier from a use_declaration
fn extract_use_identifiers(
	node: &tree_sitter::Node,
	source: &[u8],
	lang_name: &str,
	path: &Path,
) -> Option<ContextualIdentifier> {
	// Find the argument (the use path/tree)
	let arg = node.child_by_field_name("argument")?;
	// For simple paths like `use foo::bar`, get the last identifier
	let text = arg.utf8_text(source).ok()?;
	// Get the rightmost segment after ::
	let last_segment = text.rsplit("::").next()?;
	if last_segment.len() <= 1
		|| last_segment.contains('{')
		|| last_segment.contains('*')
	{
		return None;
	}
	let pos = arg.end_position();
	Some(make_ident(
		last_segment,
		pos.row + 1,
		pos.column + 1,
		IdentifierContext::Import,
		lang_name,
		path,
	))
}

// ── Python classification ────────────────────────────────────────────

fn classify_python(
	node: &tree_sitter::Node,
	kind: &str,
	source: &[u8],
	lang_name: &str,
	path: &Path,
) -> Option<ContextualIdentifier> {
	match kind {
		"function_definition" => {
			let (text, line, col) =
				extract_child_text(node, source, "name")?;
			Some(make_ident(
				text,
				line,
				col,
				IdentifierContext::Function,
				lang_name,
				path,
			))
		}
		"class_definition" => {
			let (text, line, col) =
				extract_child_text(node, source, "name")?;
			Some(make_ident(
				text,
				line,
				col,
				IdentifierContext::Type,
				lang_name,
				path,
			))
		}
		"assignment" => {
			// First child identifier is the variable name
			let child = node.child(0)?;
			if child.kind() == "identifier" {
				let text = child.utf8_text(source).ok()?;
				if text.len() <= 1 {
					return None;
				}
				let pos = child.start_position();
				Some(make_ident(
					text,
					pos.row + 1,
					pos.column + 1,
					IdentifierContext::Variable,
					lang_name,
					path,
				))
			} else {
				None
			}
		}
		"import_statement" | "import_from_statement" => {
			let (text, line, col) =
				extract_child_of_kind(node, source, "dotted_name")
					.or_else(|| {
						extract_child_of_kind(
							node, source, "identifier",
						)
					})?;
			Some(make_ident(
				text,
				line,
				col,
				IdentifierContext::Import,
				lang_name,
				path,
			))
		}
		"typed_parameter" | "typed_default_parameter" => {
			let child = node.child(0)?;
			if child.kind() == "identifier" {
				let text = child.utf8_text(source).ok()?;
				if text.len() <= 1 {
					return None;
				}
				let pos = child.start_position();
				Some(make_ident(
					text,
					pos.row + 1,
					pos.column + 1,
					IdentifierContext::Parameter,
					lang_name,
					path,
				))
			} else {
				None
			}
		}
		_ => None,
	}
}

// ── JavaScript/TypeScript classification ─────────────────────────────

fn classify_js(
	node: &tree_sitter::Node,
	kind: &str,
	source: &[u8],
	lang_name: &str,
	path: &Path,
) -> Option<ContextualIdentifier> {
	match kind {
		"function_declaration" | "method_definition" => {
			let (text, line, col) =
				extract_child_text(node, source, "name")?;
			Some(make_ident(
				text,
				line,
				col,
				IdentifierContext::Function,
				lang_name,
				path,
			))
		}
		"variable_declarator" => {
			let (text, line, col) =
				extract_child_text(node, source, "name")?;
			Some(make_ident(
				text,
				line,
				col,
				IdentifierContext::Variable,
				lang_name,
				path,
			))
		}
		"class_declaration" => {
			let (text, line, col) =
				extract_child_text(node, source, "name")?;
			Some(make_ident(
				text,
				line,
				col,
				IdentifierContext::Type,
				lang_name,
				path,
			))
		}
		"import_specifier" => {
			let (text, line, col) =
				extract_child_text(node, source, "name")?;
			Some(make_ident(
				text,
				line,
				col,
				IdentifierContext::Import,
				lang_name,
				path,
			))
		}
		"required_parameter" | "optional_parameter" => {
			let (text, line, col) =
				extract_child_text(node, source, "pattern")?;
			Some(make_ident(
				text,
				line,
				col,
				IdentifierContext::Parameter,
				lang_name,
				path,
			))
		}
		"formal_parameters" => {
			// For JS (no typed parameters), extract identifiers directly
			extract_child_of_kind(node, source, "identifier").map(
				|(text, line, col)| {
					make_ident(
						text,
						line,
						col,
						IdentifierContext::Parameter,
						lang_name,
						path,
					)
				},
			)
		}
		_ => None,
	}
}

// ── Go classification ────────────────────────────────────────────────

fn classify_go(
	node: &tree_sitter::Node,
	kind: &str,
	source: &[u8],
	lang_name: &str,
	path: &Path,
) -> Option<ContextualIdentifier> {
	match kind {
		"function_declaration" | "method_declaration" => {
			let (text, line, col) =
				extract_child_text(node, source, "name")?;
			Some(make_ident(
				text,
				line,
				col,
				IdentifierContext::Function,
				lang_name,
				path,
			))
		}
		"var_spec" | "short_var_declaration" => {
			let (text, line, col) =
				extract_child_of_kind(node, source, "identifier")?;
			Some(make_ident(
				text,
				line,
				col,
				IdentifierContext::Variable,
				lang_name,
				path,
			))
		}
		"type_spec" => {
			let (text, line, col) =
				extract_child_text(node, source, "name")?;
			Some(make_ident(
				text,
				line,
				col,
				IdentifierContext::Type,
				lang_name,
				path,
			))
		}
		"const_spec" => {
			let (text, line, col) =
				extract_child_text(node, source, "name")
					.or_else(|| {
						extract_child_of_kind(
							node, source, "identifier",
						)
					})?;
			Some(make_ident(
				text,
				line,
				col,
				IdentifierContext::Constant,
				lang_name,
				path,
			))
		}
		"import_spec" => {
			let (text, _line, _col) =
				extract_child_text(node, source, "path")?;
			// Strip quotes from import path
			let text = text.trim_matches('"');
			// Get the last path segment
			let last = text.rsplit('/').next()?;
			if last.len() <= 1 {
				return None;
			}
			let pos = node.start_position();
			Some(make_ident(
				last,
				pos.row + 1,
				pos.column + 1,
				IdentifierContext::Import,
				lang_name,
				path,
			))
		}
		"parameter_declaration" => {
			let (text, line, col) =
				extract_child_of_kind(node, source, "identifier")?;
			Some(make_ident(
				text,
				line,
				col,
				IdentifierContext::Parameter,
				lang_name,
				path,
			))
		}
		_ => None,
	}
}

// ── C/C++ classification ─────────────────────────────────────────────

fn classify_c_cpp(
	node: &tree_sitter::Node,
	kind: &str,
	source: &[u8],
	lang_name: &str,
	path: &Path,
) -> Option<ContextualIdentifier> {
	match kind {
		"function_definition" => {
			// The function name is inside the declarator
			let declarator =
				node.child_by_field_name("declarator")?;
			extract_declarator_name(&declarator, source).map(
				|(text, line, col)| {
					make_ident(
						text,
						line,
						col,
						IdentifierContext::Function,
						lang_name,
						path,
					)
				},
			)
		}
		"function_declarator" => {
			// Only match if this is not inside a function_definition
			// (avoid double-counting)
			if let Some(parent) = node.parent()
				&& parent.kind() == "function_definition"
			{
				return None;
			}
			let (text, line, col) =
				extract_child_of_kind(node, source, "identifier")?;
			Some(make_ident(
				text,
				line,
				col,
				IdentifierContext::Function,
				lang_name,
				path,
			))
		}
		"declaration" => {
			// Could be a variable or function declaration
			let declarator =
				node.child_by_field_name("declarator")?;
			if declarator.kind() == "function_declarator" {
				return None; // handled by function_declarator case
			}
			let (text, line, col) =
				extract_declarator_name(&declarator, source)?;
			Some(make_ident(
				text,
				line,
				col,
				IdentifierContext::Variable,
				lang_name,
				path,
			))
		}
		"struct_specifier" | "enum_specifier" | "type_definition" => {
			let (text, line, col) =
				extract_child_text(node, source, "name")
					.or_else(|| {
						extract_child_of_kind(
							node, source, "type_identifier",
						)
					})?;
			Some(make_ident(
				text,
				line,
				col,
				IdentifierContext::Type,
				lang_name,
				path,
			))
		}
		"preproc_include" => {
			let (text, _line, _col) =
				extract_child_text(node, source, "path")?;
			// Strip quotes/brackets
			let text = text
				.trim_matches('"')
				.trim_matches('<')
				.trim_matches('>');
			if text.len() <= 1 {
				return None;
			}
			let pos = node.start_position();
			Some(make_ident(
				text,
				pos.row + 1,
				pos.column + 1,
				IdentifierContext::Import,
				lang_name,
				path,
			))
		}
		"parameter_declaration" => {
			let (text, line, col) =
				extract_child_text(node, source, "declarator")
					.or_else(|| {
						extract_child_of_kind(
							node, source, "identifier",
						)
					})?;
			Some(make_ident(
				text,
				line,
				col,
				IdentifierContext::Parameter,
				lang_name,
				path,
			))
		}
		"field_declaration" => {
			let (text, line, col) =
				extract_child_text(node, source, "declarator")
					.or_else(|| {
						extract_child_of_kind(
							node, source, "field_identifier",
						)
					})?;
			Some(make_ident(
				text,
				line,
				col,
				IdentifierContext::Field,
				lang_name,
				path,
			))
		}
		_ => None,
	}
}

/// Extract the identifier name from a C/C++ declarator (may be nested)
fn extract_declarator_name<'a>(
	node: &tree_sitter::Node<'a>,
	source: &'a [u8],
) -> Option<(&'a str, usize, usize)> {
	match node.kind() {
		"identifier" => {
			let text = node.utf8_text(source).ok()?;
			if text.len() <= 1 {
				return None;
			}
			let pos = node.start_position();
			Some((text, pos.row + 1, pos.column + 1))
		}
		"function_declarator" | "pointer_declarator"
		| "array_declarator" | "parenthesized_declarator" => {
			// The actual name is inside a nested declarator
			let inner = node.child_by_field_name("declarator")?;
			extract_declarator_name(&inner, source)
		}
		_ => {
			// Try to find an identifier child
			let count = node.child_count() as u32;
			for i in 0..count {
				if let Some(child) = node.child(i)
					&& child.kind() == "identifier"
				{
					let text = child.utf8_text(source).ok()?;
					if text.len() <= 1 {
						return None;
					}
					let pos = child.start_position();
					return Some((
						text,
						pos.row + 1,
						pos.column + 1,
					));
				}
			}
			None
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::io::Write;

	#[test]
	fn test_detect_language() {
		assert!(matches!(
			detect_language(Path::new("foo.rs")),
			Some(Language::Rust)
		));
		assert!(matches!(
			detect_language(Path::new("bar.py")),
			Some(Language::Python)
		));
		assert!(matches!(
			detect_language(Path::new("baz.js")),
			Some(Language::JavaScript)
		));
		assert!(matches!(
			detect_language(Path::new("app.ts")),
			Some(Language::TypeScript)
		));
		assert!(matches!(
			detect_language(Path::new("comp.tsx")),
			Some(Language::Tsx)
		));
		assert!(matches!(
			detect_language(Path::new("main.go")),
			Some(Language::Go)
		));
		assert!(matches!(
			detect_language(Path::new("lib.c")),
			Some(Language::C)
		));
		assert!(matches!(
			detect_language(Path::new("lib.h")),
			Some(Language::C)
		));
		assert!(matches!(
			detect_language(Path::new("lib.cpp")),
			Some(Language::Cpp)
		));
		assert!(detect_language(Path::new("readme.md")).is_none());
	}

	#[test]
	fn test_extract_rust_functions() {
		let dir =
			std::env::temp_dir().join("tagpath_test_ts_rust_fn");
		let _ = std::fs::create_dir_all(&dir);
		let file = dir.join("sample.rs");
		{
			let mut f = std::fs::File::create(&file).unwrap();
			writeln!(
				f,
				"fn create_user(person_name: &str) -> User {{}}"
			)
			.unwrap();
			writeln!(f, "fn delete_post(post_id: u64) {{}}").unwrap();
		}
		let results = extract_with_context(&file);
		let fns: Vec<&str> = results
			.iter()
			.filter(|r| r.context == IdentifierContext::Function)
			.map(|r| r.identifier.as_str())
			.collect();
		assert!(fns.contains(&"create_user"));
		assert!(fns.contains(&"delete_post"));
		let params: Vec<&str> = results
			.iter()
			.filter(|r| r.context == IdentifierContext::Parameter)
			.map(|r| r.identifier.as_str())
			.collect();
		assert!(params.contains(&"person_name"));
		assert!(params.contains(&"post_id"));
		let _ = std::fs::remove_dir_all(&dir);
	}

	#[test]
	fn test_extract_rust_types() {
		let dir =
			std::env::temp_dir().join("tagpath_test_ts_rust_type");
		let _ = std::fs::create_dir_all(&dir);
		let file = dir.join("types.rs");
		{
			let mut f = std::fs::File::create(&file).unwrap();
			writeln!(f, "struct UserProfile {{}}").unwrap();
			writeln!(
				f,
				"enum ConnectionState {{ Active, Idle }}"
			)
			.unwrap();
			writeln!(f, "const MAX_RETRIES: u32 = 3;").unwrap();
		}
		let results = extract_with_context(&file);
		let types: Vec<&str> = results
			.iter()
			.filter(|r| r.context == IdentifierContext::Type)
			.map(|r| r.identifier.as_str())
			.collect();
		assert!(types.contains(&"UserProfile"));
		assert!(types.contains(&"ConnectionState"));
		let consts: Vec<&str> = results
			.iter()
			.filter(|r| r.context == IdentifierContext::Constant)
			.map(|r| r.identifier.as_str())
			.collect();
		assert!(consts.contains(&"MAX_RETRIES"));
		let _ = std::fs::remove_dir_all(&dir);
	}

	#[test]
	fn test_extract_python() {
		let dir = std::env::temp_dir().join("tagpath_test_ts_python");
		let _ = std::fs::create_dir_all(&dir);
		let file = dir.join("sample.py");
		{
			let mut f = std::fs::File::create(&file).unwrap();
			writeln!(f, "class UserService:").unwrap();
			writeln!(f, "    def get_user(self, user_id):").unwrap();
			writeln!(f, "        pass").unwrap();
		}
		let results = extract_with_context(&file);
		let types: Vec<&str> = results
			.iter()
			.filter(|r| r.context == IdentifierContext::Type)
			.map(|r| r.identifier.as_str())
			.collect();
		assert!(types.contains(&"UserService"));
		let fns: Vec<&str> = results
			.iter()
			.filter(|r| r.context == IdentifierContext::Function)
			.map(|r| r.identifier.as_str())
			.collect();
		assert!(fns.contains(&"get_user"));
		let _ = std::fs::remove_dir_all(&dir);
	}

	#[test]
	fn test_extract_javascript() {
		let dir = std::env::temp_dir().join("tagpath_test_ts_js");
		let _ = std::fs::create_dir_all(&dir);
		let file = dir.join("sample.js");
		{
			let mut f = std::fs::File::create(&file).unwrap();
			writeln!(
				f,
				"function createUser(userName) {{ return {{}}; }}"
			)
			.unwrap();
			writeln!(f, "class PostManager {{}}").unwrap();
		}
		let results = extract_with_context(&file);
		let fns: Vec<&str> = results
			.iter()
			.filter(|r| r.context == IdentifierContext::Function)
			.map(|r| r.identifier.as_str())
			.collect();
		assert!(fns.contains(&"createUser"));
		let types: Vec<&str> = results
			.iter()
			.filter(|r| r.context == IdentifierContext::Type)
			.map(|r| r.identifier.as_str())
			.collect();
		assert!(types.contains(&"PostManager"));
		let _ = std::fs::remove_dir_all(&dir);
	}

	#[test]
	fn test_unsupported_extension_returns_empty() {
		let dir =
			std::env::temp_dir().join("tagpath_test_ts_unsupported");
		let _ = std::fs::create_dir_all(&dir);
		let file = dir.join("readme.md");
		{
			let mut f = std::fs::File::create(&file).unwrap();
			writeln!(f, "# Hello World").unwrap();
		}
		let results = extract_with_context(&file);
		assert!(results.is_empty());
		let _ = std::fs::remove_dir_all(&dir);
	}

	#[test]
	fn test_is_supported() {
		assert!(is_supported(Path::new("foo.rs")));
		assert!(is_supported(Path::new("bar.py")));
		assert!(is_supported(Path::new("baz.js")));
		assert!(is_supported(Path::new("app.ts")));
		assert!(is_supported(Path::new("comp.tsx")));
		assert!(is_supported(Path::new("main.go")));
		assert!(is_supported(Path::new("lib.c")));
		assert!(is_supported(Path::new("lib.cpp")));
		assert!(!is_supported(Path::new("readme.md")));
		assert!(!is_supported(Path::new("data.json")));
	}
}
