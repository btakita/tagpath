//! Long-running filesystem watcher (`tagpath watch`).
//!
//! Observes a project root for source-file changes and emits newline-
//! delimited JSON events on stdout so consumers (tsift, agent-doc editor
//! plugins, ad-hoc `tail -f | jq`) can react in real time without
//! polling `tagpath index --update`.
//!
//! Event wire format:
//!
//! - `hello` — first line; tool/schema/project metadata.
//! - `ready` — initial reindex done, watcher armed.
//! - `index_update` — emitted after each successful incremental reindex
//!   pass (`changed`, `added`, `removed`, `unchanged`, `elapsed_ms`,
//!   plus the union of changed handles).
//! - `lint_finding` — emitted per agent-doc lint finding when a `.md`
//!   file containing the agent-doc trigger string changes.
//! - `error` — non-fatal reindex / lint error; watcher stays alive.
//! - `shutdown` — final line before exit.
//!
//! Stderr carries human-readable status; `TAGPATH_QUIET=1` suppresses
//! it. The schema (`schema_version` field on `hello`) is independent of
//! the on-disk index schema version (`index_schema_version`).

use std::collections::BTreeSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{RecvTimeoutError, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::Serialize;
use serde_json::Value;

use crate::index::{self, BuildOptions, IndexShape};
use crate::lint::{self, AgentDocOptions};

/// Schema version of the NDJSON wire format itself. Bumped when the
/// event shape changes in a way consumers must care about. Not the
/// same as `index::SCHEMA_VERSION`, which versions the on-disk index.
pub const WATCH_SCHEMA_VERSION: u32 = 1;

/// Default debounce window: collect filesystem events for this long
/// after the most-recent event before triggering a reindex pass. 150 ms
/// is the sweet spot empirically — long enough to coalesce an editor
/// "save" burst (touch swap + rename + chmod), short enough that humans
/// perceive the watcher as instant.
pub const DEFAULT_DEBOUNCE_MS: u64 = 150;

/// Relative path of the single-instance PID lockfile.
pub const PID_RELATIVE_PATH: &str = ".naming/watch.pid";

/// Output shape requested by the caller. `Compact` drops per-event
/// handles and lint details, leaving just summary counts for callers
/// that only want a heartbeat (e.g. external supervisor scripts).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmitShape {
    Full,
    Compact,
}

impl EmitShape {
    /// Parse the `--emit-shape` CLI argument.
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "full" => Ok(EmitShape::Full),
            "compact" => Ok(EmitShape::Compact),
            other => Err(format!(
                "unknown --emit-shape value `{other}` (expected `full` or `compact`)"
            )),
        }
    }
}

/// Caller-visible run mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchMode {
    /// Long-running watcher.
    Continuous,
    /// One reindex + lint pass, then exit.
    Once,
}

/// All knobs for a single `tagpath watch` invocation.
#[derive(Debug, Clone)]
pub struct WatchOptions {
    pub project_root: PathBuf,
    pub mode: WatchMode,
    pub run_lint: bool,
    pub emit_shape: EmitShape,
    pub debounce_ms: u64,
    pub quiet: bool,
}

impl WatchOptions {
    pub fn new(project_root: PathBuf) -> Self {
        Self {
            project_root,
            mode: WatchMode::Continuous,
            run_lint: true,
            emit_shape: EmitShape::Full,
            debounce_ms: DEFAULT_DEBOUNCE_MS,
            quiet: false,
        }
    }
}

/// RAII guard for the `.naming/watch.pid` file. Removes the file on
/// clean shutdown. Pid-file format is a single line with the decimal
/// PID.
struct PidLock {
    path: PathBuf,
}

