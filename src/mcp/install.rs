//! `tagpath mcp install` — generate and (optionally) write MCP server config
//! blocks for the five major harnesses Claude Desktop, Claude Code, Codex,
//! OpenCode, and Cursor.
//!
//! The installer is intentionally narrow: it only touches the `tagpath`
//! entry inside the harness's `mcpServers` (or equivalent) map. Other
//! servers and unrelated keys are preserved by deep-merging into an
//! existing config when present.
//!
//! Three entrypoints:
//!
//! - [`render_config`] — return the JSON or TOML text for a harness.
//! - [`apply_config`] — merge the entry into the on-disk config (atomic write).
//! - [`uninstall_config`] — remove the `tagpath` entry from the on-disk config.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

/// Known harness identifiers. Keep these alphabetically sorted for stable
/// `--list` output.
pub const HARNESSES: &[&str] = &[
    "claude-code",
    "claude-desktop",
    "codex",
    "cursor",
    "opencode",
];

/// Scope of the install — user-global vs project-local. Only Claude Code
/// and Cursor honor `Project`; the rest fall back to user-global.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallScope {
    User,
    Project,
}

/// Serialization format of the harness config file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigFormat {
    Json,
    Toml,
}

/// Inputs to install / render. `binary` overrides the executable path
/// (default `"tagpath"`, i.e. resolved via PATH).
#[derive(Debug, Clone)]
pub struct InstallOpts {
    pub binary: String,
    pub scope: InstallScope,
    /// Override for the project root when `scope = Project`.
    pub project_path: Option<PathBuf>,
    /// Override for the user config base directory (tests + advanced
    /// users). When set, this replaces the resolved home/config directory
    /// for the harness path lookup.
    pub config_dir_override: Option<PathBuf>,
}

impl Default for InstallOpts {
    fn default() -> Self {
        Self {
            binary: "tagpath".to_string(),
            scope: InstallScope::User,
            project_path: None,
            config_dir_override: None,
        }
    }
}

/// Errors surfaced by the installer.
#[derive(Debug)]
pub enum InstallError {
    UnknownHarness(String),
    NoConfigPath {
        harness: &'static str,
        reason: &'static str,
    },
    Io {
        path: PathBuf,
        message: String,
    },
    Parse {
        path: PathBuf,
        message: String,
    },
    Serialize(String),
}

impl std::fmt::Display for InstallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InstallError::UnknownHarness(name) => {
                write!(
                    f,
                    "unknown harness `{name}` (expected one of: {})",
                    HARNESSES.join(", ")
                )
            }
            InstallError::NoConfigPath { harness, reason } => {
                write!(f, "cannot resolve {harness} config path: {reason}")
            }
            InstallError::Io { path, message } => {
                write!(f, "io error at {}: {message}", path.display())
            }
            InstallError::Parse { path, message } => {
                write!(f, "parse error at {}: {message}", path.display())
            }
            InstallError::Serialize(message) => write!(f, "serialize error: {message}"),
        }
    }
}

impl std::error::Error for InstallError {}

/// Look up a harness descriptor by name. Returns `None` if the name does
/// not match a known harness.
pub fn resolve_harness(name: &str) -> Option<HarnessDescriptor> {
    match name {
        "claude-desktop" => Some(HarnessDescriptor::claude_desktop()),
        "claude-code" => Some(HarnessDescriptor::claude_code()),
        "codex" => Some(HarnessDescriptor::codex()),
        "opencode" => Some(HarnessDescriptor::opencode()),
        "cursor" => Some(HarnessDescriptor::cursor()),
        _ => None,
    }
}

/// Render the harness config block as text suitable for stdout / paste.
///
/// For JSON harnesses, the returned text is the full `{ ... }` block —
/// merged into the existing config if `--print` was paired with a file
/// hint, but typically callers pass a fresh `InstallOpts` and get the
/// minimal block back.
pub fn render_config(harness: &str, opts: &InstallOpts) -> Result<String, InstallError> {
    let desc = resolve_harness(harness)
        .ok_or_else(|| InstallError::UnknownHarness(harness.to_string()))?;
    match desc.format {
        ConfigFormat::Json => {
            let mut root = Map::new();
            desc.merge_into_json(&mut root, opts);
            let value = Value::Object(root);
            serde_json::to_string_pretty(&value).map_err(|e| InstallError::Serialize(e.to_string()))
        }
        ConfigFormat::Toml => {
            let mut doc = toml::Table::new();
            desc.merge_into_toml(&mut doc, opts);
            toml::to_string_pretty(&doc).map_err(|e| InstallError::Serialize(e.to_string()))
        }
    }
}

