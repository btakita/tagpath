use clap::{Parser, Subcommand};
use std::{io::Read, path::PathBuf};
use tagpath::{
    alias, compression, config, extract, family, graph, lint, ontology, parser, prose, query,
    search,
};

#[derive(Parser)]
#[command(
    name = "tagpath",
    about = "Tag Path — parse, lint, and search tag-based identifiers"
)]
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
        /// Output format (text, json, family, family-json)
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
        /// Output format (text, json, family, family-json)
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
    /// Generate a stable semantic tag family for an identifier
    Family {
        /// The identifier to summarize as a tag family
        name: String,
        /// Output format
        #[arg(short, long, default_value = "text")]
        format: String,
    },
    /// Group raw symbol rows and report family-preview compression savings
    CompressionReport {
        /// JSON file containing raw symbol rows, or '-' for stdin
        #[arg(default_value = "-")]
        input: PathBuf,
        /// Output format (text, json)
        #[arg(short, long, default_value = "text")]
        format: String,
        /// Maximum representative examples to keep per family
        #[arg(long, default_value_t = 3)]
        example_limit: usize,
    },
    /// Generate a human-readable prose description of an identifier
    Prose {
        /// The identifier to describe
        name: String,
        /// Output format
        #[arg(short, long, default_value = "text")]
        format: String,
    },
    /// Normalize a free-text query into ordered, weighted tags
    NormalizeQuery {
        /// Free-text query or agent prompt to normalize
        query: String,
        /// Output format
        #[arg(short, long, default_value = "text")]
        format: String,
    },
    /// Load and validate .naming/tags ontology markdown files
    Ontology {
        /// Project path containing .naming.toml and .naming/tags
        #[arg(default_value = ".")]
        path: PathBuf,
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
        Commands::Init { lang, preset } => cmd_init(lang.as_deref(), preset.as_deref()),
        Commands::Lint { path, format } => cmd_lint(&path, &format),
        Commands::Extract { path, format, ast } => cmd_extract(&path, &format, ast),
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
        Commands::Family { name, format } => cmd_family(&name, &format),
        Commands::CompressionReport {
            input,
            format,
            example_limit,
        } => cmd_compression_report(&input, &format, example_limit),
        Commands::Prose { name, format } => cmd_prose(&name, &format),
        Commands::NormalizeQuery { query, format } => cmd_normalize_query(&query, &format),
        Commands::Ontology { path, format } => cmd_ontology(&path, &format),
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
            println!("{}", serde_json::to_string_pretty(&parsed).unwrap());
        }
        _ => {
            println!("name:       {}", parsed.original);
            println!("convention: {}", parsed.convention);
            println!("tags:       [{}]", parsed.tags.join(", "));
            if !parsed.namespaces.is_empty() {
                for (i, ns) in parsed.namespaces.iter().enumerate() {
                    println!("dimension {}: [{}]", i, ns.join(", "));
                }
            }
            if let Some(ref role) = parsed.role {
                println!("role:       {}", role);
            }
            if let Some(ref shape) = parsed.shape {
                println!("shape:      {}", shape);
            }
            println!("canonical:  {}", parsed.tags.join("_"));
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
            eprintln!("hint: run `tagpath init` to create one");
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
            println!("{}", serde_json::to_string_pretty(&violations).unwrap());
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
            eprintln!("\nFound {} violation(s).", violations.len());
        }
    }
    std::process::exit(1);
}

/// Generate a human-readable context label from the convention context
fn context_label(_identifier: &str, expected: &str) -> &'static str {
    // The convention name hints at the context
    match expected {
        "snake_case" => "identifier",
        "PascalCase" => "type",
        "camelCase" | "camel" => "identifier",
        "UPPER_SNAKE_CASE" | "upper_snake" | "screaming" => "constant",
        "kebab-case" | "kebab" => "identifier",
        _ => "identifier",
    }
}