impl PidLock {
    /// Acquire the single-instance lock. Returns `Err` if another live
    /// process already holds it.
    fn acquire(project_root: &Path) -> Result<Self, String> {
        let path = project_root.join(PID_RELATIVE_PATH);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("create_dir_all({}): {e}", parent.display()))?;
        }
        if path.exists()
            && let Ok(text) = std::fs::read_to_string(&path)
            && let Ok(pid) = text.trim().parse::<i32>()
            && is_pid_alive(pid)
        {
            return Err(format!(
                "another tagpath watch is already running (pid {pid}, lockfile {})",
                path.display()
            ));
        }
        let pid = std::process::id();
        std::fs::write(&path, format!("{pid}\n"))
            .map_err(|e| format!("write({}): {e}", path.display()))?;
        Ok(Self { path })
    }
}

impl Drop for PidLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Best-effort liveness check for a PID. On unix, signal 0 returns 0
/// when the process exists, ESRCH otherwise. On other targets we
/// conservatively report "alive" so a stale lockfile after a crash on
/// Windows still needs a manual delete.
#[cfg(unix)]
fn is_pid_alive(pid: i32) -> bool {
    // SAFETY: kill(pid, 0) is signal-safe and only probes existence.
    unsafe { libc::kill(pid, 0) == 0 }
}

#[cfg(not(unix))]
fn is_pid_alive(_pid: i32) -> bool {
    true
}

/// Entry point: run the watcher. Returns process exit code.
pub fn run(opts: WatchOptions) -> i32 {
    let _pid_lock = match PidLock::acquire(&opts.project_root) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("error: {e}");
            return 1;
        }
    };

    // Stdout writer; flush after every event so consumers reading
    // line-buffered from a pipe see events promptly.
    let writer = Arc::new(Mutex::new(std::io::stdout()));

    // Emit hello.
    let hello = serde_json::json!({
        "type": "hello",
        "schema_version": WATCH_SCHEMA_VERSION,
        "tool_version": env!("CARGO_PKG_VERSION"),
        "project_root": opts.project_root.display().to_string(),
        "index_schema_version": index::SCHEMA_VERSION,
        "watcher": "notify-6",
    });
    if emit(&writer, &hello).is_err() {
        return 1;
    }

    // Initial reindex pass.
    let mut last_handles = collect_handles(&opts.project_root);
    if let Err(e) = run_reindex_pass(
        &opts,
        &writer,
        &mut last_handles,
        /*include_lint=*/ true,
    ) {
        emit_error(&writer, "index_update", &e);
        if opts.mode == WatchMode::Once {
            return 1;
        }
    }

    // Ready marker.
    let ready = serde_json::json!({ "type": "ready" });
    let _ = emit(&writer, &ready);

    if opts.mode == WatchMode::Once {
        // Once mode: do not arm the watcher. Just exit cleanly.
        return 0;
    }

    // Install signal handler.
    let shutdown_flag = Arc::new(AtomicBool::new(false));
    let shutdown_reason = Arc::new(Mutex::new(String::from("signal")));
    install_signal_handler(shutdown_flag.clone(), shutdown_reason.clone());

    // Spawn the notify watcher.
    let (tx, rx) = channel::<notify::Result<notify::Event>>();
    let mut watcher: RecommendedWatcher = match notify::recommended_watcher(tx) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("error: notify::recommended_watcher: {e}");
            return 1;
        }
    };
    if let Err(e) = watcher.watch(&opts.project_root, RecursiveMode::Recursive) {
        eprintln!("error: notify watch({}): {e}", opts.project_root.display());
        return 1;
    }

    // Event loop with debouncing.
    let debounce = Duration::from_millis(opts.debounce_ms);
    let mut pending_paths: BTreeSet<PathBuf> = BTreeSet::new();
    let mut last_event_at: Option<Instant> = None;
    loop {
        if shutdown_flag.load(Ordering::SeqCst) {
            let reason = shutdown_reason
                .lock()
                .map(|g| g.clone())
                .unwrap_or_else(|_| "signal".to_string());
            let _ = emit(
                &writer,
                &serde_json::json!({
                    "type": "shutdown",
                    "reason": reason,
                }),
            );
            return 0;
        }

        let recv_timeout = if last_event_at.is_some() {
            debounce
        } else {
            Duration::from_millis(250)
        };

        match rx.recv_timeout(recv_timeout) {
            Ok(Ok(event)) => {
                if let Some(paths) = filter_event(&opts.project_root, &event) {
                    for p in paths {
                        pending_paths.insert(p);
                    }
                    last_event_at = Some(Instant::now());
                }
            }
            Ok(Err(e)) => {
                emit_error(&writer, "watcher", &format!("notify: {e}"));
            }
            Err(RecvTimeoutError::Timeout) => {
                // Fall through to debounce check.
            }
            Err(RecvTimeoutError::Disconnected) => {
                emit_error(&writer, "watcher", "notify channel disconnected");
                return 1;
            }
        }

        // Debounce window expired?
        if let Some(when) = last_event_at
            && when.elapsed() >= debounce
            && !pending_paths.is_empty()
        {
            let lint_paths: Vec<PathBuf> = pending_paths
                .iter()
                .filter(|p| is_agent_doc_candidate(p))
                .cloned()
                .collect();
            pending_paths.clear();
            last_event_at = None;
            if let Err(e) = run_reindex_pass(&opts, &writer, &mut last_handles, opts.run_lint) {
                emit_error(&writer, "index_update", &e);
            }
            if opts.run_lint {
                for p in &lint_paths {
                    if let Err(e) = run_lint_pass(&opts, &writer, p) {
                        emit_error(&writer, "lint", &e);
                    }
                }
            }
        }
    }
}