/// Result of an apply / uninstall operation.
#[derive(Debug, Clone)]
pub struct ApplyOutcome {
    pub path: PathBuf,
    pub written: bool,
    pub preview: String,
}

/// Resolve the config path for a harness given the install options. Returns
/// `Err` when no platform default is known.
pub fn config_path(harness: &str, opts: &InstallOpts) -> Result<PathBuf, InstallError> {
    let desc = resolve_harness(harness)
        .ok_or_else(|| InstallError::UnknownHarness(harness.to_string()))?;
    desc.resolve_path(opts).ok_or(InstallError::NoConfigPath {
        harness: desc.name,
        reason: match opts.scope {
            InstallScope::User => "no home/config directory found on this platform",
            InstallScope::Project => "harness does not support project-scoped config",
        },
    })
}

/// Apply the harness config: read existing file (if present), deep-merge the
/// `tagpath` entry into the canonical map, atomically write back. When `dry`
/// is true, the file is NOT written; only the resolved path and the merged
/// preview are returned.
pub fn apply_config(
    harness: &str,
    opts: &InstallOpts,
    dry: bool,
) -> Result<ApplyOutcome, InstallError> {
    let desc = resolve_harness(harness)
        .ok_or_else(|| InstallError::UnknownHarness(harness.to_string()))?;
    let path = config_path(harness, opts)?;
    let preview = merge_and_serialize(&desc, &path, opts)?;
    if dry {
        return Ok(ApplyOutcome {
            path,
            written: false,
            preview,
        });
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| InstallError::Io {
            path: parent.to_path_buf(),
            message: e.to_string(),
        })?;
    }
    atomic_write(&path, &preview)?;
    Ok(ApplyOutcome {
        path,
        written: true,
        preview,
    })
}

/// Remove the `tagpath` entry from a harness config. Idempotent — if the
/// entry is absent (or the file does not exist), `written` is false.
pub fn uninstall_config(
    harness: &str,
    opts: &InstallOpts,
    dry: bool,
) -> Result<ApplyOutcome, InstallError> {
    let desc = resolve_harness(harness)
        .ok_or_else(|| InstallError::UnknownHarness(harness.to_string()))?;
    let path = config_path(harness, opts)?;
    if !path.exists() {
        return Ok(ApplyOutcome {
            path,
            written: false,
            preview: String::new(),
        });
    }
    let (changed, preview) = remove_and_serialize(&desc, &path)?;
    if !changed || dry {
        return Ok(ApplyOutcome {
            path,
            written: false,
            preview,
        });
    }
    atomic_write(&path, &preview)?;
    Ok(ApplyOutcome {
        path,
        written: true,
        preview,
    })
}

/// Public listing helper used by `tagpath mcp install --list`.
pub fn list_default_paths(opts: &InstallOpts) -> Vec<(&'static str, Option<PathBuf>)> {
    HARNESSES
        .iter()
        .map(|name| {
            let desc = resolve_harness(name).expect("known harness");
            (desc.name, desc.resolve_path(opts))
        })
        .collect()
}

/// Read + merge + serialize the harness config without writing.
fn merge_and_serialize(
    desc: &HarnessDescriptor,
    path: &Path,
    opts: &InstallOpts,
) -> Result<String, InstallError> {
    match desc.format {
        ConfigFormat::Json => {
            let mut root = read_json(path)?;
            desc.merge_into_json(&mut root, opts);
            let value = Value::Object(root);
            let mut text = serde_json::to_string_pretty(&value)
                .map_err(|e| InstallError::Serialize(e.to_string()))?;
            text.push('\n');
            Ok(text)
        }
        ConfigFormat::Toml => {
            let mut doc = read_toml(path)?;
            desc.merge_into_toml(&mut doc, opts);
            let text =
                toml::to_string_pretty(&doc).map_err(|e| InstallError::Serialize(e.to_string()))?;
            Ok(text)
        }
    }
}

