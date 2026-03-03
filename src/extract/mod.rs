use crate::parser;
use crate::treesitter::IdentifierContext;
use regex::Regex;
use serde::Serialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// An identifier extracted from a source file, with its parsed tag structure
#[derive(Debug, Serialize)]
pub struct ExtractedIdentifier {
	pub file: PathBuf,
	pub line: usize,
	pub column: usize,
	pub identifier: String,
	pub parsed: parser::ParsedName,
	/// AST context (only populated in AST mode)
	#[serde(skip_serializing_if = "Option::is_none")]
	pub context: Option<IdentifierContext>,
}

/// Source file extensions to scan
const SOURCE_EXTENSIONS: &[&str] = &[
	"rs", "py", "ts", "js", "go", "java", "rb", "c", "cpp", "h", "hpp",
	"cs", "swift", "kt", "scala", "zig", "nim", "ex", "exs", "erl", "hs",
	"ml", "clj", "r", "lua", "php", "pl", "d", "cr", "dart", "jl", "v",
	"odin", "gleam", "rkt", "scm", "lisp", "lsp", "f", "fs", "fsi", "fsx",
	"sh", "bash", "zsh", "sql", "css", "tsx",
];

/// Extensions that use kebab-case identifiers
const KEBAB_EXTENSIONS: &[&str] = &[
	"css", "lisp", "lsp", "clj", "scm", "rkt",
];

/// Directories to skip during traversal
const SKIP_DIRS: &[&str] = &[
	".git",
	"node_modules",
	"target",
	"__pycache__",
	".venv",
	"vendor",
];

/// Common language keywords to filter out
const KEYWORDS: &[&str] = &[
	"if", "else", "for", "while", "return", "fn", "func", "function",
	"def", "class", "struct", "enum", "impl", "use", "import", "from",
	"let", "const", "var", "pub", "self", "this", "true", "false", "null",
	"none", "nil",
];

/// Maximum file size to process (1 MB)
const MAX_FILE_SIZE: u64 = 1_048_576;

/// Extract identifiers from a path (file or directory) using regex
pub fn extract_from_path(path: &Path) -> Vec<ExtractedIdentifier> {
	extract_from_path_with_mode(path, false)
}

/// Extract identifiers from a path with optional AST mode
pub fn extract_from_path_with_mode(
	path: &Path,
	ast_mode: bool,
) -> Vec<ExtractedIdentifier> {
	if path.is_file() {
		return if ast_mode {
			extract_from_file_ast(path)
		} else {
			extract_from_file(path)
		};
	}
	let mut results = Vec::new();
	for entry in WalkDir::new(path)
		.into_iter()
		.filter_entry(|e| !is_skipped_dir(e))
	{
		let entry = match entry {
			Ok(e) => e,
			Err(_) => continue,
		};
		if !entry.file_type().is_file() {
			continue;
		}
		if !is_source_file(entry.path()) {
			continue;
		}
		if ast_mode {
			results
				.extend(extract_from_file_ast(entry.path()));
		} else {
			results.extend(extract_from_file(entry.path()));
		}
	}
	results
}

/// Extract identifiers from a single source file using regex
pub fn extract_from_file(path: &Path) -> Vec<ExtractedIdentifier> {
	// Check file size
	let metadata = match std::fs::metadata(path) {
		Ok(m) => m,
		Err(_) => return vec![],
	};
	if metadata.len() > MAX_FILE_SIZE {
		return vec![];
	}
	let content = match std::fs::read_to_string(path) {
		Ok(c) => c,
		Err(_) => return vec![], // Skip binary / unreadable files
	};
	let keywords: HashSet<&str> = KEYWORDS.iter().copied().collect();
	let use_kebab = path
		.extension()
		.and_then(|e| e.to_str())
		.is_some_and(|ext| KEBAB_EXTENSIONS.contains(&ext));
	let re = if use_kebab {
		Regex::new(r"\b[a-zA-Z_$][a-zA-Z0-9_$-]*\b").unwrap()
	} else {
		Regex::new(r"\b[a-zA-Z_$][a-zA-Z0-9_$]*\b").unwrap()
	};
	let mut results = Vec::new();
	for (line_idx, line) in content.lines().enumerate() {
		for m in re.find_iter(line) {
			let ident = m.as_str();
			// Filter out single-char identifiers
			if ident.len() <= 1 {
				continue;
			}
			// Filter out keywords (case-sensitive)
			if keywords.contains(ident) {
				continue;
			}
			let convention = parser::detect_convention(ident);
			let parsed = parser::parse(ident, convention);
			results.push(ExtractedIdentifier {
				file: path.to_path_buf(),
				line: line_idx + 1,
				column: m.start() + 1,
				identifier: ident.to_string(),
				parsed,
				context: None,
			});
		}
	}
	results
}

/// Extract identifiers from a single source file using tree-sitter AST
/// Falls back to regex extraction for unsupported languages
fn extract_from_file_ast(
	path: &Path,
) -> Vec<ExtractedIdentifier> {
	use crate::treesitter;
	if !treesitter::is_supported(path) {
		// Fall back to regex for unsupported languages
		return extract_from_file(path);
	}
	let contextual = treesitter::extract_with_context(path);
	contextual
		.into_iter()
		.map(|ci| {
			let convention =
				parser::detect_convention(&ci.identifier);
			let parsed =
				parser::parse(&ci.identifier, convention);
			ExtractedIdentifier {
				file: ci.file,
				line: ci.line,
				column: ci.column,
				identifier: ci.identifier,
				parsed,
				context: Some(ci.context),
			}
		})
		.collect()
}