fn cmd_extract(path: &std::path::Path, format: &str, ast: bool) {
    match format {
        "family-json" => {
            let families = extract::extract_families_from_path_with_mode(path, ast);
            println!("{}", serde_json::to_string_pretty(&families).unwrap());
        }
        "family" => {
            let families = extract::extract_families_from_path_with_mode(path, ast);
            print_family_summaries(&families);
        }
        "json" => {
            let results = extract::extract_from_path_with_mode(path, ast);
            println!("{}", serde_json::to_string_pretty(&results).unwrap());
        }
        _ => {
            let results = extract::extract_from_path_with_mode(path, ast);
            for r in &results {
                let role_str = r.parsed.role.as_deref().unwrap_or("none");
                let shape_str = r.parsed.shape.as_deref().unwrap_or("none");
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
    let target = convention.and_then(|c| c.parse::<parser::Convention>().ok());
    let result = alias::generate_aliases(name, target);
    match format {
        "json" => {
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        _ => {
            for (conv_name, alias_value) in &result.aliases {
                println!("{:<16} {}", format!("{conv_name}:"), alias_value);
            }
        }
    }
}

fn cmd_family(name: &str, format: &str) {
    let result = family::generate_family(name);
    match format {
        "json" => {
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        _ => {
            println!("canonical: {}", result.canonical);
            println!("tags:      [{}]", result.tags.join(", "));
            for dimension in &result.dimensions {
                println!(
                    "dimension {}: [{}] ({})",
                    dimension.index,
                    dimension.tags.join(", "),
                    dimension.canonical
                );
            }
            if let Some(ref role) = result.role {
                println!("role:      {}", role);
            }
            if let Some(ref shape) = result.shape {
                println!("shape:     {}", shape);
            }
            println!("examples:");
            for example in &result.examples {
                println!(
                    "  {:<16} {}",
                    format!("{}:", example.convention),
                    example.spelling
                );
            }
        }
    }
}

fn cmd_compression_report(input: &std::path::Path, format: &str, example_limit: usize) {
    let input_text = match read_json_input(input) {
        Ok(input_text) => input_text,
        Err(error) => {
            eprintln!("error: {error}");
            std::process::exit(1);
        }
    };
    let rows = match parse_raw_symbol_rows(&input_text) {
        Ok(rows) => rows,
        Err(error) => {
            eprintln!("error: {error}");
            std::process::exit(1);
        }
    };
    let report = compression::build_report_with_example_limit(&rows, example_limit);
    match format {
        "json" => {
            println!("{}", serde_json::to_string_pretty(&report).unwrap());
        }
        _ => {
            println!("raw symbols:    {}", report.raw_symbol_count);
            println!("families:       {}", report.family_count);
            println!(
                "bytes:          {} -> {} (saved {} / {:.1}%)",
                report.metrics.raw_utf8_bytes,
                report.metrics.compact_utf8_bytes,
                report.metrics.saved_utf8_bytes,
                report.metrics.byte_savings_percent
            );
            println!(
                "tokens:         {} -> {} (saved {} / {:.1}%)",
                report.metrics.raw_tokens,
                report.metrics.compact_tokens,
                report.metrics.saved_tokens,
                report.metrics.token_savings_percent
            );
            if !report.compact_preview.is_empty() {
                println!("families:");
                println!("{}", report.compact_preview);
            }
        }
    }
}

fn read_json_input(input: &std::path::Path) -> Result<String, String> {
    if input == std::path::Path::new("-") {
        let mut buffer = String::new();
        std::io::stdin()
            .read_to_string(&mut buffer)
            .map_err(|error| format!("failed to read stdin: {error}"))?;
        Ok(buffer)
    } else {
        std::fs::read_to_string(input)
            .map_err(|error| format!("failed to read {}: {error}", input.display()))
    }
}

fn parse_raw_symbol_rows(input: &str) -> Result<Vec<compression::RawSymbolRow>, String> {
    let value: serde_json::Value =
        serde_json::from_str(input).map_err(|error| format!("invalid JSON: {error}"))?;
    if value.is_array() {
        return serde_json::from_value(value)
            .map_err(|error| format!("invalid raw symbol row array: {error}"));
    }
    if let Some(rows) = value.get("raw_symbols") {
        return serde_json::from_value(rows.clone())
            .map_err(|error| format!("invalid raw_symbols rows: {error}"));
    }
    if let Some(rows) = value.get("rows") {
        return serde_json::from_value(rows.clone())
            .map_err(|error| format!("invalid rows entries: {error}"));
    }
    Err("expected a JSON array or an object with raw_symbols/rows".to_string())
}

fn cmd_prose(name: &str, format: &str) {
    let result = prose::to_prose(name);
    match format {
        "json" => {
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        _ => {
            println!("{}", result.prose);
        }
    }
}

fn cmd_normalize_query(input: &str, format: &str) {
    let result = query::normalize_query(input);
    match format {
        "json" => {
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        _ => {
            for tag in &result.tags {
                println!(
                    "{}\tweight:{:.1}\toccurrences:{}\tsources:[{}]",
                    tag.tag,
                    tag.weight,
                    tag.occurrences,
                    tag.sources.join(", ")
                );
            }
        }
    }
}

fn cmd_ontology(path: &std::path::Path, format: &str) {
    let report = match ontology::load_project(path) {
        Ok(report) => report,
        Err(error) => {
            eprintln!("error: {error}");
            std::process::exit(1);
        }
    };
    match format {
        "json" => {
            println!("{}", serde_json::to_string_pretty(&report).unwrap());
        }
        _ => {
            println!("ontology: {}", report.ontology_dir.display());
            if let Some(config_path) = &report.config_path {
                println!("config:   {}", config_path.display());
            }
            println!("valid:    {}", report.validation.valid);
            for tag in &report.tags {
                let title = tag.title.as_deref().unwrap_or("-");
                let summary = tag.summary.as_deref().unwrap_or("-");
                println!("{}\t{}\t{}", tag.tag, title, summary);
            }
            for error in &report.validation.errors {
                eprintln!("error: {}", format_ontology_diagnostic(error));
            }
            for warning in &report.validation.warnings {
                eprintln!("warning: {}", format_ontology_diagnostic(warning));
            }
        }
    }
    if !report.validation.valid {
        std::process::exit(1);
    }
}

fn format_ontology_diagnostic(diagnostic: &ontology::OntologyDiagnostic) -> String {
    match (&diagnostic.tag, &diagnostic.path) {
        (Some(tag), Some(path)) => {
            format!("{tag} ({}): {}", path.display(), diagnostic.message)
        }
        (Some(tag), None) => format!("{tag}: {}", diagnostic.message),
        (None, Some(path)) => format!("{}: {}", path.display(), diagnostic.message),
        (None, None) => diagnostic.message.clone(),
    }
}

fn cmd_graph(path: &std::path::Path, format: &str, query: Option<&str>) {
    let tag_graph = graph::build_graph(path);
    match format {
        "json" => {
            let json = graph::to_json(&tag_graph, query);
            println!("{}", serde_json::to_string_pretty(&json).unwrap());
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
    match format {
        "family-json" => {
            let families = search::search_families(query, path);
            println!("{}", serde_json::to_string_pretty(&families).unwrap());
        }
        "family" => {
            let families = search::search_families(query, path);
            print_family_summaries(&families);
        }
        "json" => {
            let results = search::search(query, path);
            println!("{}", serde_json::to_string_pretty(&results).unwrap());
        }
        _ => {
            let results = search::search(query, path);
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

fn print_family_summaries(families: &[family::TagFamilySummary]) {
    for summary in families {
        println!(
            "{}\tcount:{}\ttags:[{}]",
            summary.canonical,
            summary.count,
            summary.tags.join(", ")
        );
        if !summary.roles.is_empty() {
            println!("  roles:  [{}]", summary.roles.join(", "));
        }
        if !summary.shapes.is_empty() {
            println!("  shapes: [{}]", summary.shapes.join(", "));
        }
        if !summary.examples.is_empty() {
            println!("  examples:");
            for example in &summary.examples {
                println!(
                    "    {}\t{}:{}:{}\t{}",
                    example.identifier,
                    example.file.display(),
                    example.line,
                    example.column,
                    example.convention
                );
            }
        }
    }
}
