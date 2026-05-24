//! Runtime tree-sitter grammar loading.
//!
//! Loads `.so` / `.dylib` / `.dll` files via [`libloading`] and wraps the
//! exported `tree_sitter_<lang>` entrypoint as a [`tree_sitter::Language`].
//!
//! Gated behind the `dyn-grammar` Cargo feature and only compiled for
//! non-WASM targets — loading arbitrary shared libraries is a code-execution
//! boundary and never appropriate on WASM.
//!
//! See `SPEC.md` § "Dynamic grammar loading" for the full contract.

#![cfg(all(feature = "dyn-grammar", not(target_arch = "wasm32")))]

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use libloading::{Library, Symbol};
use tree_sitter::{LANGUAGE_VERSION, Language, MIN_COMPATIBLE_LANGUAGE_VERSION};
use tree_sitter_language::LanguageFn;

use crate::config::GrammarsConfig;

/// A grammar that was successfully loaded from a shared library.
pub struct LoadedGrammar {
    /// Logical language key (e.g. `"zig"`).
    pub language: String,
    /// File extensions this grammar handles (without leading dot).
    pub extensions: Vec<String>,
    /// The constructed `tree_sitter::Language`.
    pub language_fn: Language,
    /// Where the shared library was loaded from.
    pub source_path: PathBuf,
    /// Reported ABI version of the grammar (matches `Language::abi_version`).
    pub abi_version: usize,
    /// Keep the `Library` handle alive for the lifetime of `language_fn` —
    /// dropping it before `language_fn` is fatal.
    _lib: Library,
}

impl fmt::Debug for LoadedGrammar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LoadedGrammar")
            .field("language", &self.language)
            .field("extensions", &self.extensions)
            .field("source_path", &self.source_path)
            .field("abi_version", &self.abi_version)
            .finish()
    }
}

/// A candidate grammar file discovered on disk under one of the configured
/// `load_dirs`. Loading is deferred — call [`load_path`] to actually open it.
#[derive(Debug, Clone)]
pub struct DiscoveredGrammar {
    /// Inferred language key from the file stem
    /// (e.g. `libtree-sitter-zig.so` → `"zig"`).
    pub language: String,
    /// Absolute path to the shared library.
    pub path: PathBuf,
    /// Inferred entry symbol — `tree_sitter_{language}`.
    pub symbol: String,
}

/// Errors that may occur while loading a runtime grammar.
#[derive(Debug)]
pub enum LoadError {
    /// The configured path does not exist on disk.
    MissingFile { path: PathBuf },
    /// `dlopen` (or platform equivalent) failed.
    DlOpen {
        path: PathBuf,
        source: libloading::Error,
    },
    /// `dlsym` for the configured (or default) entry symbol failed.
    MissingSymbol {
        path: PathBuf,
        symbol: String,
        source: libloading::Error,
    },
    /// The grammar's reported ABI version is outside the range supported by
    /// the linked `tree-sitter` runtime.
    AbiMismatch {
        path: PathBuf,
        actual: usize,
        expected_min: usize,
        expected_max: usize,
    },
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::MissingFile { path } => write!(
                f,
                "tagpath dyn-grammar: file not found at {}; check the `path` value under [grammars.languages.*] in .naming.toml",
                path.display()
            ),
            LoadError::DlOpen { path, source } => write!(
                f,
                "tagpath dyn-grammar: failed to open shared library {}: {source} (is the file a valid tree-sitter grammar built for this platform?)",
                path.display()
            ),
            LoadError::MissingSymbol {
                path,
                symbol,
                source,
            } => write!(
                f,
                "tagpath dyn-grammar: symbol `{symbol}` not found in {}: {source} (set `symbol = \"...\"` under the language entry if the grammar uses a non-default entrypoint name)",
                path.display()
            ),
            LoadError::AbiMismatch {
                path,
                actual,
                expected_min,
                expected_max,
            } => write!(
                f,
                "tagpath dyn-grammar: ABI version mismatch for {}: grammar reports v{actual}, this tagpath build supports v{expected_min}..=v{expected_max}. Rebuild the grammar against a compatible tree-sitter CLI.",
                path.display()
            ),
        }
    }
}

impl std::error::Error for LoadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            LoadError::DlOpen { source, .. } | LoadError::MissingSymbol { source, .. } => {
                Some(source)
            }
            _ => None,
        }
    }
}

impl LoadError {
    /// The path that triggered this error (for grouping in CLI output).
    pub fn path(&self) -> &Path {
        match self {
            LoadError::MissingFile { path }
            | LoadError::DlOpen { path, .. }
            | LoadError::MissingSymbol { path, .. }
            | LoadError::AbiMismatch { path, .. } => path,
        }
    }
}

/// Type signature every tree-sitter grammar entrypoint must expose:
/// `unsafe extern "C" fn() -> *const ()`.
type GrammarFn = unsafe extern "C" fn() -> *const ();

/// Recognized shared-library suffixes on the supported platforms.
const SHARED_LIB_SUFFIXES: &[&str] = &["so", "dylib", "dll"];

/// Strip common prefixes/suffixes from a grammar filename to recover the
/// logical language name (e.g. `libtree-sitter-zig.so` → `zig`).
fn infer_language_name(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_str()?;
    let stripped = stem.strip_prefix("lib").unwrap_or(stem);
    let stripped = stripped
        .strip_prefix("tree-sitter-")
        .or_else(|| stripped.strip_prefix("tree_sitter_"))
        .unwrap_or(stripped);
    if stripped.is_empty() {
        None
    } else {
        Some(stripped.to_string())
    }
}