/// Check if a walkdir entry is a directory we should skip
fn is_skipped_dir(entry: &walkdir::DirEntry) -> bool {
	if !entry.file_type().is_dir() {
		return false;
	}
	let name = entry.file_name().to_string_lossy();
	// Skip hidden directories (starting with .) except the root entry
	if entry.depth() > 0 && name.starts_with('.') {
		return true;
	}
	SKIP_DIRS.contains(&name.as_ref())
}

/// Check if a path is a recognized source file by extension
fn is_source_file(path: &Path) -> bool {
	path.extension()
		.and_then(|e| e.to_str())
		.is_some_and(|ext| SOURCE_EXTENSIONS.contains(&ext))
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::io::Write;

	#[test]
	fn test_is_source_file() {
		assert!(is_source_file(Path::new("foo.rs")));
		assert!(is_source_file(Path::new("bar.py")));
		assert!(is_source_file(Path::new("baz.css")));
		assert!(!is_source_file(Path::new("readme.md")));
		assert!(!is_source_file(Path::new("data.json")));
	}

	#[test]
	fn test_extract_from_file() {
		let dir = std::env::temp_dir().join("tagpath_test_extract");
		let _ = std::fs::create_dir_all(&dir);
		let file = dir.join("sample.rs");
		{
			let mut f = std::fs::File::create(&file).unwrap();
			writeln!(f, "fn create_user(person_name: &str) {{}}").unwrap();
		}
		let results = extract_from_file(&file);
		// Should find create_user and person_name (fn is a keyword, str is only 3 chars but not a keyword)
		let idents: Vec<&str> =
			results.iter().map(|r| r.identifier.as_str()).collect();
		assert!(idents.contains(&"create_user"));
		assert!(idents.contains(&"person_name"));
		// fn is a keyword, should be excluded
		assert!(!idents.contains(&"fn"));
		// context should be None in regex mode
		assert!(results.iter().all(|r| r.context.is_none()));
		let _ = std::fs::remove_dir_all(&dir);
	}

	#[test]
	fn test_extract_filters_single_char() {
		let dir = std::env::temp_dir().join("tagpath_test_single_char");
		let _ = std::fs::create_dir_all(&dir);
		let file = dir.join("sample.py");
		{
			let mut f = std::fs::File::create(&file).unwrap();
			writeln!(f, "x = get_value(i)").unwrap();
		}
		let results = extract_from_file(&file);
		let idents: Vec<&str> =
			results.iter().map(|r| r.identifier.as_str()).collect();
		assert!(!idents.contains(&"x"));
		assert!(!idents.contains(&"i"));
		assert!(idents.contains(&"get_value"));
		let _ = std::fs::remove_dir_all(&dir);
	}

	#[test]
	fn test_extract_kebab_case_css() {
		let dir = std::env::temp_dir().join("tagpath_test_kebab");
		let _ = std::fs::create_dir_all(&dir);
		let file = dir.join("style.css");
		{
			let mut f = std::fs::File::create(&file).unwrap();
			writeln!(f, ".main-container {{ font-size: 16px; }}").unwrap();
		}
		let results = extract_from_file(&file);
		let idents: Vec<&str> =
			results.iter().map(|r| r.identifier.as_str()).collect();
		assert!(idents.contains(&"main-container"));
		assert!(idents.contains(&"font-size"));
		let _ = std::fs::remove_dir_all(&dir);
	}

	#[cfg(feature = "lang-rust")]
	#[test]
	fn test_extract_ast_mode() {
		let dir = std::env::temp_dir().join("tagpath_test_ast_mode");
		let _ = std::fs::create_dir_all(&dir);
		let file = dir.join("sample.rs");
		{
			let mut f = std::fs::File::create(&file).unwrap();
			writeln!(
				f,
				"fn create_user(person_name: &str) {{}}"
			)
			.unwrap();
		}
		let results =
			extract_from_path_with_mode(&file, true);
		// AST mode should populate context
		let fns: Vec<&str> = results
			.iter()
			.filter(|r| {
				r.context == Some(IdentifierContext::Function)
			})
			.map(|r| r.identifier.as_str())
			.collect();
		assert!(fns.contains(&"create_user"));
		let params: Vec<&str> = results
			.iter()
			.filter(|r| {
				r.context == Some(IdentifierContext::Parameter)
			})
			.map(|r| r.identifier.as_str())
			.collect();
		assert!(params.contains(&"person_name"));
		let _ = std::fs::remove_dir_all(&dir);
	}

	#[test]
	fn test_extract_ast_fallback_unsupported() {
		let dir = std::env::temp_dir()
			.join("tagpath_test_ast_fallback");
		let _ = std::fs::create_dir_all(&dir);
		let file = dir.join("style.css");
		{
			let mut f = std::fs::File::create(&file).unwrap();
			writeln!(
				f,
				".main-container {{ font-size: 16px; }}"
			)
			.unwrap();
		}
		// AST mode should fall back to regex for CSS
		let results =
			extract_from_path_with_mode(&file, true);
		let idents: Vec<&str> =
			results.iter().map(|r| r.identifier.as_str()).collect();
		assert!(idents.contains(&"main-container"));
		// Regex fallback means context is None
		assert!(results.iter().all(|r| r.context.is_none()));
		let _ = std::fs::remove_dir_all(&dir);
	}
}