/// One reindex pass. Updates `last_handles` to the post-pass snapshot
/// so the next call can compute the changed-handle set.
fn run_reindex_pass(
    opts: &WatchOptions,
    writer: &Arc<Mutex<std::io::Stdout>>,
    last_handles: &mut BTreeSet<String>,
    _include_lint: bool,
) -> Result<(), String> {
    let build_opts = BuildOptions {
        project_root: opts.project_root.clone(),
    };
    let result = index::update_incremental_with(&build_opts, IndexShape::Full)?;
    let new_handles = handles_from_index(&result.index);
    let added: Vec<&String> = new_handles.difference(last_handles).collect();
    let removed: Vec<&String> = last_handles.difference(&new_handles).collect();
    let mut changed_handles: Vec<String> = added.iter().map(|s| (*s).clone()).collect();
    for r in &removed {
        changed_handles.push((*r).clone());
    }
    changed_handles.sort();
    changed_handles.dedup();

    // Persist the index on disk so external `tagpath search --index`
    // consumers see fresh data. Skip on a true no-op to preserve the
    // mtime semantics that `tagpath index --update` already enforces.
    if !result.summary.is_noop() {
        let idx_path = index::index_path(&opts.project_root);
        if let Err(e) = index::write(&result.index, &idx_path) {
            return Err(format!("write index: {e}"));
        }
    }

    if !opts.quiet {
        eprintln!(
            "[tagpath watch] reindexed {} files in {}ms",
            result.summary.changed + result.summary.added + result.summary.removed,
            result.summary.elapsed_ms,
        );
    }

    let summary = serde_json::json!({
        "changed": result.summary.changed,
        "added": result.summary.added,
        "removed": result.summary.removed,
        "unchanged": result.summary.unchanged,
        "elapsed_ms": result.summary.elapsed_ms,
    });
    let event = match opts.emit_shape {
        EmitShape::Full => serde_json::json!({
            "type": "index_update",
            "summary": summary,
            "changed_handles": changed_handles,
        }),
        EmitShape::Compact => serde_json::json!({
            "type": "index_update",
            "summary": summary,
        }),
    };
    let _ = emit(writer, &event);
    *last_handles = new_handles;
    Ok(())
}

