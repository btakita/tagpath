use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Top-level .naming.toml configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamingConfig {
	pub version: u32,
	pub name: String,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub extends: Option<Vec<String>>,
	#[serde(default)]
	pub convention: String,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub immutable: Option<bool>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub singular: Option<bool>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub vectors: Option<VectorConfig>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub patterns: Option<HashMap<String, String>>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub externals: Option<ExternalConfig>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub packages: Option<PackageConfig>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub contexts: Option<HashMap<String, ContextConfig>>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub tags: Option<TagConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorConfig {
	#[serde(default = "default_join")]
	pub join: String,
	#[serde(default = "default_namespace")]
	pub namespace: String,
}

fn default_join() -> String {
	"_".to_string()
}
fn default_namespace() -> String {
	"__".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalConfig {
	#[serde(default = "default_true")]
	pub preserve_casing: bool,
	#[serde(default = "default_join")]
	pub join_with: String,
}

fn default_true() -> bool {
	true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageConfig {
	#[serde(default)]
	pub separator: String,
	#[serde(default)]
	pub pattern: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextConfig {
	pub convention: String,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub prefix: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub suffix: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagConfig {
	#[serde(default)]
	pub open: bool,
	#[serde(default)]
	pub declared: HashMap<String, TagDeclaration>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagDeclaration {
	#[serde(skip_serializing_if = "Option::is_none")]
	pub level: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub domain: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub shape: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub role: Option<String>,
}

/// Load a .naming.toml from a path (raw, no extends resolution)
pub fn load(path: &Path) -> Result<NamingConfig, String> {
	let content = std::fs::read_to_string(path)
		.map_err(|e| format!("failed to read {}: {e}", path.display()))?;
	toml::from_str(&content)
		.map_err(|e| format!("failed to parse {}: {e}", path.display()))
}

/// Load and resolve a .naming.toml, processing the `extends` chain.
///
/// Resolution order (last wins):
/// 1. Each entry in `extends` is loaded left-to-right
/// 2. The project's own config overrides everything
///
/// Extends entries resolve as:
/// - Built-in language name (e.g., `"rust"`) → bundled `lang/rust.toml`
/// - Built-in preset name (e.g., `"immutable-tag"`) → bundled `presets/immutable-tag.toml`
/// - Relative path (e.g., `"./team.toml"`) → resolved from config file's directory
pub fn resolve(path: &Path) -> Result<NamingConfig, String> {
	let config = load(path)?;
	let extends = match &config.extends {
		Some(list) if !list.is_empty() => list.clone(),
		_ => return Ok(config),
	};
	let config_dir = path
		.parent()
		.ok_or_else(|| "config path has no parent directory".to_string())?;
	// Start with an empty base and layer each extends entry
	let mut merged = empty_config();
	for entry in &extends {
		let base = load_extends_entry(entry, config_dir)?;
		merge_into(&mut merged, &base);
	}
	// Layer the project config on top (overrides everything)
	merge_into(&mut merged, &config);
	// Keep the project's name and version
	merged.name = config.name;
	merged.version = config.version;
	Ok(merged)
}

/// Load a single extends entry: built-in name or relative path
fn load_extends_entry(
	entry: &str,
	config_dir: &Path,
) -> Result<NamingConfig, String> {
	// Try built-in language/preset first
	if let Some(content) = resolve_builtin(entry) {
		return toml::from_str(content).map_err(|e| {
			format!("failed to parse built-in '{entry}': {e}")
		});
	}
	// Treat as relative path
	let resolved = config_dir.join(entry);
	if resolved.exists() {
		load(&resolved)
	} else {
		Err(format!(
			"extends '{entry}': not a built-in preset and file not found at {}",
			resolved.display()
		))
	}
}

/// Resolve a built-in language or preset name to its TOML content
fn resolve_builtin(name: &str) -> Option<&'static str> {
	match name {
		"c" => Some(include_str!("../../lang/c.toml")),
		"cpp" | "c++" | "cxx" => {
			Some(include_str!("../../lang/cpp.toml"))
		}
		"csharp" | "cs" | "c#" => {
			Some(include_str!("../../lang/csharp.toml"))
		}
		"clojure" | "clj" => {
			Some(include_str!("../../lang/clojure.toml"))
		}
		"common-lisp" | "cl" | "lisp" => {
			Some(include_str!("../../lang/common-lisp.toml"))
		}
		"crystal" | "cr" => {
			Some(include_str!("../../lang/crystal.toml"))
		}
		"css" => Some(include_str!("../../lang/css.toml")),
		"d" | "dlang" => Some(include_str!("../../lang/d.toml")),
		"dart" => Some(include_str!("../../lang/dart.toml")),
		"elixir" | "ex" => {
			Some(include_str!("../../lang/elixir.toml"))
		}
		"erlang" | "erl" => {
			Some(include_str!("../../lang/erlang.toml"))
		}
		"fsharp" | "fs" | "f#" => {
			Some(include_str!("../../lang/fsharp.toml"))
		}
		"gleam" => Some(include_str!("../../lang/gleam.toml")),
		"go" => Some(include_str!("../../lang/go.toml")),
		"haskell" | "hs" => {
			Some(include_str!("../../lang/haskell.toml"))
		}
		"java" => Some(include_str!("../../lang/java.toml")),
		"javascript" | "js" => {
			Some(include_str!("../../lang/javascript.toml"))
		}
		"julia" | "jl" => {
			Some(include_str!("../../lang/julia.toml"))
		}
		"kotlin" | "kt" => {
			Some(include_str!("../../lang/kotlin.toml"))
		}
		"lua" => Some(include_str!("../../lang/lua.toml")),
		"nim" => Some(include_str!("../../lang/nim.toml")),
		"objective-c" | "objc" => {
			Some(include_str!("../../lang/objective-c.toml"))
		}
		"ocaml" | "ml" => {
			Some(include_str!("../../lang/ocaml.toml"))
		}
		"odin" => Some(include_str!("../../lang/odin.toml")),
		"perl" | "pl" => {
			Some(include_str!("../../lang/perl.toml"))
		}
		"php" => Some(include_str!("../../lang/php.toml")),
		"python" | "py" => {
			Some(include_str!("../../lang/python.toml"))
		}
		"r" => Some(include_str!("../../lang/r.toml")),
		"racket" | "rkt" => {
			Some(include_str!("../../lang/racket.toml"))
		}
		"ruby" | "rb" => {
			Some(include_str!("../../lang/ruby.toml"))
		}
		"rust" | "rs" => {
			Some(include_str!("../../lang/rust.toml"))
		}
		"scala" => Some(include_str!("../../lang/scala.toml")),
		"scheme" | "scm" => {
			Some(include_str!("../../lang/scheme.toml"))
		}
		"shell" | "sh" | "bash" | "zsh" => {
			Some(include_str!("../../lang/shell.toml"))
		}
		"sql" => Some(include_str!("../../lang/sql.toml")),
		"swift" => Some(include_str!("../../lang/swift.toml")),
		"typescript" | "ts" => {
			Some(include_str!("../../lang/typescript.toml"))
		}
		"v" | "vlang" => Some(include_str!("../../lang/v.toml")),
		"zig" => Some(include_str!("../../lang/zig.toml")),
		// Presets
		"immutable-tag" => {
			Some(include_str!("../../presets/immutable-tag.toml"))
		}
		_ => None,
	}
}

/// Create an empty config as merge base
fn empty_config() -> NamingConfig {
	NamingConfig {
		version: 1,
		name: String::new(),
		extends: None,
		convention: String::new(),
		immutable: None,
		singular: None,
		vectors: None,
		patterns: None,
		externals: None,
		packages: None,
		contexts: None,
		tags: None,
	}
}

/// Merge `source` into `target`. Non-empty/non-None fields in source override target.
/// For maps (contexts, patterns), individual keys from source override target keys.
fn merge_into(target: &mut NamingConfig, source: &NamingConfig) {
	if !source.convention.is_empty() {
		target.convention = source.convention.clone();
	}
	// Booleans: only override if source explicitly sets them
	if source.immutable.is_some() {
		target.immutable = source.immutable;
	}
	if source.singular.is_some() {
		target.singular = source.singular;
	}
	if source.vectors.is_some() {
		target.vectors = source.vectors.clone();
	}
	if let Some(ref src_patterns) = source.patterns {
		let tgt = target.patterns.get_or_insert_with(HashMap::new);
		for (k, v) in src_patterns {
			tgt.insert(k.clone(), v.clone());
		}
	}
	if source.externals.is_some() {
		target.externals = source.externals.clone();
	}
	if source.packages.is_some() {
		target.packages = source.packages.clone();
	}
	if let Some(ref src_contexts) = source.contexts {
		let tgt = target.contexts.get_or_insert_with(HashMap::new);
		for (k, v) in src_contexts {
			tgt.insert(k.clone(), v.clone());
		}
	}
	if let Some(ref src_tags) = source.tags {
		match target.tags {
			Some(ref mut tgt_tags) => {
				tgt_tags.open = src_tags.open;
				for (k, v) in &src_tags.declared {
					tgt_tags.declared.insert(k.clone(), v.clone());
				}
			}
			None => target.tags = source.tags.clone(),
		}
	}
}

/// Generate a .naming.toml config from a language/preset name
pub fn generate_config(
	lang: Option<&str>,
	preset: Option<&str>,
) -> String {
	match (lang, preset) {
		(Some("typescript") | Some("ts"), _) => {
			include_str!("../../lang/typescript.toml").to_string()
		}
		(Some("python") | Some("py"), _) => {
			include_str!("../../lang/python.toml").to_string()
		}
		(Some("rust") | Some("rs"), _) => {
			include_str!("../../lang/rust.toml").to_string()
		}
		(Some("javascript") | Some("js"), _) => {
			include_str!("../../lang/javascript.toml").to_string()
		}
		(Some("go"), _) => {
			include_str!("../../lang/go.toml").to_string()
		}
		(Some("java"), _) => {
			include_str!("../../lang/java.toml").to_string()
		}
		(Some("ruby") | Some("rb"), _) => {
			include_str!("../../lang/ruby.toml").to_string()
		}
		(Some("swift"), _) => {
			include_str!("../../lang/swift.toml").to_string()
		}
		(Some("kotlin") | Some("kt"), _) => {
			include_str!("../../lang/kotlin.toml").to_string()
		}
		(Some("c"), _) => {
			include_str!("../../lang/c.toml").to_string()
		}
		(Some("cpp") | Some("c++") | Some("cxx"), _) => {
			include_str!("../../lang/cpp.toml").to_string()
		}
		(Some("csharp") | Some("cs") | Some("c#"), _) => {
			include_str!("../../lang/csharp.toml").to_string()
		}
		(Some("php"), _) => {
			include_str!("../../lang/php.toml").to_string()
		}
		(Some("elixir") | Some("ex"), _) => {
			include_str!("../../lang/elixir.toml").to_string()
		}
		(Some("css"), _) => {
			include_str!("../../lang/css.toml").to_string()
		}
		(Some("sql"), _) => {
			include_str!("../../lang/sql.toml").to_string()
		}
		(Some("shell") | Some("sh") | Some("bash") | Some("zsh"), _) => {
			include_str!("../../lang/shell.toml").to_string()
		}
		(Some("zig"), _) => {
			include_str!("../../lang/zig.toml").to_string()
		}
		(Some("odin"), _) => {
			include_str!("../../lang/odin.toml").to_string()
		}
		(Some("nim"), _) => {
			include_str!("../../lang/nim.toml").to_string()
		}
		(Some("haskell") | Some("hs"), _) => {
			include_str!("../../lang/haskell.toml").to_string()
		}
		(Some("d") | Some("dlang"), _) => {
			include_str!("../../lang/d.toml").to_string()
		}
		(Some("lua"), _) => {
			include_str!("../../lang/lua.toml").to_string()
		}
		(Some("perl") | Some("pl"), _) => {
			include_str!("../../lang/perl.toml").to_string()
		}
		(Some("clojure") | Some("clj"), _) => {
			include_str!("../../lang/clojure.toml").to_string()
		}
		(Some("r"), _) => {
			include_str!("../../lang/r.toml").to_string()
		}
		(Some("scala"), _) => {
			include_str!("../../lang/scala.toml").to_string()
		}
		(Some("dart"), _) => {
			include_str!("../../lang/dart.toml").to_string()
		}
		// New languages
		(Some("common-lisp") | Some("cl") | Some("lisp"), _) => {
			include_str!("../../lang/common-lisp.toml").to_string()
		}
		(Some("scheme") | Some("scm"), _) => {
			include_str!("../../lang/scheme.toml").to_string()
		}
		(Some("racket") | Some("rkt"), _) => {
			include_str!("../../lang/racket.toml").to_string()
		}
		(Some("erlang") | Some("erl"), _) => {
			include_str!("../../lang/erlang.toml").to_string()
		}
		(Some("fsharp") | Some("fs") | Some("f#"), _) => {
			include_str!("../../lang/fsharp.toml").to_string()
		}
		(Some("ocaml") | Some("ml"), _) => {
			include_str!("../../lang/ocaml.toml").to_string()
		}
		(Some("julia") | Some("jl"), _) => {
			include_str!("../../lang/julia.toml").to_string()
		}
		(Some("objective-c") | Some("objc"), _) => {
			include_str!("../../lang/objective-c.toml").to_string()
		}
		(Some("v") | Some("vlang"), _) => {
			include_str!("../../lang/v.toml").to_string()
		}
		(Some("crystal") | Some("cr"), _) => {
			include_str!("../../lang/crystal.toml").to_string()
		}
		(Some("gleam"), _) => {
			include_str!("../../lang/gleam.toml").to_string()
		}
		(_, Some("immutable-tag")) => {
			include_str!("../../presets/immutable-tag.toml").to_string()
		}
		_ => default_config(),
	}
}

fn default_config() -> String {
	r#"version = 1
name = "my-project"

convention = "snake_case"
immutable = true
singular = true

[vectors]
join = "_"
namespace = "__"

[patterns]
factory = "create_{name}"
hook = "use_{name}"
setter = "set_{name}"
signal = "{name}$"
type = "{name}_T"
array = "{name}_a"

[externals]
preserve_casing = true
join_with = "_"

[tags]
open = true
"#
	.to_string()
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::io::Write;

	#[test]
	fn test_resolve_builtin() {
		assert!(resolve_builtin("rust").is_some());
		assert!(resolve_builtin("python").is_some());
		assert!(resolve_builtin("immutable-tag").is_some());
		assert!(resolve_builtin("nonexistent").is_none());
	}

	#[test]
	fn test_resolve_extends_builtin() {
		let dir =
			std::env::temp_dir().join("tagpath_test_resolve_extends");
		let _ = std::fs::create_dir_all(&dir);
		let config_path = dir.join(".naming.toml");
		{
			let mut f =
				std::fs::File::create(&config_path).unwrap();
			write!(
				f,
				r#"version = 1
name = "my-project"
extends = ["rust"]

[contexts.function]
convention = "camelCase"
"#
			)
			.unwrap();
		}
		let resolved = resolve(&config_path).unwrap();
		assert_eq!(resolved.name, "my-project");
		// Function convention overridden to camelCase
		let contexts = resolved.contexts.unwrap();
		assert_eq!(
			contexts.get("function").unwrap().convention,
			"camelCase"
		);
		// Type convention inherited from Rust base (PascalCase)
		assert_eq!(
			contexts.get("type").unwrap().convention,
			"PascalCase"
		);
		// Variable convention inherited from Rust base
		assert_eq!(
			contexts.get("variable").unwrap().convention,
			"snake_case"
		);
		let _ = std::fs::remove_dir_all(&dir);
	}

	#[test]
	fn test_resolve_no_extends() {
		let dir =
			std::env::temp_dir().join("tagpath_test_resolve_none");
		let _ = std::fs::create_dir_all(&dir);
		let config_path = dir.join(".naming.toml");
		{
			let mut f =
				std::fs::File::create(&config_path).unwrap();
			write!(
				f,
				r#"version = 1
name = "simple"
convention = "snake_case"
"#
			)
			.unwrap();
		}
		let resolved = resolve(&config_path).unwrap();
		assert_eq!(resolved.name, "simple");
		assert_eq!(resolved.convention, "snake_case");
		assert!(resolved.contexts.is_none());
		let _ = std::fs::remove_dir_all(&dir);
	}

	#[test]
	fn test_resolve_extends_relative_path() {
		let dir =
			std::env::temp_dir().join("tagpath_test_resolve_rel");
		let _ = std::fs::create_dir_all(&dir);
		// Create a base config file
		let base_path = dir.join("base.toml");
		{
			let mut f =
				std::fs::File::create(&base_path).unwrap();
			write!(
				f,
				r#"version = 1
name = "base"
convention = "snake_case"

[contexts.function]
convention = "snake_case"

[contexts.type]
convention = "PascalCase"
"#
			)
			.unwrap();
		}
		// Create project config that extends the base
		let config_path = dir.join(".naming.toml");
		{
			let mut f =
				std::fs::File::create(&config_path).unwrap();
			write!(
				f,
				r#"version = 1
name = "project"
extends = ["./base.toml"]

[contexts.function]
convention = "camelCase"
"#
			)
			.unwrap();
		}
		let resolved = resolve(&config_path).unwrap();
		assert_eq!(resolved.name, "project");
		let contexts = resolved.contexts.unwrap();
		// Function overridden
		assert_eq!(
			contexts.get("function").unwrap().convention,
			"camelCase"
		);
		// Type inherited from base
		assert_eq!(
			contexts.get("type").unwrap().convention,
			"PascalCase"
		);
		let _ = std::fs::remove_dir_all(&dir);
	}

	#[test]
	fn test_resolve_extends_chain() {
		let dir =
			std::env::temp_dir().join("tagpath_test_resolve_chain");
		let _ = std::fs::create_dir_all(&dir);
		// Extends rust + immutable-tag, then overrides
		let config_path = dir.join(".naming.toml");
		{
			let mut f =
				std::fs::File::create(&config_path).unwrap();
			write!(
				f,
				r#"version = 1
name = "layered"
extends = ["rust", "immutable-tag"]

[contexts.constant]
convention = "camelCase"
"#
			)
			.unwrap();
		}
		let resolved = resolve(&config_path).unwrap();
		assert_eq!(resolved.name, "layered");
		// immutable-tag should have set immutable = true
		assert_eq!(resolved.immutable, Some(true));
		// constant convention overridden by project
		let contexts = resolved.contexts.unwrap();
		assert_eq!(
			contexts.get("constant").unwrap().convention,
			"camelCase"
		);
		// type convention from rust base still present
		assert_eq!(
			contexts.get("type").unwrap().convention,
			"PascalCase"
		);
		let _ = std::fs::remove_dir_all(&dir);
	}

	#[test]
	fn test_merge_patterns() {
		let dir =
			std::env::temp_dir().join("tagpath_test_merge_pat");
		let _ = std::fs::create_dir_all(&dir);
		let base_path = dir.join("base.toml");
		{
			let mut f =
				std::fs::File::create(&base_path).unwrap();
			write!(
				f,
				r#"version = 1
name = "base"

[patterns]
factory = "create_{{name}}"
hook = "use_{{name}}"
"#
			)
			.unwrap();
		}
		let config_path = dir.join(".naming.toml");
		{
			let mut f =
				std::fs::File::create(&config_path).unwrap();
			write!(
				f,
				r#"version = 1
name = "project"
extends = ["./base.toml"]

[patterns]
factory = "make_{{name}}"
signal = "{{name}}$"
"#
			)
			.unwrap();
		}
		let resolved = resolve(&config_path).unwrap();
		let patterns = resolved.patterns.unwrap();
		// factory overridden
		assert_eq!(patterns.get("factory").unwrap(), "make_{name}");
		// hook inherited
		assert_eq!(patterns.get("hook").unwrap(), "use_{name}");
		// signal added
		assert_eq!(patterns.get("signal").unwrap(), "{name}$");
		let _ = std::fs::remove_dir_all(&dir);
	}
}
