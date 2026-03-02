mod config;
mod parser;

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
	/// Lint identifiers against .naming.yml rules
	Lint {
		/// Path to lint
		#[arg(default_value = ".")]
		path: PathBuf,
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
		Commands::Lint { path } => cmd_lint(&path),
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
			println!("convention: {:?}", parsed.convention);
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

fn cmd_lint(path: &std::path::Path) {
	let _ = path;
	eprintln!("tagpath lint: not yet implemented (Phase 2)");
	std::process::exit(1);
}