/// Lint a single `.md` file using the agent-doc dialect. Findings are
/// emitted as `lint_finding` events; structural read errors as
/// `error` events.
fn run_lint_pass(
    opts: &WatchOptions,
    writer: &Arc<Mutex<std::io::Stdout>>,
    path: &Path,
) -> Result<(), String> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => {
            // Treat read failure as non-fatal: file may have been
            // deleted between debounce and lint pass.
            return Err(format!("read({}): {e}", path.display()));
        }
    };
    if !lint::looks_like_agent_doc(&text) {
        return Ok(());
    }
    let findings = lint::lint_agent_doc(path, &text, &AgentDocOptions::default());
    for f in &findings {
        let event = match opts.emit_shape {
            EmitShape::Full => serde_json::json!({
                "type": "lint_finding",
                "dialect": "agent-doc",
                "path": f.path.display().to_string(),
                "line": f.line,
                "col": f.col,
                "rule": f.rule,
                "severity": match f.severity {
                    lint::LintSeverity::Error => "error",
                    lint::LintSeverity::Warning => "warning",
                },
                "message": f.message,
                "fix_hint": f.fix_hint,
            }),
            EmitShape::Compact => serde_json::json!({
                "type": "lint_finding",
                "rule": f.rule,
                "path": f.path.display().to_string(),
            }),
        };
        let _ = emit(writer, &event);
    }
    Ok(())
}

#[derive(Debug, Serialize)]
struct ErrorEvent<'a> {
    #[serde(rename = "type")]
    ty: &'static str,
    stage: &'a str,
    reason: &'a str,
}

fn emit(writer: &Arc<Mutex<std::io::Stdout>>, value: &Value) -> Result<(), std::io::Error> {
    let mut g = writer
        .lock()
        .map_err(|_| std::io::Error::other("stdout writer poisoned"))?;
    let mut line = serde_json::to_string(value).unwrap_or_else(|_| "{}".to_string());
    line.push('\n');
    g.write_all(line.as_bytes())?;
    g.flush()
}

fn emit_error(writer: &Arc<Mutex<std::io::Stdout>>, stage: &str, reason: &str) {
    let evt = ErrorEvent {
        ty: "error",
        stage,
        reason,
    };
    let value = serde_json::to_value(&evt).unwrap_or(Value::Null);
    let _ = emit(writer, &value);
}

/// Build the set of handles (`fam:…` + `mem:…`) currently in an index.
fn handles_from_index(idx: &index::Index) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for fam in &idx.families {
        out.insert(fam.handle.clone());
        for m in &fam.members {
            out.insert(m.handle.clone());
        }
    }
    out
}

/// Read the existing on-disk index (if any) and harvest its handle
/// set. On any failure we return an empty set so the next reindex
/// reports every handle as "new" — acceptable because the alternative
/// is silently dropping events.
fn collect_handles(project_root: &Path) -> BTreeSet<String> {
    let path = index::index_path(project_root);
    if !path.exists() {
        return BTreeSet::new();
    }
    match index::read(&path) {
        Ok(idx) => handles_from_index(&idx),
        Err(_) => BTreeSet::new(),
    }
}

/// Decide whether a notify event is interesting and, if so, return
/// the candidate paths to feed into the next reindex pass. Returns
/// `None` for events we want to ignore entirely.
fn filter_event(project_root: &Path, event: &notify::Event) -> Option<Vec<PathBuf>> {
    if !matches!(
        event.kind,
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
    ) {
        return None;
    }
    let mut paths = Vec::new();
    for p in &event.paths {
        if is_ignored_path(project_root, p) {
            continue;
        }
        paths.push(p.clone());
    }
    if paths.is_empty() { None } else { Some(paths) }
}