/// Remove the `tagpath` entry and re-serialize. Returns `(changed, text)`.
fn remove_and_serialize(
    desc: &HarnessDescriptor,
    path: &Path,
) -> Result<(bool, String), InstallError> {
    match desc.format {
        ConfigFormat::Json => {
            let mut root = read_json(path)?;
            let changed = desc.remove_from_json(&mut root);
            let value = Value::Object(root);
            let mut text = serde_json::to_string_pretty(&value)
                .map_err(|e| InstallError::Serialize(e.to_string()))?;
            text.push('\n');
            Ok((changed, text))
        }
        ConfigFormat::Toml => {
            let mut doc = read_toml(path)?;
            let changed = desc.remove_from_toml(&mut doc);
            let text =
                toml::to_string_pretty(&doc).map_err(|e| InstallError::Serialize(e.to_string()))?;
            Ok((changed, text))
        }
    }
}

fn read_json(path: &Path) -> Result<Map<String, Value>, InstallError> {
    if !path.exists() {
        return Ok(Map::new());
    }
    let text = fs::read_to_string(path).map_err(|e| InstallError::Io {
        path: path.to_path_buf(),
        message: e.to_string(),
    })?;
    if text.trim().is_empty() {
        return Ok(Map::new());
    }
    let value: Value = serde_json::from_str(&text).map_err(|e| InstallError::Parse {
        path: path.to_path_buf(),
        message: e.to_string(),
    })?;
    match value {
        Value::Object(map) => Ok(map),
        _ => Err(InstallError::Parse {
            path: path.to_path_buf(),
            message: "expected a JSON object at the top level".to_string(),
        }),
    }
}

fn read_toml(path: &Path) -> Result<toml::Table, InstallError> {
    if !path.exists() {
        return Ok(toml::Table::new());
    }
    let text = fs::read_to_string(path).map_err(|e| InstallError::Io {
        path: path.to_path_buf(),
        message: e.to_string(),
    })?;
    if text.trim().is_empty() {
        return Ok(toml::Table::new());
    }
    text.parse::<toml::Table>()
        .map_err(|e| InstallError::Parse {
            path: path.to_path_buf(),
            message: e.to_string(),
        })
}

/// Atomic write via `<path>.tmp` then `rename(2)`, mirroring the index
/// writer in `src/index/mod.rs`.
fn atomic_write(path: &Path, contents: &str) -> Result<(), InstallError> {
    let mut tmp = path.as_os_str().to_os_string();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    fs::write(&tmp, contents.as_bytes()).map_err(|e| InstallError::Io {
        path: tmp.clone(),
        message: e.to_string(),
    })?;
    fs::rename(&tmp, path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        InstallError::Io {
            path: path.to_path_buf(),
            message: e.to_string(),
        }
    })
}

// ── Harness descriptors ────────────────────────────────────────────────

/// Per-harness behavior: format, default config path, and how to merge the
/// `tagpath` server entry into the existing config tree.
pub struct HarnessDescriptor {
    pub name: &'static str,
    pub format: ConfigFormat,
    resolver: fn(&InstallOpts) -> Option<PathBuf>,
    merge_json: fn(&mut Map<String, Value>, &InstallOpts),
    remove_json: fn(&mut Map<String, Value>) -> bool,
    merge_toml: fn(&mut toml::Table, &InstallOpts),
    remove_toml: fn(&mut toml::Table) -> bool,
}

impl HarnessDescriptor {
    pub fn resolve_path(&self, opts: &InstallOpts) -> Option<PathBuf> {
        (self.resolver)(opts)
    }

    fn merge_into_json(&self, root: &mut Map<String, Value>, opts: &InstallOpts) {
        (self.merge_json)(root, opts);
    }

    fn remove_from_json(&self, root: &mut Map<String, Value>) -> bool {
        (self.remove_json)(root)
    }

    fn merge_into_toml(&self, doc: &mut toml::Table, opts: &InstallOpts) {
        (self.merge_toml)(doc, opts);
    }

    fn remove_from_toml(&self, doc: &mut toml::Table) -> bool {
        (self.remove_toml)(doc)
    }

    fn claude_desktop() -> Self {
        Self {
            name: "claude-desktop",
            format: ConfigFormat::Json,
            resolver: claude_desktop_path,
            merge_json: merge_mcp_servers_tagpath,
            remove_json: remove_mcp_servers_tagpath,
            merge_toml: merge_toml_unsupported,
            remove_toml: remove_toml_unsupported,
        }
    }

