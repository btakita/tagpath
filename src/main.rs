mod alias;
mod config;
mod extract;
mod graph;
mod lint;
mod parser;
mod prose;
mod search;
mod treesitter;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "tagpath", about = "Tag Path — parse, lint, and search tag-based identifiers")]
struct Cli {
	#[command(subcommand)]
	command: Commands,
}

#[derive(Subcommand)]
enum Commands {
	/// Parse a name into its constituent tags and structure
	Parse {
		/// The identifier to parse
		name: String,
		/// Convention to parse as (auto-detected if omitted)
		#[arg(short, long)]
		convention: Option<String>,
		/// Output format
		#[arg(short, long, default_value = "text")]
		format: String,
	},
	/// Initialize a .naming.toml in the current directory
	Init {
		/// Language preset to use
		#[arg(short, long)]
		lang: Option<String>,
		/// Convention preset to use
		#[arg(short, long)]
		preset: Option<String>,
	},
	/// Lint identifiers against .naming.toml rules
	Lint {
		/// Path to lint
		#[arg(default_value = ".")]
		path: PathBuf,
		/// Output format
		#[arg(short, long, default_value = "text")]
		format: String,
	},
	/// Extract identifiers from source files and parse their tag structure
	Extract {
		/// Path to extract from (file or directory)
		path: PathBuf,
		/// Output format
		#[arg(short, long, default_value = "text")]
		format: String,
		/// Use tree-sitter AST for context-aware extraction
		#[arg(long)]
		ast: bool,
	},
	/// Search for identifiers matching a tag query
	Search {
		/// Tag query (e.g. "user", "create_user", "createUser")
		query: String,
		/// Path to search in (file or directory)
		path: PathBuf,
		/// Output format
		#[arg(short, long, default_value = "text")]
		format: String,
	},
	/// Generate aliases for an identifier in all naming conventions
	Alias {
		/// The identifier to generate aliases for
		name: String,
		/// Target convention (show only this convention's alias)
		#[arg(short, long)]
		convention: Option<String>,
		/// Output format
		#[arg(short, long, default_value = "text")]
		format: String,
	},
	/// Generate a human-readable prose description of an identifier
	Prose {
		/// The identifier to describe
		name: String,
		/// Output format
		#[arg(short, long, default_value = "text")]
		format: String,
	},
	/// Build a tag co-occurrence graph from extracted identifiers
	Graph {
		/// Path to scan (file or directory)
		#[arg(default_value = ".")]
		path: PathBuf,
		/// Output format (dot, json, text)
		#[arg(short, long, default_value = "text")]
		format: String,
		/// Filter to subgraph around these tags
		#[arg(short, long)]
		query: Option<String>,
	},
}

fn main() {
	let cli = Cli::parse();
	match cli.command {
		Commands::Parse {
			name,
			convention,
			format,
		} => cmd_parse(&name, convention.as_deref(), &format),
		Commands::Init { lang, preset } => {
			cmd_init(lang.as_deref(), preset.as_deref())
		}
		Commands::Lint { path, format } => {
			cmd_lint(&path, &format)
		}
		Commands::Extract { path, format, ast } => {
			cmd_extract(&path, &format, ast)
		}
		Commands::Search {
			query,
			path,
			format,
		} => cmd_search(&query, &path, &format),
		Commands::Alias {
			name,
			convention,
			format,
		} => cmd_alias(&name, convention.as_deref(), &format),
		Commands::Prose { name, format } => {
			cmd_prose(&name, &format)
		}
		Commands::Graph {
			path,
			format,
			query,
		} => cmd_graph(&path, &format, query.as_deref()),
	}
}

fn cmd_parse(name: &str, convention: Option<&str>, format: &str) {
	let conv = convention
		.and_then(|c| c.parse::<parser::Convention>().ok())
		.unwrap_or_else(|| parser::detect_convention(name));
	let parsed = parser::parse(name, conv);
	match format {
		"json" => {
			println!(
				"{}",
				serde_json::to_string_pretty(&parsed).unwrap()
			);
		}
		_ => {
			println!("name:       {}", parsed.original);
			println!("convention: {}", parsed.convention);
			println!(
				"tags:       [{}]",
				parsed.tags.join(", ")
			);
			if !parsed.namespaces.is_empty() {
				for (i, ns) in parsed.namespaces.iter().enumerate() {
					println!(
						"dimension {}: [{}]",
						i,
						ns.join(", ")
					);
				}
			}
			if let Some(ref role) = parsed.role {
				println!("role:       {}", role);
			}
			if let Some(ref shape) = parsed.shape {
				println!("shape:      {}", shape);
			}
			println!(
				"canonical:  {}",
				parsed.tags.join("_")
			);
		}
	}
}

