use clap::{Parser, Subcommand};
use std::{
    io::{Read, Write},
    path::PathBuf,
};
use tagpath::{
    alias, compression, config, extract, family, graph, index, lint, ontology, parser, prose,
    query, rename as rename_mod, search,
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
    /// Lint identifiers against .naming.toml rules, or agent-doc session-document tags
    Lint {
        /// Path to lint
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Output format
        #[arg(short, long, default_value = "text")]
        format: String,
        /// Dialect: `identifier` (default), `agent-doc`, or `auto`
        #[arg(long, default_value = "auto")]
        dialect: String,
        /// Enable filesystem-dependent checks (agent-doc dialect only)
        #[arg(long)]
        fs_checks: bool,
        /// Restrict to specific rule IDs (repeatable; agent-doc dialect only)
        #[arg(long = "rule")]
        rules: Vec<String>,
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
        /// Read from the persisted .naming/index.json instead of rescanning
        #[arg(long)]
        index: bool,
    },
    /// Rename an indexed tag family across files and naming conventions
    Rename {
        /// Existing identifier or family member to rename
        old: String,
        /// Replacement identifier; its tags are rendered per source convention
        new: String,
        /// Project path (defaults to current directory)
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Print the rename plan without writing source files or the index
        #[arg(long)]
        dry_run: bool,
        /// Output format (text, json)
        #[arg(short, long, default_value = "text")]
        format: String,
    },
    /// Build, check, or rebuild the persistent project index (.naming/index.json)
    Index {
        /// Project path (defaults to current directory)
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Recompute fingerprint/hashes and exit 0 if fresh, 1 if stale. Does not write.
        #[arg(long)]
        check: bool,
        /// Rebuild even if the on-disk index is still fresh
        #[arg(long)]
        force: bool,
        /// Output transport: `json` (default, writes .naming/index.json) or
        /// `jsonl` (stream NDJSON to stdout; no on-disk write).
        #[arg(long, default_value = "json")]
        emit: String,
        /// Print the integer schema version and exit 0. No other output.
        #[arg(long)]
        schema_version: bool,
        /// Incrementally update the on-disk index, re-extracting only files
        /// whose content actually changed. Falls back to a full rebuild on
        /// any schema or config-fingerprint mismatch.
        #[arg(long)]
        update: bool,
        /// With `--update`, force a full rebuild but keep the update digest
        /// summary format. Ignored when `--update` is not set.
        #[arg(long)]
        force_full: bool,
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
    /// Start the MCP (Model Context Protocol) stdio server, or run an
    /// installer/uninstaller for a known harness config file.
    ///
    /// `tagpath mcp` (no subcommand) starts the stdio server.
    /// `tagpath mcp install ...` generates / writes / removes harness configs.
    Mcp {
        #[command(subcommand)]
        action: Option<McpAction>,
    },
    /// Manage runtime-loaded tree-sitter grammars (requires the `dyn-grammar` feature)
    Grammars {
        #[command(subcommand)]
        action: GrammarsAction,
    },
    /// Watch a project for filesystem changes and emit NDJSON events on stdout
    Watch {
        /// Project path (defaults to current directory)
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Perform a single reindex + lint pass, emit the events, exit.
        #[arg(long)]
        once: bool,
        /// Skip the agent-doc lint pass on changed `.md` files.
        #[arg(long = "no-lint")]
        no_lint: bool,
        /// Output shape: `full` (default) or `compact`.
        #[arg(long = "emit-shape", default_value = "full")]
        emit_shape: String,
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

#[derive(Subcommand)]
enum McpAction {
    /// Start the stdio JSON-RPC server (default when `tagpath mcp` is run with no subcommand).
    Serve,
    /// Generate or apply MCP server config blocks for known harnesses.
    Install(McpInstallArgs),
}

#[derive(clap::Args)]
struct McpInstallArgs {
    /// Print the harness config to stdout (no file writes).
    #[arg(long, value_name = "HARNESS", group = "install_mode")]
    print: Option<String>,
    /// Apply the harness config (writes file with --yes; otherwise dry-run).
    #[arg(long, value_name = "HARNESS", group = "install_mode")]
    apply: Option<String>,
    /// Remove the `tagpath` entry from a harness config.
    #[arg(long, value_name = "HARNESS", group = "install_mode")]
    uninstall: Option<String>,
    /// List known harnesses and their default config paths.
    #[arg(long, group = "install_mode")]
    list: bool,
    /// Override the resolved `command` binary path (default: "tagpath").
    #[arg(long, value_name = "PATH")]
    binary: Option<String>,
    /// Write to <project>/.claude or <project>/.cursor instead of the user-level config.
    #[arg(long, value_name = "PATH")]
    project: Option<PathBuf>,
    /// Override the resolved config base directory (tests + advanced).
    #[arg(long, value_name = "DIR")]
    config_dir_override: Option<PathBuf>,
    /// Confirm a write for --apply / --uninstall. Without --yes, those are dry-runs.
    #[arg(long)]
    yes: bool,
}

#[derive(Subcommand)]
enum GrammarsAction {
    /// List configured and discovered dynamic tree-sitter grammars
    List {
        /// Project path (defaults to current directory)
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Output format (text, json)
        #[arg(short, long, default_value = "text")]
        format: String,
    },
    /// Verify every configured grammar loads (exits 1 on any failure)
    Check {
        /// Project path (defaults to current directory)
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
        Commands::Init { lang, preset } => cmd_init(lang.as_deref(), preset.as_deref()),
        Commands::Lint {
            path,
            format,
            dialect,
            fs_checks,
            rules,
        } => cmd_lint(&path, &format, &dialect, fs_checks, &rules),
        Commands::Extract { path, format, ast } => cmd_extract(&path, &format, ast),
        Commands::Search {
            query,
            path,
            format,
            index,
        } => cmd_search(&query, &path, &format, index),
        Commands::Rename {
            old,
            new,
            path,
            dry_run,
            format,
        } => cmd_rename(&old, &new, &path, dry_run, &format),
        Commands::Index {
            path,
            check,
            force,
            emit,
            schema_version,
            update,
            force_full,
        } => cmd_index(
            &path,
            check,
            force,
            &emit,
            schema_version,
            update,
            force_full,
        ),
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
        Commands::Mcp { action } => cmd_mcp_dispatch(action),
        Commands::Grammars { action } => cmd_grammars(action),
        Commands::Watch {
            path,
            once,
            no_lint,
            emit_shape,
        } => cmd_watch(&path, once, no_lint, &emit_shape),
    }
}

#[cfg(feature = "watch")]
fn cmd_watch(path: &std::path::Path, once: bool, no_lint: bool, emit_shape: &str) {
    use tagpath::watch::{self, EmitShape, WatchMode, WatchOptions};
    let project_root = match index::find_project_root(path) {
        Some(root) => root,
        None => {
            eprintln!(
                "error: no .naming.toml found (searched from {} upward); run `tagpath init`",
                path.display()
            );
            std::process::exit(1);
        }
    };
    let shape = match EmitShape::parse(emit_shape) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(2);
        }
    };
    let mut opts = WatchOptions::new(project_root);
    opts.mode = if once {
        WatchMode::Once
    } else {
        WatchMode::Continuous
    };
    opts.run_lint = !no_lint;
    opts.emit_shape = shape;
    opts.quiet = std::env::var_os("TAGPATH_QUIET").is_some();
    let code = watch::run(opts);
    std::process::exit(code);
}

#[cfg(not(feature = "watch"))]
fn cmd_watch(_path: &std::path::Path, _once: bool, _no_lint: bool, _emit_shape: &str) {
    eprintln!("error: tagpath was built without the `watch` feature");
    eprintln!("hint: rebuild with `cargo build --features watch`");
    std::process::exit(1);
}

fn cmd_mcp_dispatch(action: Option<McpAction>) {
    match action {
        None | Some(McpAction::Serve) => cmd_mcp(),
        Some(McpAction::Install(args)) => cmd_mcp_install(args),
    }
}

#[cfg(feature = "mcp")]
fn cmd_mcp() {
    if let Err(error) = tagpath::mcp::run() {
        eprintln!("error: mcp server: {error}");
        std::process::exit(1);
    }
}

#[cfg(not(feature = "mcp"))]
fn cmd_mcp() {
    eprintln!("error: tagpath was built without the `mcp` feature");
    eprintln!("hint: rebuild with `cargo build --features mcp` or use a release that includes it");
    std::process::exit(1);
}

#[cfg(feature = "mcp")]
fn cmd_mcp_install(args: McpInstallArgs) {
    use tagpath::mcp::install::{
        self, InstallOpts, InstallScope, apply_config, list_default_paths, render_config,
        uninstall_config,
    };

    let scope = if args.project.is_some() {
        InstallScope::Project
    } else {
        InstallScope::User
    };
    let opts = InstallOpts {
        binary: args.binary.unwrap_or_else(|| "tagpath".to_string()),
        scope,
        project_path: args.project.clone(),
        config_dir_override: args.config_dir_override.clone(),
    };

    if args.list {
        let entries = list_default_paths(&opts);
        for (name, path) in entries {
            match path {
                Some(p) => println!("{name}\t{}", p.display()),
                None => println!("{name}\t(no default on this platform/scope)"),
            }
        }
        return;
    }

    if let Some(harness) = args.print.as_deref() {
        match render_config(harness, &opts) {
            Ok(text) => {
                if !text.ends_with('\n') {
                    println!("{text}");
                } else {
                    print!("{text}");
                }
            }
            Err(e) => {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
        return;
    }

    if let Some(harness) = args.apply.as_deref() {
        let dry = !args.yes;
        match apply_config(harness, &opts, dry) {
            Ok(outcome) => {
                if dry {
                    eprintln!("dry-run: would write {}", outcome.path.display());
                    eprintln!("(pass --yes to apply)");
                    print!("{}", outcome.preview);
                } else {
                    eprintln!("wrote {}", outcome.path.display());
                }
            }
            Err(e) => {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
        return;
    }

    if let Some(harness) = args.uninstall.as_deref() {
        let dry = !args.yes;
        match uninstall_config(harness, &opts, dry) {
            Ok(outcome) => {
                if !outcome.preview.is_empty() && !outcome.written {
                    if dry {
                        eprintln!("dry-run: would update {}", outcome.path.display());
                        eprintln!("(pass --yes to apply)");
                        print!("{}", outcome.preview);
                    } else {
                        eprintln!(
                            "no tagpath entry found in {}; nothing to do",
                            outcome.path.display()
                        );
                    }
                } else if outcome.written {
                    eprintln!("removed tagpath entry from {}", outcome.path.display());
                } else {
                    eprintln!(
                        "no tagpath entry found in {}; nothing to do",
                        outcome.path.display()
                    );
                }
            }
            Err(e) => {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
        return;
    }

    // No mode flag set — print usage hint.
    let _ = install::HARNESSES; // ensure the symbol is referenced
    eprintln!("error: tagpath mcp install requires one of --print, --apply, --uninstall, --list");
    eprintln!("hint: try `tagpath mcp install --list` to see known harnesses");
    std::process::exit(2);
}

#[cfg(not(feature = "mcp"))]
fn cmd_mcp_install(_args: McpInstallArgs) {
    eprintln!("error: tagpath was built without the `mcp` feature");
    eprintln!("hint: rebuild with `cargo build --features mcp`");
    std::process::exit(1);
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

fn cmd_lint(
    path: &std::path::Path,
    format: &str,
    dialect: &str,
    fs_checks: bool,
    rules: &[String],
) {
    let run_agent_doc = matches!(dialect, "agent-doc" | "agentdoc");
    let run_identifier = matches!(dialect, "identifier" | "ident");
    let run_auto = matches!(dialect, "auto" | "");

    if run_agent_doc {
        let code = run_agent_doc_lint(path, format, fs_checks, rules);
        std::process::exit(code);
    }

    if run_auto {
        // Auto-detect agent-doc files (single file fast path); for
        // directories we route to identifier lint and rely on per-file
        // detection inside the markdown walker.
        if path.is_file() {
            if let Ok(text) = std::fs::read_to_string(path)
                && lint::looks_like_agent_doc(&text)
            {
                let code = run_agent_doc_lint(path, format, fs_checks, rules);
                std::process::exit(code);
            }
        } else if path.is_dir() {
            // Walk and collect markdown files for agent-doc lint, then
            // continue with identifier lint on the same root.
            let code_md = run_agent_doc_lint_dir(path, format, fs_checks, rules);
            // If the dir contains no .naming.toml, the identifier pass
            // would error out — only run it when a config can be found.
            if lint::find_config(path).is_none() {
                std::process::exit(code_md);
            }
            let code_id = run_identifier_lint(path, format);
            std::process::exit(code_md.max(code_id));
        }
    }

    if run_identifier || run_auto {
        let code = run_identifier_lint(path, format);
        std::process::exit(code);
    }

    eprintln!("error: unknown dialect `{dialect}`");
    eprintln!("hint: use `identifier`, `agent-doc`, or `auto`");
    std::process::exit(2);
}

fn run_identifier_lint(path: &std::path::Path, format: &str) -> i32 {
    // Find .naming.toml by walking up from the target path
    let config_path = match lint::find_config(path) {
        Some(p) => p,
        None => {
            eprintln!(
                "error: no .naming.toml found (searched from {} upward)",
                path.display()
            );
            eprintln!("hint: run `tagpath init` to create one");
            return 2;
        }
    };
    let naming_config = match config::resolve(&config_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {e}");
            return 2;
        }
    };
    let violations = lint::lint(path, &naming_config);
    if violations.is_empty() {
        if format != "json" {
            println!("No naming convention violations found.");
        } else {
            println!("[]");
        }
        return 0;
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
    1
}

fn run_agent_doc_lint(
    path: &std::path::Path,
    format: &str,
    fs_checks: bool,
    rules: &[String],
) -> i32 {
    let opts = lint::AgentDocOptions {
        fs_checks,
        rule_filter: rules.to_vec(),
    };
    let mut all = Vec::new();
    if path.is_file() {
        match std::fs::read_to_string(path) {
            Ok(text) => {
                all.extend(lint::lint_agent_doc(path, &text, &opts));
            }
            Err(e) => {
                eprintln!("error: read {}: {e}", path.display());
                return 2;
            }
        }
    } else if path.is_dir() {
        for entry in walkdir::WalkDir::new(path) {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    eprintln!("error: walk: {e}");
                    return 2;
                }
            };
            if !entry.file_type().is_file() {
                continue;
            }
            let p = entry.path();
            if p.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            if let Ok(text) = std::fs::read_to_string(p) {
                if !lint::looks_like_agent_doc(&text) {
                    continue;
                }
                all.extend(lint::lint_agent_doc(p, &text, &opts));
            }
        }
    } else {
        eprintln!("error: path does not exist: {}", path.display());
        return 2;
    }
    emit_agent_doc_findings(format, &all);
    if all
        .iter()
        .any(|f| matches!(f.severity, lint::LintSeverity::Error))
    {
        1
    } else {
        0
    }
}

fn run_agent_doc_lint_dir(
    path: &std::path::Path,
    format: &str,
    fs_checks: bool,
    rules: &[String],
) -> i32 {
    run_agent_doc_lint(path, format, fs_checks, rules)
}

fn emit_agent_doc_findings(format: &str, findings: &[lint::LintFinding]) {
    match format {
        "json" => {
            println!("{}", serde_json::to_string_pretty(findings).unwrap());
        }
        _ => {
            if findings.is_empty() {
                println!("No agent-doc tag violations found.");
                return;
            }
            print!("{}", lint::format_findings_text(findings));
            let errors = findings
                .iter()
                .filter(|f| matches!(f.severity, lint::LintSeverity::Error))
                .count();
            let warnings = findings.len() - errors;
            eprintln!(
                "\nFound {} error(s), {} warning(s) in agent-doc tags.",
                errors, warnings
            );
        }
    }
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

fn cmd_search(query: &str, path: &std::path::Path, format: &str, use_index: bool) {
    if use_index {
        cmd_search_index(query, path, format);
        return;
    }
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

fn cmd_search_index(query: &str, path: &std::path::Path, format: &str) {
    let project_root = match index::find_project_root(path) {
        Some(root) => root,
        None => {
            eprintln!(
                "error: no .naming.toml found (searched from {} upward); run `tagpath init`",
                path.display()
            );
            std::process::exit(2);
        }
    };
    let idx_path = index::index_path(&project_root);
    if !idx_path.exists() {
        eprintln!(
            "error: no index found at {}; run `tagpath index` first",
            idx_path.display()
        );
        std::process::exit(2);
    }
    let report = match index::check(&project_root) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(2);
        }
    };
    if !report.fresh {
        eprintln!("error: index is stale; run `tagpath index` to rebuild");
        for reason in &report.stale_reasons {
            eprintln!("  - {}", format_stale_reason(reason));
        }
        std::process::exit(2);
    }
    let idx = match index::read(&idx_path) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(2);
        }
    };
    let hits = index::search_index(&idx, query);
    match format {
        "json" => {
            println!("{}", serde_json::to_string_pretty(&hits).unwrap());
        }
        _ => {
            for hit in &hits {
                println!(
                    "{}:{}\t{}\t{}",
                    hit.path, hit.line, hit.name, hit.convention,
                );
            }
        }
    }
}

fn cmd_rename(old: &str, new: &str, path: &std::path::Path, dry_run: bool, format: &str) {
    let report = match rename_mod::rename_family(&rename_mod::RenameOptions {
        path: path.to_path_buf(),
        old: old.to_string(),
        new: new.to_string(),
        dry_run,
    }) {
        Ok(report) => report,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    };

    match format {
        "json" => println!("{}", serde_json::to_string_pretty(&report).unwrap()),
        "text" => {
            for edit in &report.edits {
                println!(
                    "{}:\t{} -> {}\t{} replacements",
                    edit.path, edit.old, edit.new, edit.replacements
                );
            }
            let action = if report.dry_run {
                "would rename"
            } else {
                "renamed"
            };
            println!(
                "{action} {} occurrences across {} files ({} -> {})",
                report.replacements, report.files_changed, report.old, report.new
            );
        }
        other => {
            eprintln!("error: unknown --format value `{other}` (expected `text` or `json`)");
            std::process::exit(2);
        }
    }
}

fn cmd_index(
    path: &std::path::Path,
    check: bool,
    force: bool,
    emit: &str,
    schema_version: bool,
    update: bool,
    force_full: bool,
) {
    if schema_version {
        println!("{}", index::SCHEMA_VERSION);
        return;
    }
    let emit_jsonl = match emit {
        "json" => false,
        "jsonl" => true,
        other => {
            eprintln!("error: unknown --emit value `{other}` (expected `json` or `jsonl`)");
            std::process::exit(2);
        }
    };
    let project_root = match index::find_project_root(path) {
        Some(root) => root,
        None => {
            eprintln!(
                "error: no .naming.toml found (searched from {} upward); run `tagpath init`",
                path.display()
            );
            std::process::exit(1);
        }
    };
    let idx_path = index::index_path(&project_root);
    if update {
        cmd_index_update(&project_root, &idx_path, emit_jsonl, force_full);
        return;
    }
    if check {
        let report = match index::check(&project_root) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        };
        if emit_jsonl {
            let stdout = std::io::stdout();
            let mut out = stdout.lock();
            if let Err(e) = index::emit_jsonl_stale(&project_root, &report, &mut out) {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
            if !report.fresh {
                std::process::exit(1);
            }
            return;
        }
        if report.fresh {
            println!("index is fresh: {}", idx_path.display());
            return;
        }
        eprintln!("index is stale: {}", idx_path.display());
        for reason in &report.stale_reasons {
            eprintln!("  - {}", format_stale_reason(reason));
        }
        std::process::exit(1);
    }
    if !force && !emit_jsonl && idx_path.exists() {
        match index::check(&project_root) {
            Ok(r) if r.fresh => {
                println!("index is already fresh: {}", idx_path.display());
                return;
            }
            Ok(_) => {}
            Err(e) => {
                eprintln!("warning: check failed, rebuilding anyway: {e}");
            }
        }
    }
    let opts = index::BuildOptions {
        project_root: project_root.clone(),
    };
    let idx = match index::build(&opts) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    };
    if emit_jsonl {
        let stdout = std::io::stdout();
        let mut out = stdout.lock();
        if let Err(e) = index::emit_jsonl(&idx, &mut out) {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
        return;
    }
    if let Err(e) = index::write(&idx, &idx_path) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
    println!(
        "wrote {} ({} sources, {} families, {} ontology refs)",
        idx_path.display(),
        idx.sources.len(),
        idx.families.len(),
        idx.ontology_refs.len(),
    );
}

fn cmd_index_update(
    project_root: &std::path::Path,
    idx_path: &std::path::Path,
    emit_jsonl: bool,
    force_full: bool,
) {
    let quiet = std::env::var_os("TAGPATH_QUIET").is_some();
    let opts = index::BuildOptions {
        project_root: project_root.to_path_buf(),
    };

    let result = if force_full {
        let started = std::time::Instant::now();
        let idx = match index::build(&opts) {
            Ok(i) => i,
            Err(e) => {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        };
        let elapsed_ms = started.elapsed().as_millis();
        let summary = index::UpdateSummary {
            changed: 0,
            added: idx.sources.len(),
            removed: 0,
            unchanged: 0,
            elapsed_ms,
            fallback: None,
        };
        index::UpdateResult {
            index: idx,
            summary,
        }
    } else {
        // Bare `tagpath index --update` only consults the summary
        // (digest + skip-write decision); the returned `Index` is
        // discarded on no-op. Hand `LazyTail` to skip the bincode
        // decode of ~15k families on a true no-op cycle. The NDJSON
        // path below still needs the full Index, so emit_jsonl forces
        // the Full shape.
        let shape = if emit_jsonl {
            index::IndexShape::Full
        } else {
            index::IndexShape::LazyTail
        };
        match index::update_incremental_with(&opts, shape) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
    };

    if let Some(reason) = &result.summary.fallback
        && !quiet
    {
        eprintln!(
            "[tagpath] incremental update falling back to full rebuild: {}",
            reason.as_str()
        );
    }

    if emit_jsonl {
        let stdout = std::io::stdout();
        let mut out = stdout.lock();
        let plan = result.summary.plan_line();
        if let Err(e) = writeln!(out, "{}", serde_json::to_string(&plan).unwrap()) {
            eprintln!("error: write update_plan: {e}");
            std::process::exit(1);
        }
        if let Err(e) = index::emit_jsonl(&result.index, &mut out) {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
        return;
    }

    // Skip the JSON re-serialize + atomic rename on a true no-op update:
    // every classification was Unchanged or MtimeOnly, nothing added, nothing
    // removed → the on-disk file is already byte-equivalent (modulo
    // `generated_at`). This is the hot path for `tagpath index --update`
    // polled by tsift/agent-doc and avoids ~50ms of serialization on a
    // 1000-file repo.
    if !result.summary.is_noop()
        && let Err(e) = index::write(&result.index, idx_path)
    {
        eprintln!("error: {e}");
        std::process::exit(1);
    }

    if !quiet {
        if force_full || result.summary.fallback.is_some() {
            eprintln!(
                "{}",
                index::format_full_rebuild_digest(&result.index, result.summary.elapsed_ms)
            );
        } else {
            eprintln!("{}", index::format_update_digest(&result.summary));
        }
    }
}

fn format_stale_reason(reason: &index::StaleReason) -> String {
    use index::StaleReason::*;
    match reason {
        IndexMissing => "index missing".to_string(),
        IndexUnreadable { message } => format!("index unreadable: {message}"),
        SchemaVersion { found, expected } => {
            format!("schema_version mismatch: found {found}, expected {expected}")
        }
        SchemaChanged { found, expected } => {
            format!(
                "schema_version migration: found {found}, expected {expected} (rebuild silently)"
            )
        }
        ConfigChanged => "config_fingerprint changed (.naming.toml or extends)".to_string(),
        ToolVersion { found, expected } => {
            format!("tool_version mismatch: found {found}, expected {expected}")
        }
        SourceAdded { path } => format!("source added: {path}"),
        SourceRemoved { path } => format!("source removed: {path}"),
        SourceModified { path } => format!("source modified: {path}"),
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

// ── Grammars subcommand ─────────────────────────────────────────────

fn cmd_grammars(action: GrammarsAction) {
    match action {
        GrammarsAction::List { path, format } => cmd_grammars_list(&path, &format),
        GrammarsAction::Check { path } => cmd_grammars_check(&path),
    }
}

#[cfg(feature = "dyn-grammar")]
fn cmd_grammars_list(path: &std::path::Path, format: &str) {
    use tagpath::treesitter::dyn_loader;
    let (cfg, base_dir) = match load_grammars_config(path) {
        Ok((Some(c), d)) => (c, d),
        Ok((None, _)) => {
            println!("no [grammars] section found in .naming.toml");
            return;
        }
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    };
    // Resolve load_dirs (~ expansion + relative resolution).
    let resolved_dirs: Vec<std::path::PathBuf> = cfg
        .load_dirs
        .iter()
        .map(|d| config::expand_grammar_path(d, &base_dir))
        .collect();
    let discovered = dyn_loader::discover(&resolved_dirs);
    let loaded = dyn_loader::load_configured_all(&cfg, &base_dir);
    if format == "json" {
        let mut entries = Vec::new();
        for (lang, result) in &loaded {
            let mut obj = serde_json::Map::new();
            obj.insert(
                "language".to_string(),
                serde_json::Value::String(lang.clone()),
            );
            obj.insert(
                "source".to_string(),
                serde_json::Value::String("configured".to_string()),
            );
            match result {
                Ok(g) => {
                    obj.insert(
                        "path".to_string(),
                        serde_json::Value::String(g.source_path.display().to_string()),
                    );
                    obj.insert(
                        "abi_version".to_string(),
                        serde_json::Value::Number(serde_json::Number::from(g.abi_version)),
                    );
                    obj.insert(
                        "extensions".to_string(),
                        serde_json::Value::Array(
                            g.extensions
                                .iter()
                                .map(|e| serde_json::Value::String(e.clone()))
                                .collect(),
                        ),
                    );
                    obj.insert(
                        "status".to_string(),
                        serde_json::Value::String("ok".to_string()),
                    );
                }
                Err(e) => {
                    obj.insert(
                        "path".to_string(),
                        serde_json::Value::String(e.path().display().to_string()),
                    );
                    obj.insert(
                        "status".to_string(),
                        serde_json::Value::String("error".to_string()),
                    );
                    obj.insert(
                        "error".to_string(),
                        serde_json::Value::String(e.to_string()),
                    );
                }
            }
            entries.push(serde_json::Value::Object(obj));
        }
        for d in &discovered {
            let mut obj = serde_json::Map::new();
            obj.insert(
                "language".to_string(),
                serde_json::Value::String(d.language.clone()),
            );
            obj.insert(
                "source".to_string(),
                serde_json::Value::String("discovered".to_string()),
            );
            obj.insert(
                "path".to_string(),
                serde_json::Value::String(d.path.display().to_string()),
            );
            obj.insert(
                "symbol".to_string(),
                serde_json::Value::String(d.symbol.clone()),
            );
            entries.push(serde_json::Value::Object(obj));
        }
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::Value::Array(entries)).unwrap()
        );
        return;
    }
    if loaded.is_empty() && discovered.is_empty() {
        println!("no grammars configured or discovered");
        return;
    }
    if !loaded.is_empty() {
        println!("Configured grammars:");
        for (lang, result) in &loaded {
            match result {
                Ok(g) => println!(
                    "  ok       {lang:<16} abi=v{:<3} ext=[{}] path={}",
                    g.abi_version,
                    g.extensions.join(","),
                    g.source_path.display()
                ),
                Err(e) => println!("  error    {lang:<16} {e}"),
            }
        }
    }
    if !discovered.is_empty() {
        println!("Discovered grammars:");
        for d in &discovered {
            println!(
                "  found    {:<16} symbol={} path={}",
                d.language,
                d.symbol,
                d.path.display()
            );
        }
    }
}

#[cfg(feature = "dyn-grammar")]
fn cmd_grammars_check(path: &std::path::Path) {
    use tagpath::treesitter::dyn_loader;
    let (cfg, base_dir) = match load_grammars_config(path) {
        Ok((Some(c), d)) => (c, d),
        Ok((None, _)) => {
            println!("no [grammars] section found in .naming.toml");
            return;
        }
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    };
    let results = dyn_loader::load_configured_all(&cfg, &base_dir);
    let mut failed = 0usize;
    let mut succeeded = 0usize;
    for (lang, result) in &results {
        match result {
            Ok(g) => {
                println!(
                    "ok       {lang:<16} abi=v{} path={}",
                    g.abi_version,
                    g.source_path.display()
                );
                succeeded += 1;
            }
            Err(e) => {
                eprintln!("error    {lang:<16} {e}");
                failed += 1;
            }
        }
    }
    eprintln!("\n{succeeded} ok, {failed} failed");
    if failed > 0 {
        std::process::exit(1);
    }
}

/// Locate `.naming.toml` from `path` and return `(grammars_config, base_dir)`.
#[cfg(feature = "dyn-grammar")]
fn load_grammars_config(
    path: &std::path::Path,
) -> Result<(Option<config::GrammarsConfig>, std::path::PathBuf), String> {
    let config_path = lint::find_config(path).ok_or_else(|| {
        format!(
            "no .naming.toml found (searched from {} upward); run `tagpath init` to create one",
            path.display()
        )
    })?;
    let resolved = config::resolve(&config_path)?;
    let base_dir = config_path
        .parent()
        .ok_or_else(|| "config path has no parent directory".to_string())?
        .to_path_buf();
    Ok((resolved.grammars, base_dir))
}

#[cfg(not(feature = "dyn-grammar"))]
fn cmd_grammars_list(_path: &std::path::Path, _format: &str) {
    eprintln!("error: tagpath was built without the `dyn-grammar` feature");
    eprintln!("hint: rebuild with `cargo install --features dyn-grammar tagpath`");
    std::process::exit(1);
}

#[cfg(not(feature = "dyn-grammar"))]
fn cmd_grammars_check(_path: &std::path::Path) {
    eprintln!("error: tagpath was built without the `dyn-grammar` feature");
    eprintln!("hint: rebuild with `cargo install --features dyn-grammar tagpath`");
    std::process::exit(1);
}