    fn claude_code() -> Self {
        Self {
            name: "claude-code",
            format: ConfigFormat::Json,
            resolver: claude_code_path,
            merge_json: merge_mcp_servers_tagpath,
            remove_json: remove_mcp_servers_tagpath,
            merge_toml: merge_toml_unsupported,
            remove_toml: remove_toml_unsupported,
        }
    }

    fn codex() -> Self {
        Self {
            name: "codex",
            format: ConfigFormat::Toml,
            resolver: codex_path,
            merge_json: merge_json_unsupported,
            remove_json: remove_json_unsupported,
            merge_toml: merge_codex_tagpath,
            remove_toml: remove_codex_tagpath,
        }
    }

    fn opencode() -> Self {
        Self {
            name: "opencode",
            format: ConfigFormat::Json,
            resolver: opencode_path,
            merge_json: merge_opencode_tagpath,
            remove_json: remove_opencode_tagpath,
            merge_toml: merge_toml_unsupported,
            remove_toml: remove_toml_unsupported,
        }
    }

    fn cursor() -> Self {
        Self {
            name: "cursor",
            format: ConfigFormat::Json,
            resolver: cursor_path,
            merge_json: merge_mcp_servers_tagpath,
            remove_json: remove_mcp_servers_tagpath,
            merge_toml: merge_toml_unsupported,
            remove_toml: remove_toml_unsupported,
        }
    }
}

// ── Path resolvers ─────────────────────────────────────────────────────

fn base_config_dir(opts: &InstallOpts) -> Option<PathBuf> {
    if let Some(override_dir) = &opts.config_dir_override {
        return Some(override_dir.clone());
    }
    dirs::config_dir()
}

fn base_home_dir(opts: &InstallOpts) -> Option<PathBuf> {
    if let Some(override_dir) = &opts.config_dir_override {
        return Some(override_dir.clone());
    }
    dirs::home_dir()
}

fn claude_desktop_path(opts: &InstallOpts) -> Option<PathBuf> {
    if opts.scope == InstallScope::Project {
        return None;
    }
    // macOS:   ~/Library/Application Support/Claude/claude_desktop_config.json
    // Windows: %APPDATA%\Claude\claude_desktop_config.json
    // Linux:   ~/.config/Claude/claude_desktop_config.json
    //
    // dirs::config_dir() returns the correct base for all three (it
    // returns "Library/Application Support" on macOS, %APPDATA% on
    // Windows, ~/.config on Linux). When --config-dir-override is set
    // we use that path directly as the base — tests rely on this.
    let base = base_config_dir(opts)?;
    Some(base.join("Claude").join("claude_desktop_config.json"))
}

fn claude_code_path(opts: &InstallOpts) -> Option<PathBuf> {
    if opts.scope == InstallScope::Project {
        let project = opts.project_path.clone()?;
        return Some(project.join(".claude").join("settings.json"));
    }
    let home = base_home_dir(opts)?;
    Some(home.join(".claude").join("settings.json"))
}

fn codex_path(opts: &InstallOpts) -> Option<PathBuf> {
    if opts.scope == InstallScope::Project {
        return None;
    }
    let home = base_home_dir(opts)?;
    Some(home.join(".codex").join("config.toml"))
}

fn opencode_path(opts: &InstallOpts) -> Option<PathBuf> {
    if opts.scope == InstallScope::Project {
        return None;
    }
    let base = base_config_dir(opts)?;
    Some(base.join("opencode").join("config.json"))
}

fn cursor_path(opts: &InstallOpts) -> Option<PathBuf> {
    if opts.scope == InstallScope::Project {
        let project = opts.project_path.clone()?;
        return Some(project.join(".cursor").join("mcp.json"));
    }
    let home = base_home_dir(opts)?;
    Some(home.join(".cursor").join("mcp.json"))
}

// ── Merge / remove helpers ─────────────────────────────────────────────

fn tagpath_json_entry(opts: &InstallOpts) -> Value {
    json!({
        "command": opts.binary,
        "args": ["mcp"],
    })
}