/// Match the same ignore rules used by `extract::list_source_files`
/// (target/, .git/, node_modules/, __pycache__/, .venv/, vendor/) and
/// additionally reject every path inside `.naming/` to prevent the
/// watcher firing on its own `.naming/index.json` writes.
fn is_ignored_path(project_root: &Path, path: &Path) -> bool {
    let rel = match path.strip_prefix(project_root) {
        Ok(r) => r,
        Err(_) => return false,
    };
    for comp in rel.components() {
        let name = comp.as_os_str().to_string_lossy();
        if name == ".naming"
            || name == ".git"
            || name == "target"
            || name == "node_modules"
            || name == "__pycache__"
            || name == ".venv"
            || name == "vendor"
        {
            return true;
        }
    }
    false
}

/// True for `.md` files (lint scan candidates). The `looks_like_agent_doc`
/// content check still runs inside [`run_lint_pass`].
fn is_agent_doc_candidate(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()) == Some("md")
}

// Process-wide signal-handling state. SIGINT/SIGTERM write into the
// flag + reason code from an async-signal-safe handler; the main loop
// polls them between debounce ticks.
#[cfg(unix)]
static SHUTDOWN_FLAG: AtomicBool = AtomicBool::new(false);
#[cfg(unix)]
static SHUTDOWN_REASON: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

#[cfg(unix)]
fn install_signal_handler(flag: Arc<AtomicBool>, reason: Arc<Mutex<String>>) {
    // We rebroadcast the global state into the caller-supplied
    // Arcs from a tiny watcher thread. Doing the rebroadcast outside
    // the signal handler keeps the handler async-signal-safe.
    extern "C" fn handler(sig: libc::c_int) {
        SHUTDOWN_FLAG.store(true, Ordering::SeqCst);
        SHUTDOWN_REASON.store(sig, Ordering::SeqCst);
    }

    unsafe {
        let mut sa: libc::sigaction = std::mem::zeroed();
        sa.sa_sigaction = handler as *const () as usize;
        sa.sa_flags = 0;
        libc::sigemptyset(&mut sa.sa_mask);
        libc::sigaction(libc::SIGINT, &sa, std::ptr::null_mut());
        libc::sigaction(libc::SIGTERM, &sa, std::ptr::null_mut());
    }

    std::thread::spawn(move || {
        loop {
            if SHUTDOWN_FLAG.load(Ordering::SeqCst) {
                let sig = SHUTDOWN_REASON.load(Ordering::SeqCst);
                let name = match sig {
                    libc::SIGINT => "sigint",
                    libc::SIGTERM => "sigterm",
                    _ => "signal",
                };
                if let Ok(mut g) = reason.lock() {
                    *g = name.to_string();
                }
                flag.store(true, Ordering::SeqCst);
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    });
}

#[cfg(not(unix))]
fn install_signal_handler(_flag: Arc<AtomicBool>, _reason: Arc<Mutex<String>>) {
    // No-op on non-unix targets; the user can still Ctrl+C the
    // process and the OS will tear it down without emitting the
    // shutdown event. PID lockfile cleanup falls to the next-run
    // staleness check.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emit_shape_parses_known_values() {
        assert_eq!(EmitShape::parse("full").unwrap(), EmitShape::Full);
        assert_eq!(EmitShape::parse("compact").unwrap(), EmitShape::Compact);
        assert!(EmitShape::parse("bogus").is_err());
    }

    #[test]
    fn ignored_paths_cover_naming_and_skipped_dirs() {
        let root = PathBuf::from("/tmp/proj");
        assert!(is_ignored_path(&root, &root.join(".naming/index.json")));
        assert!(is_ignored_path(&root, &root.join(".git/HEAD")));
        assert!(is_ignored_path(&root, &root.join("target/debug/foo")));
        assert!(is_ignored_path(&root, &root.join("node_modules/x/y.js")));
        assert!(!is_ignored_path(&root, &root.join("src/main.rs")));
    }

    #[test]
    fn agent_doc_candidate_matches_md() {
        assert!(is_agent_doc_candidate(Path::new("plan.md")));
        assert!(!is_agent_doc_candidate(Path::new("src/main.rs")));
    }
}