fn cmd_init(lang: Option<&str>, preset: Option<&str>) {
	let config = config::generate_config(lang, preset);
	let path = std::path::Path::new(".naming.toml");
	if path.exists() {
		eprintln!(".naming.toml already exists");
		std::process::exit(1);
	}
	std::fs::write(path, config).expect("failed to write .naming.toml");
	println!("Created .naming.toml");
}

fn cmd_lint(path: &std::path::Path, format: &str) {
	// Find .naming.toml by walking up from the target path
	let config_path = match lint::find_config(path) {
		Some(p) => p,
		None => {
			eprintln!(
				"error: no .naming.toml found (searched from {} upward)",
				path.display()
			);
			eprintln!(
				"hint: run `tagpath init` to create one"
			);
			std::process::exit(1);
		}
	};
	let naming_config = match config::resolve(&config_path) {
		Ok(c) => c,
		Err(e) => {
			eprintln!("error: {e}");
			std::process::exit(1);
		}
	};
	let violations = lint::lint(path, &naming_config);
	if violations.is_empty() {
		println!("No naming convention violations found.");
		return;
	}
	match format {
		"json" => {
			println!(
				"{}",
				serde_json::to_string_pretty(&violations)
					.unwrap()
			);
		}
		_ => {
			for v in &violations {
				println!(
					"{}:{}:{} warning: {} `{}` should be {} → `{}`",
					v.file.display(),
					v.line,
					v.column,
					context_label(&v.identifier, &v.expected_convention),
					v.identifier,
					v.expected_convention,
					v.suggested_fix.as_deref().unwrap_or("?"),
				);
			}
			eprintln!(
				"\nFound {} violation(s).",
				violations.len()
			);
		}
	}
	std::process::exit(1);
}

/// Generate a human-readable context label from the convention context
fn context_label(
	_identifier: &str,
	expected: &str,
) -> &'static str {
	// The convention name hints at the context
	match expected {
		"snake_case" => "identifier",
		"PascalCase" => "type",
		"camelCase" | "camel" => "identifier",
		"UPPER_SNAKE_CASE" | "upper_snake" | "screaming" => {
			"constant"
		}
		"kebab-case" | "kebab" => "identifier",
		_ => "identifier",
	}
}

fn cmd_extract(
	path: &std::path::Path,
	format: &str,
	ast: bool,
) {
	let results =
		extract::extract_from_path_with_mode(path, ast);
	match format {
		"json" => {
			println!(
				"{}",
				serde_json::to_string_pretty(&results).unwrap()
			);
		}
		_ => {
			for r in &results {
				let role_str = r
					.parsed
					.role
					.as_deref()
					.unwrap_or("none");
				let shape_str = r
					.parsed
					.shape
					.as_deref()
					.unwrap_or("none");
				let ctx_str = match &r.context {
					Some(c) => format!("ctx:{c}"),
					None => "ctx:none".to_string(),
				};
				println!(
					"{}:{}\t{}\t[{}]\t{:?}\t{}\trole:{}\tshape:{}",
					r.file.display(),
					r.line,
					r.identifier,
					r.parsed.tags.join(", "),
					r.parsed.convention,
					ctx_str,
					role_str,
					shape_str,
				);
			}
		}
	}
}

fn cmd_alias(name: &str, convention: Option<&str>, format: &str) {
	let target = convention
		.and_then(|c| c.parse::<parser::Convention>().ok());
	let result = alias::generate_aliases(name, target);
	match format {
		"json" => {
			println!(
				"{}",
				serde_json::to_string_pretty(&result).unwrap()
			);
		}
		_ => {
			for (conv_name, alias_value) in &result.aliases {
				println!(
					"{:<16} {}",
					format!("{conv_name}:"),
					alias_value
				);
			}
		}
	}
}

fn cmd_prose(name: &str, format: &str) {
	let result = prose::to_prose(name);
	match format {
		"json" => {
			println!(
				"{}",
				serde_json::to_string_pretty(&result).unwrap()
			);
		}
		_ => {
			println!("{}", result.prose);
		}
	}
}

fn cmd_graph(
	path: &std::path::Path,
	format: &str,
	query: Option<&str>,
) {
	let tag_graph = graph::build_graph(path);
	match format {
		"json" => {
			let json = graph::to_json(&tag_graph, query);
			println!(
				"{}",
				serde_json::to_string_pretty(&json).unwrap()
			);
		}
		"dot" => {
			print!("{}", graph::to_dot(&tag_graph, query));
		}
		_ => {
			// Default text format outputs DOT
			print!("{}", graph::to_dot(&tag_graph, query));
		}
	}
}

fn cmd_search(query: &str, path: &std::path::Path, format: &str) {
	let results = search::search(query, path);
	match format {
		"json" => {
			println!(
				"{}",
				serde_json::to_string_pretty(&results).unwrap()
			);
		}
		_ => {
			for r in &results {
				println!(
					"{}:{}\t{}\t{:?}",
					r.file.display(),
					r.line,
					r.identifier,
					r.convention,
				);
			}
		}
	}
}