fn merge_mcp_servers_tagpath(root: &mut Map<String, Value>, opts: &InstallOpts) {
    let servers = root
        .entry("mcpServers".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    if !servers.is_object() {
        *servers = Value::Object(Map::new());
    }
    if let Value::Object(map) = servers {
        map.insert("tagpath".to_string(), tagpath_json_entry(opts));
    }
}

fn remove_mcp_servers_tagpath(root: &mut Map<String, Value>) -> bool {
    let servers = match root.get_mut("mcpServers") {
        Some(Value::Object(map)) => map,
        _ => return false,
    };
    servers.remove("tagpath").is_some()
}

fn merge_opencode_tagpath(root: &mut Map<String, Value>, opts: &InstallOpts) {
    let mcp = root
        .entry("mcp".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    if !mcp.is_object() {
        *mcp = Value::Object(Map::new());
    }
    if let Value::Object(map) = mcp {
        map.insert(
            "tagpath".to_string(),
            json!({
                "command": opts.binary,
                "args": ["mcp"],
                "type": "local",
            }),
        );
    }
}

fn remove_opencode_tagpath(root: &mut Map<String, Value>) -> bool {
    let mcp = match root.get_mut("mcp") {
        Some(Value::Object(map)) => map,
        _ => return false,
    };
    mcp.remove("tagpath").is_some()
}

fn merge_codex_tagpath(doc: &mut toml::Table, opts: &InstallOpts) {
    let servers = doc
        .entry("mcp_servers".to_string())
        .or_insert_with(|| toml::Value::Table(toml::Table::new()));
    if !servers.is_table() {
        *servers = toml::Value::Table(toml::Table::new());
    }
    if let toml::Value::Table(map) = servers {
        let mut entry = toml::Table::new();
        entry.insert(
            "command".to_string(),
            toml::Value::String(opts.binary.clone()),
        );
        entry.insert(
            "args".to_string(),
            toml::Value::Array(vec![toml::Value::String("mcp".to_string())]),
        );
        map.insert("tagpath".to_string(), toml::Value::Table(entry));
    }
}

fn remove_codex_tagpath(doc: &mut toml::Table) -> bool {
    let servers = match doc.get_mut("mcp_servers") {
        Some(toml::Value::Table(map)) => map,
        _ => return false,
    };
    servers.remove("tagpath").is_some()
}

fn merge_json_unsupported(_root: &mut Map<String, Value>, _opts: &InstallOpts) {
    // Unreachable: TOML-format harnesses never call into JSON helpers.
    debug_assert!(false, "TOML harness should not request JSON merge");
}

fn remove_json_unsupported(_root: &mut Map<String, Value>) -> bool {
    debug_assert!(false, "TOML harness should not request JSON remove");
    false
}

fn merge_toml_unsupported(_doc: &mut toml::Table, _opts: &InstallOpts) {
    // Unreachable: JSON-format harnesses never call into TOML helpers.
    debug_assert!(false, "JSON harness should not request TOML merge");
}

fn remove_toml_unsupported(_doc: &mut toml::Table) -> bool {
    debug_assert!(false, "JSON harness should not request TOML remove");
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_claude_desktop_emits_mcp_servers_tagpath() {
        let opts = InstallOpts::default();
        let text = render_config("claude-desktop", &opts).unwrap();
        let value: Value = serde_json::from_str(&text).unwrap();
        let entry = value.pointer("/mcpServers/tagpath").expect("tagpath entry");
        assert_eq!(
            entry.get("command").and_then(Value::as_str),
            Some("tagpath")
        );
        assert_eq!(
            entry.get("args").and_then(Value::as_array).map(Vec::len),
            Some(1)
        );
    }

    #[test]
    fn render_codex_emits_toml() {
        let opts = InstallOpts::default();
        let text = render_config("codex", &opts).unwrap();
        assert!(text.contains("[mcp_servers.tagpath]"), "got: {text}");
        let parsed: toml::Table = text.parse().unwrap();
        let entry = parsed
            .get("mcp_servers")
            .and_then(toml::Value::as_table)
            .and_then(|t| t.get("tagpath"))
            .and_then(toml::Value::as_table)
            .expect("tagpath table");
        assert_eq!(
            entry.get("command").and_then(toml::Value::as_str),
            Some("tagpath")
        );
    }

    #[test]
    fn unknown_harness_errors() {
        let opts = InstallOpts::default();
        let err = render_config("ghost-harness", &opts).unwrap_err();
        matches!(err, InstallError::UnknownHarness(_));
    }
}