/// True if `path` has a recognized shared-library extension.
fn is_shared_library(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|ext| {
            SHARED_LIB_SUFFIXES
                .iter()
                .any(|s| s.eq_ignore_ascii_case(ext))
        })
        .unwrap_or(false)
}

/// Scan each directory for `*.so` / `*.dylib` / `*.dll` files and return
/// discovered candidates. Non-existent or unreadable directories are silently
/// skipped (they show up via `load_configured` or `grammars check` instead).
pub fn discover(dirs: &[PathBuf]) -> Vec<DiscoveredGrammar> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for dir in dirs {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() || !is_shared_library(&path) {
                continue;
            }
            let Some(language) = infer_language_name(&path) else {
                continue;
            };
            if !seen.insert(path.clone()) {
                continue;
            }
            let symbol = format!("tree_sitter_{language}");
            out.push(DiscoveredGrammar {
                language,
                path,
                symbol,
            });
        }
    }
    out
}

/// Load a single grammar from `path` using `symbol` (or `tree_sitter_{language}`
/// if `symbol` is `None`).
pub fn load_path(
    language: &str,
    path: &Path,
    symbol: Option<&str>,
    extensions: Vec<String>,
) -> Result<LoadedGrammar, LoadError> {
    if !path.exists() {
        return Err(LoadError::MissingFile {
            path: path.to_path_buf(),
        });
    }
    // SAFETY: dynamic library loading is inherently unsafe — the caller has
    // already accepted this by pointing `[grammars]` at a path on disk. See
    // SPEC § "Dynamic grammar loading" → Security.
    let lib = unsafe { Library::new(path) }.map_err(|e| LoadError::DlOpen {
        path: path.to_path_buf(),
        source: e,
    })?;
    let default_symbol;
    let symbol_name = match symbol {
        Some(s) => s,
        None => {
            default_symbol = format!("tree_sitter_{language}");
            &default_symbol
        }
    };
    // SAFETY: every tree-sitter grammar shared library exposes
    // `unsafe extern "C" fn() -> *const ()` as its entrypoint. A mismatched
    // signature is undefined behaviour, but is also a misuse of this API —
    // documented in SPEC.
    let entry: Symbol<'_, GrammarFn> =
        unsafe { lib.get(symbol_name.as_bytes()) }.map_err(|e| LoadError::MissingSymbol {
            path: path.to_path_buf(),
            symbol: symbol_name.to_string(),
            source: e,
        })?;
    // Wrap as LanguageFn → Language so it integrates with tree-sitter's runtime.
    let raw_fn: GrammarFn = *entry;
    let language_fn = Language::new(unsafe { LanguageFn::from_raw(raw_fn) });
    let abi_version = language_fn.abi_version();
    if !(MIN_COMPATIBLE_LANGUAGE_VERSION..=LANGUAGE_VERSION).contains(&abi_version) {
        return Err(LoadError::AbiMismatch {
            path: path.to_path_buf(),
            actual: abi_version,
            expected_min: MIN_COMPATIBLE_LANGUAGE_VERSION,
            expected_max: LANGUAGE_VERSION,
        });
    }
    Ok(LoadedGrammar {
        language: language.to_string(),
        extensions,
        language_fn,
        source_path: path.to_path_buf(),
        abi_version,
        _lib: lib,
    })
}

/// Load every grammar enumerated under `cfg.languages`. `base_dir` is the
/// directory containing `.naming.toml`, used to resolve relative paths.
///
/// On the first error this function stops and returns it; callers that need
/// per-grammar errors should use [`load_configured_all`].
pub fn load_configured(
    cfg: &GrammarsConfig,
    base_dir: &Path,
) -> Result<Vec<LoadedGrammar>, LoadError> {
    let mut grammars = Vec::with_capacity(cfg.languages.len());
    for (lang, entry) in &cfg.languages {
        let resolved = crate::config::expand_grammar_path(&entry.path, base_dir);
        let loaded = load_path(
            lang,
            &resolved,
            entry.symbol.as_deref(),
            entry.extensions.clone(),
        )?;
        grammars.push(loaded);
    }
    Ok(grammars)
}

/// Load every configured grammar, collecting per-language results separately
/// so the CLI can render a full success/failure report.
pub fn load_configured_all(
    cfg: &GrammarsConfig,
    base_dir: &Path,
) -> BTreeMap<String, Result<LoadedGrammar, LoadError>> {
    let mut out = BTreeMap::new();
    for (lang, entry) in &cfg.languages {
        let resolved = crate::config::expand_grammar_path(&entry.path, base_dir);
        let loaded = load_path(
            lang,
            &resolved,
            entry.symbol.as_deref(),
            entry.extensions.clone(),
        );
        out.insert(lang.clone(), loaded);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infer_language_strips_lib_and_tree_sitter() {
        assert_eq!(
            infer_language_name(Path::new("/g/libtree-sitter-zig.so")).as_deref(),
            Some("zig")
        );
        assert_eq!(
            infer_language_name(Path::new("tree-sitter-nim.dylib")).as_deref(),
            Some("nim")
        );
        assert_eq!(
            infer_language_name(Path::new("tree_sitter_odin.dll")).as_deref(),
            Some("odin")
        );
        assert_eq!(
            infer_language_name(Path::new("custom.so")).as_deref(),
            Some("custom")
        );
    }

    #[test]
    fn is_shared_library_recognizes_platform_suffixes() {
        assert!(is_shared_library(Path::new("foo.so")));
        assert!(is_shared_library(Path::new("foo.dylib")));
        assert!(is_shared_library(Path::new("foo.DLL")));
        assert!(!is_shared_library(Path::new("foo.txt")));
        assert!(!is_shared_library(Path::new("foo")));
    }
}
