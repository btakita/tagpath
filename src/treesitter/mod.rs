use serde::Serialize;
use std::path::Path;

#[cfg(feature = "treesitter")]
use tree_sitter::Parser;

/// The AST context in which an identifier appears
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
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

// ── Tree-sitter internals (only when treesitter feature is enabled) ──

/// Supported languages for tree-sitter extraction
#[cfg(feature = "treesitter")]
#[derive(Debug, Clone, Copy)]
enum Language {
	#[cfg(feature = "lang-rust")]
	Rust,
	#[cfg(feature = "lang-python")]
	Python,
	#[cfg(feature = "lang-javascript")]
	JavaScript,
	#[cfg(feature = "lang-typescript")]
	TypeScript,
	#[cfg(feature = "lang-typescript")]
	Tsx,
	#[cfg(feature = "lang-go")]
	Go,
	#[cfg(feature = "lang-c")]
	C,
	#[cfg(feature = "lang-cpp")]
	Cpp,
	#[cfg(feature = "lang-java")]
	Java,
	#[cfg(feature = "lang-ruby")]
	Ruby,
	#[cfg(feature = "lang-php")]
	Php,
	#[cfg(feature = "lang-csharp")]
	CSharp,
	#[cfg(feature = "lang-swift")]
	Swift,
	#[cfg(feature = "lang-kotlin")]
	Kotlin,
}

/// Detect language from file extension
#[cfg(feature = "treesitter")]
fn detect_language(path: &Path) -> Option<Language> {
	let ext = path.extension()?.to_str()?;
	match ext {
		#[cfg(feature = "lang-rust")]
		"rs" => Some(Language::Rust),
		#[cfg(feature = "lang-python")]
		"py" => Some(Language::Python),
		#[cfg(feature = "lang-javascript")]
		"js" => Some(Language::JavaScript),
		#[cfg(feature = "lang-typescript")]
		"ts" => Some(Language::TypeScript),
		#[cfg(feature = "lang-typescript")]
		"tsx" => Some(Language::Tsx),
		#[cfg(feature = "lang-go")]
		"go" => Some(Language::Go),
		#[cfg(feature = "lang-c")]
		"c" | "h" => Some(Language::C),
		#[cfg(feature = "lang-cpp")]
		"cpp" | "hpp" | "cc" | "cxx" => Some(Language::Cpp),
		#[cfg(feature = "lang-java")]
		"java" => Some(Language::Java),
		#[cfg(feature = "lang-ruby")]
		"rb" => Some(Language::Ruby),
		#[cfg(feature = "lang-php")]
		"php" => Some(Language::Php),
		#[cfg(feature = "lang-csharp")]
		"cs" => Some(Language::CSharp),
		#[cfg(feature = "lang-swift")]
		"swift" => Some(Language::Swift),
		#[cfg(feature = "lang-kotlin")]
		"kt" | "kts" => Some(Language::Kotlin),
		_ => None,
	}
}

/// Get the tree-sitter language for a detected language
#[cfg(feature = "treesitter")]
fn get_ts_language(lang: Language) -> tree_sitter::Language {
	match lang {
		#[cfg(feature = "lang-rust")]
		Language::Rust => tree_sitter_rust::LANGUAGE.into(),
		#[cfg(feature = "lang-python")]
		Language::Python => tree_sitter_python::LANGUAGE.into(),
		#[cfg(feature = "lang-javascript")]
		Language::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
		#[cfg(feature = "lang-typescript")]
		Language::TypeScript => {
			tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
		}
		#[cfg(feature = "lang-typescript")]
		Language::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
		#[cfg(feature = "lang-go")]
		Language::Go => tree_sitter_go::LANGUAGE.into(),
		#[cfg(feature = "lang-c")]
		Language::C => tree_sitter_c::LANGUAGE.into(),
		#[cfg(feature = "lang-cpp")]
		Language::Cpp => tree_sitter_cpp::LANGUAGE.into(),
		#[cfg(feature = "lang-java")]
		Language::Java => tree_sitter_java::LANGUAGE.into(),
		#[cfg(feature = "lang-ruby")]
		Language::Ruby => tree_sitter_ruby::LANGUAGE.into(),
		#[cfg(feature = "lang-php")]
		Language::Php => tree_sitter_php::LANGUAGE_PHP.into(),
		#[cfg(feature = "lang-csharp")]
		Language::CSharp => tree_sitter_c_sharp::LANGUAGE.into(),
		#[cfg(feature = "lang-swift")]
		Language::Swift => tree_sitter_swift::LANGUAGE.into(),
		#[cfg(feature = "lang-kotlin")]
		Language::Kotlin => tree_sitter_kotlin_ng::LANGUAGE.into(),
	}
}

