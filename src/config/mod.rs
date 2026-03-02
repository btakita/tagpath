use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Top-level .naming.yml configuration
#[derive(Debug, Serialize, Deserialize)]
pub struct NamingConfig {
	pub version: u32,
	pub name: String,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub extends: Option<Vec<String>>,
	#[serde(default)]
	pub convention: String,
	#[serde(default)]
	pub immutable: bool,
	#[serde(default)]
	pub singular: bool,
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

#[derive(Debug, Serialize, Deserialize)]
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

#[derive(Debug, Serialize, Deserialize)]
pub struct ExternalConfig {
	#[serde(default = "default_true")]
	pub preserve_casing: bool,
	#[serde(default = "default_join")]
	pub join_with: String,
}

fn default_true() -> bool {
	true
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PackageConfig {
	#[serde(default)]
	pub separator: String,
	#[serde(default)]
	pub pattern: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ContextConfig {
	pub convention: String,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub prefix: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub suffix: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TagConfig {
	#[serde(default)]
	pub open: bool,
	#[serde(default)]
	pub declared: HashMap<String, TagDeclaration>,
}

#[derive(Debug, Serialize, Deserialize)]
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

/// Load a .naming.yml from a path
pub fn load(path: &Path) -> Result<NamingConfig, String> {
	let content = std::fs::read_to_string(path)
		.map_err(|e| format!("failed to read {}: {e}", path.display()))?;
	serde_yaml::from_str(&content)
		.map_err(|e| format!("failed to parse {}: {e}", path.display()))
}

/// Generate a .naming.yml config from a language/preset name
pub fn generate_config(
	lang: Option<&str>,
	preset: Option<&str>,
) -> String {
	match (lang, preset) {
		(Some("typescript") | Some("ts"), _) => {
			include_str!("../../lang/typescript.yml").to_string()
		}
		(Some("python") | Some("py"), _) => {
			include_str!("../../lang/python.yml").to_string()
		}
		(Some("rust") | Some("rs"), _) => {
			include_str!("../../lang/rust.yml").to_string()
		}
		(Some("javascript") | Some("js"), _) => {
			include_str!("../../lang/javascript.yml").to_string()
		}
		(Some("go"), _) => {
			include_str!("../../lang/go.yml").to_string()
		}
		(Some("java"), _) => {
			include_str!("../../lang/java.yml").to_string()
		}
		(Some("ruby") | Some("rb"), _) => {
			include_str!("../../lang/ruby.yml").to_string()
		}
		(Some("swift"), _) => {
			include_str!("../../lang/swift.yml").to_string()
		}
		(Some("kotlin") | Some("kt"), _) => {
			include_str!("../../lang/kotlin.yml").to_string()
		}
		(Some("c"), _) => {
			include_str!("../../lang/c.yml").to_string()
		}
		(Some("cpp") | Some("c++") | Some("cxx"), _) => {
			include_str!("../../lang/cpp.yml").to_string()
		}
		(Some("csharp") | Some("cs") | Some("c#"), _) => {
			include_str!("../../lang/csharp.yml").to_string()
		}
		(Some("php"), _) => {
			include_str!("../../lang/php.yml").to_string()
		}
		(Some("elixir") | Some("ex"), _) => {
			include_str!("../../lang/elixir.yml").to_string()
		}
		(Some("css"), _) => {
			include_str!("../../lang/css.yml").to_string()
		}
		(Some("sql"), _) => {
			include_str!("../../lang/sql.yml").to_string()
		}
		(Some("shell") | Some("sh") | Some("bash") | Some("zsh"), _) => {
			include_str!("../../lang/shell.yml").to_string()
		}
		(Some("zig"), _) => {
			include_str!("../../lang/zig.yml").to_string()
		}
		(_, Some("immutable-tag")) => {
			include_str!("../../presets/immutable-tag.yml").to_string()
		}
		_ => default_config(),
	}
}

fn default_config() -> String {
	r#"version: 1
name: my-project

convention: snake_case
immutable: true
singular: true

vectors:
  join: "_"
  namespace: "__"

patterns:
  factory: "create_{name}"
  hook: "use_{name}"
  setter: "set_{name}"
  signal: "{name}$"
  type: "{name}_T"
  array: "{name}_a"

externals:
  preserve_casing: true
  join_with: "_"

tags:
  open: true
"#
	.to_string()
}