/// Get the language name string
#[cfg(feature = "treesitter")]
fn language_name(lang: Language) -> &'static str {
	match lang {
		#[cfg(feature = "lang-rust")]
		Language::Rust => "rust",
		#[cfg(feature = "lang-python")]
		Language::Python => "python",
		#[cfg(feature = "lang-javascript")]
		Language::JavaScript => "javascript",
		#[cfg(feature = "lang-typescript")]
		Language::TypeScript => "typescript",
		#[cfg(feature = "lang-typescript")]
		Language::Tsx => "tsx",
		#[cfg(feature = "lang-go")]
		Language::Go => "go",
		#[cfg(feature = "lang-c")]
		Language::C => "c",
		#[cfg(feature = "lang-cpp")]
		Language::Cpp => "cpp",
		#[cfg(feature = "lang-java")]
		Language::Java => "java",
		#[cfg(feature = "lang-ruby")]
		Language::Ruby => "ruby",
		#[cfg(feature = "lang-php")]
		Language::Php => "php",
		#[cfg(feature = "lang-csharp")]
		Language::CSharp => "csharp",
		#[cfg(feature = "lang-swift")]
		Language::Swift => "swift",
		#[cfg(feature = "lang-kotlin")]
		Language::Kotlin => "kotlin",
	}
}

// ── Public API ───────────────────────────────────────────────────────

/// Check if a file extension is supported by tree-sitter extraction
#[cfg(feature = "treesitter")]
pub fn is_supported(path: &Path) -> bool {
	detect_language(path).is_some()
}

#[cfg(not(feature = "treesitter"))]
pub fn is_supported(_path: &Path) -> bool {
	false
}

/// Extract identifiers with AST context from a source file
#[cfg(feature = "treesitter")]
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

#[cfg(not(feature = "treesitter"))]
pub fn extract_with_context(_path: &Path) -> Vec<ContextualIdentifier> {
	vec![]
}

// ── Node classification dispatch ─────────────────────────────────────

/// Classify a node and extract its identifier if it's a definition
#[cfg(feature = "treesitter")]
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
		#[cfg(feature = "lang-rust")]
		Language::Rust => classify_rust(node, kind, source, lang_name, path),
		#[cfg(feature = "lang-python")]
		Language::Python => {
			classify_python(node, kind, source, lang_name, path)
		}
		#[cfg(feature = "lang-javascript")]
		Language::JavaScript => {
			classify_js(node, kind, source, lang_name, path)
		}
		#[cfg(feature = "lang-typescript")]
		Language::TypeScript => {
			classify_js(node, kind, source, lang_name, path)
		}
		#[cfg(feature = "lang-typescript")]
		Language::Tsx => {
			classify_js(node, kind, source, lang_name, path)
		}
		#[cfg(feature = "lang-go")]
		Language::Go => classify_go(node, kind, source, lang_name, path),
		#[cfg(feature = "lang-c")]
		Language::C => {
			classify_c_cpp(node, kind, source, lang_name, path)
		}
		#[cfg(feature = "lang-cpp")]
		Language::Cpp => {
			classify_c_cpp(node, kind, source, lang_name, path)
		}
		#[cfg(feature = "lang-java")]
		Language::Java => {
			classify_java(node, kind, source, lang_name, path)
		}
		#[cfg(feature = "lang-ruby")]
		Language::Ruby => {
			classify_ruby(node, kind, source, lang_name, path)
		}
		#[cfg(feature = "lang-php")]
		Language::Php => {
			classify_php(node, kind, source, lang_name, path)
		}
		#[cfg(feature = "lang-csharp")]
		Language::CSharp => {
			classify_csharp(node, kind, source, lang_name, path)
		}
		#[cfg(feature = "lang-swift")]
		Language::Swift => {
			classify_swift(node, kind, source, lang_name, path)
		}
		#[cfg(feature = "lang-kotlin")]
		Language::Kotlin => {
			classify_kotlin(node, kind, source, lang_name, path)
		}
	}
}

// ── Common helpers ───────────────────────────────────────────────────

/// Extract identifier text from a child node by field name or node kind
#[cfg(feature = "treesitter")]
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
#[cfg(feature = "treesitter")]
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

#[cfg(feature = "treesitter")]
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

#[cfg(feature = "lang-rust")]
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
#[cfg(feature = "lang-rust")]
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

#[cfg(feature = "lang-python")]
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

#[cfg(any(feature = "lang-javascript", feature = "lang-typescript"))]
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

#[cfg(feature = "lang-go")]
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

#[cfg(any(feature = "lang-c", feature = "lang-cpp"))]
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
#[cfg(any(feature = "lang-c", feature = "lang-cpp"))]
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

// ── Java classification ───────────────────────────────────────────────

#[cfg(feature = "lang-java")]
fn classify_java(
	node: &tree_sitter::Node,
	kind: &str,
	source: &[u8],
	lang_name: &str,
	path: &Path,
) -> Option<ContextualIdentifier> {
	match kind {
		"method_declaration" | "constructor_declaration" => {
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
		"local_variable_declaration" => {
			// The variable name is inside a variable_declarator child
			extract_child_of_kind(node, source, "variable_declarator")
				.and_then(|_| {
					// Walk children to find variable_declarator, then its name
					let count = node.child_count() as u32;
					for i in 0..count {
						if let Some(child) = node.child(i)
							&& child.kind() == "variable_declarator"
							&& let Some((text, line, col)) =
								extract_child_text(
									&child, source, "name",
								)
						{
							return Some(make_ident(
								text,
								line,
								col,
								IdentifierContext::Variable,
								lang_name,
								path,
							));
						}
					}
					None
				})
		}
		"variable_declarator" => {
			// Only match if parent is NOT local_variable_declaration
			// (to avoid double-counting with the above)
			if let Some(parent) = node.parent()
				&& parent.kind() == "local_variable_declaration"
			{
				return None;
			}
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
		"class_declaration" | "interface_declaration"
		| "enum_declaration" | "record_declaration"
		| "annotation_type_declaration" => {
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
		"constant_declaration" => {
			// Constants have variable_declarator children
			let count = node.child_count() as u32;
			for i in 0..count {
				if let Some(child) = node.child(i)
					&& child.kind() == "variable_declarator"
					&& let Some((text, line, col)) =
						extract_child_text(&child, source, "name")
				{
					return Some(make_ident(
						text,
						line,
						col,
						IdentifierContext::Constant,
						lang_name,
						path,
					));
				}
			}
			None
		}
		"import_declaration" => {
			// Extract the last segment of the import path
			let text = node.utf8_text(source).ok()?;
			// e.g. "import java.util.List;"
			let stripped = text
				.trim_start_matches("import ")
				.trim_start_matches("static ")
				.trim_end_matches(';')
				.trim();
			let last = stripped.rsplit('.').next()?;
			if last.len() <= 1 || last == "*" {
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
		"formal_parameter" | "spread_parameter" => {
			let (text, line, col) =
				extract_child_text(node, source, "name")?;
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
			// Field has variable_declarator children
			let count = node.child_count() as u32;
			for i in 0..count {
				if let Some(child) = node.child(i)
					&& child.kind() == "variable_declarator"
					&& let Some((text, line, col)) =
						extract_child_text(&child, source, "name")
				{
					return Some(make_ident(
						text,
						line,
						col,
						IdentifierContext::Field,
						lang_name,
						path,
					));
				}
			}
			None
		}
		_ => None,
	}
}

// ── Ruby classification ──────────────────────────────────────────────

#[cfg(feature = "lang-ruby")]
fn classify_ruby(
	node: &tree_sitter::Node,
	kind: &str,
	source: &[u8],
	lang_name: &str,
	path: &Path,
) -> Option<ContextualIdentifier> {
	match kind {
		"method" | "singleton_method" => {
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
		"assignment" => {
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
			} else if child.kind() == "constant" {
				// UPPER_CASE = value is a constant
				let text = child.utf8_text(source).ok()?;
				if text.len() <= 1 {
					return None;
				}
				// Check if ALL_CAPS (constant convention)
				let is_const = text
					.chars()
					.all(|c| c.is_uppercase() || c == '_');
				let pos = child.start_position();
				Some(make_ident(
					text,
					pos.row + 1,
					pos.column + 1,
					if is_const {
						IdentifierContext::Constant
					} else {
						IdentifierContext::Variable
					},
					lang_name,
					path,
				))
			} else if child.kind() == "instance_variable" {
				let text = child.utf8_text(source).ok()?;
				// Strip @ prefix for the identifier
				let name = text.trim_start_matches('@');
				if name.len() <= 1 {
					return None;
				}
				let pos = child.start_position();
				Some(make_ident(
					name,
					pos.row + 1,
					pos.column + 1,
					IdentifierContext::Field,
					lang_name,
					path,
				))
			} else {
				None
			}
		}
		"class" => {
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
		"module" => {
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
		"call" => {
			// Check if this is a require/require_relative call
			let method_node = node.child_by_field_name("method")?;
			let method = method_node.utf8_text(source).ok()?;
			if method == "require" || method == "require_relative" {
				// Get the argument
				let args =
					node.child_by_field_name("arguments")?;
				let arg = args.child(0)?;
				let text = arg.utf8_text(source).ok()?;
				let text = text.trim_matches('"').trim_matches('\'');
				// Get last path segment
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
			} else {
				None
			}
		}
		"method_parameters" => {
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
		"optional_parameter" | "keyword_parameter" => {
			let (text, line, col) =
				extract_child_text(node, source, "name")?;
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

// ── PHP classification ───────────────────────────────────────────────

#[cfg(feature = "lang-php")]
fn classify_php(
	node: &tree_sitter::Node,
	kind: &str,
	source: &[u8],
	lang_name: &str,
	path: &Path,
) -> Option<ContextualIdentifier> {
	match kind {
		"function_definition" | "method_declaration" => {
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
		"static_variable_declaration" => {
			extract_child_of_kind(node, source, "variable_name").map(
				|(text, line, col)| {
					let name = text.trim_start_matches('$');
					make_ident(
						name,
						line,
						col,
						IdentifierContext::Variable,
						lang_name,
						path,
					)
				},
			)
		}
		"class_declaration" | "interface_declaration"
		| "trait_declaration" | "enum_declaration" => {
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
		"const_element" => {
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
		"namespace_use_clause" => {
			// Extract the last segment of the namespace path
			let text = node.utf8_text(source).ok()?;
			let last = text.rsplit('\\').next()?;
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
		"simple_parameter" | "variadic_parameter"
		| "property_promotion_parameter" => {
			let (text, line, col) =
				extract_child_text(node, source, "name")?;
			// PHP parameters have $ prefix in the AST
			let name = text.trim_start_matches('$');
			if name.len() <= 1 {
				return None;
			}
			Some(make_ident(
				name,
				line,
				col,
				IdentifierContext::Parameter,
				lang_name,
				path,
			))
		}
		"property_element" => {
			extract_child_of_kind(node, source, "variable_name").map(
				|(text, line, col)| {
					let name = text.trim_start_matches('$');
					make_ident(
						name,
						line,
						col,
						IdentifierContext::Field,
						lang_name,
						path,
					)
				},
			)
		}
		_ => None,
	}
}

// ── C# classification ────────────────────────────────────────────────

#[cfg(feature = "lang-csharp")]
fn classify_csharp(
	node: &tree_sitter::Node,
	kind: &str,
	source: &[u8],
	lang_name: &str,
	path: &Path,
) -> Option<ContextualIdentifier> {
	match kind {
		"method_declaration" | "local_function_statement"
		| "constructor_declaration" => {
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
		"variable_declaration" => {
			// Contains variable_declarator children
			let count = node.child_count() as u32;
			for i in 0..count {
				if let Some(child) = node.child(i)
					&& child.kind() == "variable_declarator"
					&& let Some((text, line, col)) =
						extract_child_text(
							&child, source, "name",
						)
						.or_else(|| {
							extract_child_of_kind(
								&child, source, "identifier",
							)
						})
				{
					return Some(make_ident(
						text,
						line,
						col,
						IdentifierContext::Variable,
						lang_name,
						path,
					));
				}
			}
			None
		}
		"class_declaration" | "struct_declaration"
		| "interface_declaration" | "enum_declaration"
		| "record_declaration" | "delegate_declaration" => {
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
		"constant_pattern" => None, // Not a definition
		"using_directive" => {
			// Extract the last segment of the namespace
			let text = node.utf8_text(source).ok()?;
			let stripped = text
				.trim_start_matches("using ")
				.trim_start_matches("static ")
				.trim_end_matches(';')
				.trim();
			let last = stripped.rsplit('.').next()?;
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
		"parameter" => {
			let (text, line, col) =
				extract_child_text(node, source, "name")?;
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
			// Contains variable_declaration which has variable_declarator
			let count = node.child_count() as u32;
			for i in 0..count {
				if let Some(child) = node.child(i)
					&& child.kind() == "variable_declaration"
				{
					let inner_count = child.child_count() as u32;
					for j in 0..inner_count {
						if let Some(decl) = child.child(j)
							&& decl.kind() == "variable_declarator"
							&& let Some((text, line, col)) =
								extract_child_text(
									&decl, source, "name",
								)
								.or_else(|| {
									extract_child_of_kind(
										&decl,
										source,
										"identifier",
									)
								})
						{
							return Some(make_ident(
								text,
								line,
								col,
								IdentifierContext::Field,
								lang_name,
								path,
							));
						}
					}
				}
			}
			None
		}
		"property_declaration" => {
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

// ── Swift classification ─────────────────────────────────────────────

#[cfg(feature = "lang-swift")]
fn classify_swift(
	node: &tree_sitter::Node,
	kind: &str,
	source: &[u8],
	lang_name: &str,
	path: &Path,
) -> Option<ContextualIdentifier> {
	match kind {
		"function_declaration" | "init_declaration" => {
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
		"property_declaration" => {
			let (text, line, col) =
				extract_child_text(node, source, "name")?;
			// Check if preceded by "let" — treat as constant
			let full_text = node.utf8_text(source).ok()?;
			let context =
				if full_text.trim_start().starts_with("let ") {
					IdentifierContext::Constant
				} else {
					IdentifierContext::Variable
				};
			Some(make_ident(
				text, line, col, context, lang_name, path,
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
		"protocol_declaration" => {
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
		"import_declaration" => {
			// "import Foundation" -> extract "Foundation"
			let text = node.utf8_text(source).ok()?;
			let stripped =
				text.trim_start_matches("import ").trim();
			let last = stripped.rsplit('.').next()?;
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
		"parameter" => {
			let (text, line, col) =
				extract_child_text(node, source, "name")?;
			Some(make_ident(
				text,
				line,
				col,
				IdentifierContext::Parameter,
				lang_name,
				path,
			))
		}
		"enum_entry" => {
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
		_ => None,
	}
}

// ── Kotlin classification ────────────────────────────────────────────

#[cfg(feature = "lang-kotlin")]
fn classify_kotlin(
	node: &tree_sitter::Node,
	kind: &str,
	source: &[u8],
	lang_name: &str,
	path: &Path,
) -> Option<ContextualIdentifier> {
	match kind {
		"function_declaration" => {
			extract_child_of_kind(node, source, "identifier").map(
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
		"property_declaration" => {
			extract_child_of_kind(
				node,
				source,
				"variable_declaration",
			)
			.and_then(|_| {
				// Walk children to find variable_declaration,
				// then get its identifier
				let count = node.child_count() as u32;
				for i in 0..count {
					if let Some(child) = node.child(i)
						&& child.kind() == "variable_declaration"
						&& let Some((text, line, col)) =
							extract_child_of_kind(
								&child, source, "identifier",
							)
					{
						// Check if preceded by "val" — treat as constant
						let full_text =
							node.utf8_text(source).ok()?;
						let trimmed = full_text.trim_start();
						let context = if trimmed
							.starts_with("val ")
							&& text.chars().all(|c| {
								c.is_uppercase() || c == '_'
							}) {
							IdentifierContext::Constant
						} else {
							IdentifierContext::Variable
						};
						return Some(make_ident(
							text, line, col, context,
							lang_name, path,
						));
					}
				}
				None
			})
		}
		"class_declaration" | "object_declaration" => {
			extract_child_of_kind(node, source, "identifier").map(
				|(text, line, col)| {
					make_ident(
						text,
						line,
						col,
						IdentifierContext::Type,
						lang_name,
						path,
					)
				},
			)
		}
		"type_alias" => {
			extract_child_of_kind(node, source, "identifier").map(
				|(text, line, col)| {
					make_ident(
						text,
						line,
						col,
						IdentifierContext::Type,
						lang_name,
						path,
					)
				},
			)
		}
		"import" => {
			// Extract the last segment of the import
			let text = node.utf8_text(source).ok()?;
			let stripped =
				text.trim_start_matches("import ").trim();
			let last = stripped.rsplit('.').next()?;
			if last.len() <= 1 || last == "*" {
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
		"parameter" | "class_parameter" => {
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
		"enum_entry" => {
			extract_child_of_kind(node, source, "identifier").map(
				|(text, line, col)| {
					make_ident(
						text,
						line,
						col,
						IdentifierContext::Constant,
						lang_name,
						path,
					)
				},
			)
		}
		_ => None,
	}
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(all(test, feature = "treesitter"))]
mod tests {
	use super::*;
	use std::io::Write;

	#[test]
	fn test_detect_language() {
		#[cfg(feature = "lang-rust")]
		assert!(matches!(
			detect_language(Path::new("foo.rs")),
			Some(Language::Rust)
		));
		#[cfg(feature = "lang-python")]
		assert!(matches!(
			detect_language(Path::new("bar.py")),
			Some(Language::Python)
		));
		#[cfg(feature = "lang-javascript")]
		assert!(matches!(
			detect_language(Path::new("baz.js")),
			Some(Language::JavaScript)
		));
		#[cfg(feature = "lang-typescript")]
		assert!(matches!(
			detect_language(Path::new("app.ts")),
			Some(Language::TypeScript)
		));
		#[cfg(feature = "lang-typescript")]
		assert!(matches!(
			detect_language(Path::new("comp.tsx")),
			Some(Language::Tsx)
		));
		#[cfg(feature = "lang-go")]
		assert!(matches!(
			detect_language(Path::new("main.go")),
			Some(Language::Go)
		));
		#[cfg(feature = "lang-c")]
		assert!(matches!(
			detect_language(Path::new("lib.c")),
			Some(Language::C)
		));
		#[cfg(feature = "lang-c")]
		assert!(matches!(
			detect_language(Path::new("lib.h")),
			Some(Language::C)
		));
		#[cfg(feature = "lang-cpp")]
		assert!(matches!(
			detect_language(Path::new("lib.cpp")),
			Some(Language::Cpp)
		));
		#[cfg(feature = "lang-java")]
		assert!(matches!(
			detect_language(Path::new("Main.java")),
			Some(Language::Java)
		));
		#[cfg(feature = "lang-ruby")]
		assert!(matches!(
			detect_language(Path::new("app.rb")),
			Some(Language::Ruby)
		));
		#[cfg(feature = "lang-php")]
		assert!(matches!(
			detect_language(Path::new("index.php")),
			Some(Language::Php)
		));
		#[cfg(feature = "lang-csharp")]
		assert!(matches!(
			detect_language(Path::new("Program.cs")),
			Some(Language::CSharp)
		));
		#[cfg(feature = "lang-swift")]
		assert!(matches!(
			detect_language(Path::new("main.swift")),
			Some(Language::Swift)
		));
		#[cfg(feature = "lang-kotlin")]
		assert!(matches!(
			detect_language(Path::new("Main.kt")),
			Some(Language::Kotlin)
		));
		#[cfg(feature = "lang-kotlin")]
		assert!(matches!(
			detect_language(Path::new("build.kts")),
			Some(Language::Kotlin)
		));
		assert!(detect_language(Path::new("readme.md")).is_none());
	}

	#[cfg(feature = "lang-rust")]
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

	#[cfg(feature = "lang-rust")]
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

	#[cfg(feature = "lang-python")]
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

	#[cfg(feature = "lang-javascript")]
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
		#[cfg(feature = "lang-rust")]
		assert!(is_supported(Path::new("foo.rs")));
		#[cfg(feature = "lang-python")]
		assert!(is_supported(Path::new("bar.py")));
		#[cfg(feature = "lang-javascript")]
		assert!(is_supported(Path::new("baz.js")));
		#[cfg(feature = "lang-typescript")]
		assert!(is_supported(Path::new("app.ts")));
		#[cfg(feature = "lang-typescript")]
		assert!(is_supported(Path::new("comp.tsx")));
		#[cfg(feature = "lang-go")]
		assert!(is_supported(Path::new("main.go")));
		#[cfg(feature = "lang-c")]
		assert!(is_supported(Path::new("lib.c")));
		#[cfg(feature = "lang-cpp")]
		assert!(is_supported(Path::new("lib.cpp")));
		#[cfg(feature = "lang-java")]
		assert!(is_supported(Path::new("Main.java")));
		#[cfg(feature = "lang-ruby")]
		assert!(is_supported(Path::new("app.rb")));
		#[cfg(feature = "lang-php")]
		assert!(is_supported(Path::new("index.php")));
		#[cfg(feature = "lang-csharp")]
		assert!(is_supported(Path::new("Program.cs")));
		#[cfg(feature = "lang-swift")]
		assert!(is_supported(Path::new("main.swift")));
		#[cfg(feature = "lang-kotlin")]
		assert!(is_supported(Path::new("Main.kt")));
		assert!(!is_supported(Path::new("readme.md")));
		assert!(!is_supported(Path::new("data.json")));
	}

	#[cfg(feature = "lang-java")]
	#[test]
	fn test_extract_java() {
		let dir = std::env::temp_dir().join("tagpath_test_ts_java");
		let _ = std::fs::create_dir_all(&dir);
		let file = dir.join("Sample.java");
		{
			let mut f = std::fs::File::create(&file).unwrap();
			writeln!(f, "import java.util.List;").unwrap();
			writeln!(f, "public class UserService {{").unwrap();
			writeln!(f, "    private String userName;").unwrap();
			writeln!(
				f,
				"    public void getUser(String userId) {{}}"
			)
			.unwrap();
			writeln!(f, "}}").unwrap();
		}
		let results = extract_with_context(&file);
		let types: Vec<&str> = results
			.iter()
			.filter(|r| r.context == IdentifierContext::Type)
			.map(|r| r.identifier.as_str())
			.collect();
		assert!(
			types.contains(&"UserService"),
			"expected UserService in types, got: {:?}",
			types
		);
		let fns: Vec<&str> = results
			.iter()
			.filter(|r| r.context == IdentifierContext::Function)
			.map(|r| r.identifier.as_str())
			.collect();
		assert!(
			fns.contains(&"getUser"),
			"expected getUser in fns, got: {:?}",
			fns
		);
		let imports: Vec<&str> = results
			.iter()
			.filter(|r| r.context == IdentifierContext::Import)
			.map(|r| r.identifier.as_str())
			.collect();
		assert!(
			imports.contains(&"List"),
			"expected List in imports, got: {:?}",
			imports
		);
		let params: Vec<&str> = results
			.iter()
			.filter(|r| r.context == IdentifierContext::Parameter)
			.map(|r| r.identifier.as_str())
			.collect();
		assert!(
			params.contains(&"userId"),
			"expected userId in params, got: {:?}",
			params
		);
		let _ = std::fs::remove_dir_all(&dir);
	}

	#[cfg(feature = "lang-ruby")]
	#[test]
	fn test_extract_ruby() {
		let dir = std::env::temp_dir().join("tagpath_test_ts_ruby");
		let _ = std::fs::create_dir_all(&dir);
		let file = dir.join("sample.rb");
		{
			let mut f = std::fs::File::create(&file).unwrap();
			writeln!(f, "class UserService").unwrap();
			writeln!(f, "  def get_user(user_id)").unwrap();
			writeln!(f, "    @user_name = 'test'").unwrap();
			writeln!(f, "  end").unwrap();
			writeln!(f, "end").unwrap();
		}
		let results = extract_with_context(&file);
		let types: Vec<&str> = results
			.iter()
			.filter(|r| r.context == IdentifierContext::Type)
			.map(|r| r.identifier.as_str())
			.collect();
		assert!(
			types.contains(&"UserService"),
			"expected UserService in types, got: {:?}",
			types
		);
		let fns: Vec<&str> = results
			.iter()
			.filter(|r| r.context == IdentifierContext::Function)
			.map(|r| r.identifier.as_str())
			.collect();
		assert!(
			fns.contains(&"get_user"),
			"expected get_user in fns, got: {:?}",
			fns
		);
		let _ = std::fs::remove_dir_all(&dir);
	}

	#[cfg(feature = "lang-php")]
	#[test]
	fn test_extract_php() {
		let dir = std::env::temp_dir().join("tagpath_test_ts_php");
		let _ = std::fs::create_dir_all(&dir);
		let file = dir.join("sample.php");
		{
			let mut f = std::fs::File::create(&file).unwrap();
			writeln!(f, "<?php").unwrap();
			writeln!(f, "class UserService {{").unwrap();
			writeln!(
				f,
				"    public function getUser($userId) {{}}"
			)
			.unwrap();
			writeln!(f, "}}").unwrap();
		}
		let results = extract_with_context(&file);
		let types: Vec<&str> = results
			.iter()
			.filter(|r| r.context == IdentifierContext::Type)
			.map(|r| r.identifier.as_str())
			.collect();
		assert!(
			types.contains(&"UserService"),
			"expected UserService in types, got: {:?}",
			types
		);
		let fns: Vec<&str> = results
			.iter()
			.filter(|r| r.context == IdentifierContext::Function)
			.map(|r| r.identifier.as_str())
			.collect();
		assert!(
			fns.contains(&"getUser"),
			"expected getUser in fns, got: {:?}",
			fns
		);
		let _ = std::fs::remove_dir_all(&dir);
	}

	#[cfg(feature = "lang-csharp")]
	#[test]
	fn test_extract_csharp() {
		let dir =
			std::env::temp_dir().join("tagpath_test_ts_csharp");
		let _ = std::fs::create_dir_all(&dir);
		let file = dir.join("Program.cs");
		{
			let mut f = std::fs::File::create(&file).unwrap();
			writeln!(f, "using System.Collections;").unwrap();
			writeln!(f, "class UserService {{").unwrap();
			writeln!(
				f,
				"    public void GetUser(string userId) {{}}"
			)
			.unwrap();
			writeln!(f, "}}").unwrap();
		}
		let results = extract_with_context(&file);
		let types: Vec<&str> = results
			.iter()
			.filter(|r| r.context == IdentifierContext::Type)
			.map(|r| r.identifier.as_str())
			.collect();
		assert!(
			types.contains(&"UserService"),
			"expected UserService in types, got: {:?}",
			types
		);
		let fns: Vec<&str> = results
			.iter()
			.filter(|r| r.context == IdentifierContext::Function)
			.map(|r| r.identifier.as_str())
			.collect();
		assert!(
			fns.contains(&"GetUser"),
			"expected GetUser in fns, got: {:?}",
			fns
		);
		let imports: Vec<&str> = results
			.iter()
			.filter(|r| r.context == IdentifierContext::Import)
			.map(|r| r.identifier.as_str())
			.collect();
		assert!(
			imports.contains(&"Collections"),
			"expected Collections in imports, got: {:?}",
			imports
		);
		let _ = std::fs::remove_dir_all(&dir);
	}

	#[cfg(feature = "lang-swift")]
	#[test]
	fn test_extract_swift() {
		let dir =
			std::env::temp_dir().join("tagpath_test_ts_swift");
		let _ = std::fs::create_dir_all(&dir);
		let file = dir.join("sample.swift");
		{
			let mut f = std::fs::File::create(&file).unwrap();
			writeln!(f, "import Foundation").unwrap();
			writeln!(f, "class UserService {{").unwrap();
			writeln!(
				f,
				"    func getUser(userId: String) -> String {{"
			)
			.unwrap();
			writeln!(f, "        return \"\"").unwrap();
			writeln!(f, "    }}").unwrap();
			writeln!(f, "}}").unwrap();
		}
		let results = extract_with_context(&file);
		let types: Vec<&str> = results
			.iter()
			.filter(|r| r.context == IdentifierContext::Type)
			.map(|r| r.identifier.as_str())
			.collect();
		assert!(
			types.contains(&"UserService"),
			"expected UserService in types, got: {:?}",
			types
		);
		let fns: Vec<&str> = results
			.iter()
			.filter(|r| r.context == IdentifierContext::Function)
			.map(|r| r.identifier.as_str())
			.collect();
		assert!(
			fns.contains(&"getUser"),
			"expected getUser in fns, got: {:?}",
			fns
		);
		let imports: Vec<&str> = results
			.iter()
			.filter(|r| r.context == IdentifierContext::Import)
			.map(|r| r.identifier.as_str())
			.collect();
		assert!(
			imports.contains(&"Foundation"),
			"expected Foundation in imports, got: {:?}",
			imports
		);
		let _ = std::fs::remove_dir_all(&dir);
	}

	#[cfg(feature = "lang-kotlin")]
	#[test]
	fn test_extract_kotlin() {
		let dir =
			std::env::temp_dir().join("tagpath_test_ts_kotlin");
		let _ = std::fs::create_dir_all(&dir);
		let file = dir.join("Sample.kt");
		{
			let mut f = std::fs::File::create(&file).unwrap();
			writeln!(f, "import kotlin.collections.List")
				.unwrap();
			writeln!(f, "class UserService {{").unwrap();
			writeln!(
				f,
				"    fun getUser(userId: String): String {{"
			)
			.unwrap();
			writeln!(f, "        return \"\"").unwrap();
			writeln!(f, "    }}").unwrap();
			writeln!(f, "}}").unwrap();
		}
		let results = extract_with_context(&file);
		let types: Vec<&str> = results
			.iter()
			.filter(|r| r.context == IdentifierContext::Type)
			.map(|r| r.identifier.as_str())
			.collect();
		assert!(
			types.contains(&"UserService"),
			"expected UserService in types, got: {:?}",
			types
		);
		let fns: Vec<&str> = results
			.iter()
			.filter(|r| r.context == IdentifierContext::Function)
			.map(|r| r.identifier.as_str())
			.collect();
		assert!(
			fns.contains(&"getUser"),
			"expected getUser in fns, got: {:?}",
			fns
		);
		let imports: Vec<&str> = results
			.iter()
			.filter(|r| r.context == IdentifierContext::Import)
			.map(|r| r.identifier.as_str())
			.collect();
		assert!(
			imports.contains(&"List"),
			"expected List in imports, got: {:?}",
			imports
		);
		let _ = std::fs::remove_dir_all(&dir);
	}
}
