//! Integration tests for `tagpath watch` (#p5wat).
//!
//! Spawns the compiled binary, performs filesystem mutations against a
//! disposable project root, and parses NDJSON from stdout. Each test
//! has its own per-process timeout so a missed event fails the test
//! instead of hanging.
//!
//! Some tests are inherently sensitive to notify-crate timings: the
//! debounce window is 150 ms but the OS may coalesce or delay events.
//! Where that's a known flake risk the test uses a bounded retry loop
//! instead of disabling the assertion.

#![cfg(feature = "watch")]

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::thread;
use std::time::{Duration, Instant};

const TIMEOUT: Duration = Duration::from_secs(8);

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_tagpath")
}

fn make_project(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tagpath_test_watch_{label}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    // Bare minimum .naming.toml.
    std::fs::write(
        dir.join(".naming.toml"),
        "version = 1\nname = \"watch-test\"\nconvention = \"snake_case\"\n",
    )
    .unwrap();
    dir
}

fn write_source(root: &Path, rel: &str, contents: &str) {
    let abs = root.join(rel);
    if let Some(parent) = abs.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(&abs, contents).unwrap();
}

fn spawn_watch(root: &Path, extra: &[&str]) -> (Child, Receiver<String>) {
    let mut cmd = Command::new(bin());
    cmd.arg("watch").arg(root);
    for a in extra {
        cmd.arg(a);
    }
    cmd.env("TAGPATH_QUIET", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null());
    let mut child = cmd.spawn().expect("spawn tagpath watch");
    let stdout = child.stdout.take().expect("stdout pipe");
    let (tx, rx) = channel::<String>();
    thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    (child, rx)
}

/// Pull lines from `rx` until `predicate(line)` returns Some, or the
/// timeout elapses. Returns the matched parsed value.
fn await_event<F>(rx: &Receiver<String>, deadline: Instant, mut predicate: F) -> serde_json::Value
where
    F: FnMut(&serde_json::Value) -> bool,
{
    while Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match rx.recv_timeout(remaining) {
            Ok(line) => {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&line)
                    && predicate(&v)
                {
                    return v;
                }
            }
            Err(_) => break,
        }
    }
    panic!("await_event: timed out waiting for matching event");
}

fn kill(mut child: Child) {
    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn once_emits_hello_ready_index_update_then_exits() {
    let root = make_project("once");
    write_source(&root, "src/foo.rs", "fn create_user() {}\n");

    let output = Command::new(bin())
        .arg("watch")
        .arg(&root)
        .arg("--once")
        .env("TAGPATH_QUIET", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("run tagpath watch --once");
    assert!(output.status.success(), "watch --once exit: {output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<_> = stdout.lines().collect();
    assert!(
        lines.len() >= 3,
        "expected at least 3 lines (hello,index_update,ready), got: {lines:?}",
    );
    let parsed: Vec<serde_json::Value> = lines
        .iter()
        .map(|l| serde_json::from_str(l).expect("ndjson"))
        .collect();
    assert_eq!(parsed[0]["type"], "hello", "first line must be hello");
    assert!(
        parsed.iter().any(|v| v["type"] == "ready"),
        "ready event missing",
    );
    let updates: Vec<_> = parsed
        .iter()
        .filter(|v| v["type"] == "index_update")
        .collect();
    assert_eq!(
        updates.len(),
        1,
        "expected exactly one index_update on --once, got {}: {parsed:?}",
        updates.len(),
    );
    // Should not see watcher-loop events.
    assert!(
        !parsed.iter().any(|v| v["type"] == "shutdown"),
        "--once should not emit shutdown (it never armed)"
    );
}

#[test]
fn live_watch_emits_index_update_on_file_touch() {
    let root = make_project("touch");
    write_source(&root, "src/foo.rs", "fn create_user() {}\n");

    let (child, rx) = spawn_watch(&root, &[]);
    let deadline = Instant::now() + TIMEOUT;

    // Wait for ready.
    await_event(&rx, deadline, |v| v["type"] == "ready");

    // Modify the file.
    thread::sleep(Duration::from_millis(50));
    write_source(&root, "src/foo.rs", "fn create_user_v2() {}\n");

    // Expect an index_update with summary.changed >= 1.
    let evt = await_event(&rx, Instant::now() + TIMEOUT, |v| {
        v["type"] == "index_update" && v["summary"]["changed"].as_u64().unwrap_or(0) >= 1
    });
    assert!(evt["summary"]["changed"].as_u64().unwrap() >= 1);
    kill(child);
}

#[test]
fn live_watch_emits_index_update_on_file_delete() {
    let root = make_project("delete");
    write_source(&root, "src/foo.rs", "fn create_user() {}\n");
    write_source(&root, "src/bar.rs", "fn delete_user() {}\n");

    let (child, rx) = spawn_watch(&root, &[]);
    let deadline = Instant::now() + TIMEOUT;
    await_event(&rx, deadline, |v| v["type"] == "ready");

    thread::sleep(Duration::from_millis(50));
    std::fs::remove_file(root.join("src/bar.rs")).unwrap();

    let evt = await_event(&rx, Instant::now() + TIMEOUT, |v| {
        v["type"] == "index_update" && v["summary"]["removed"].as_u64().unwrap_or(0) >= 1
    });
    assert!(evt["summary"]["removed"].as_u64().unwrap() >= 1);
    kill(child);
}

#[test]
fn live_watch_emits_lint_finding_for_malformed_agent_doc() {
    let root = make_project("lint");
    write_source(&root, "src/foo.rs", "fn create_user() {}\n");
    // Initial well-formed agent-doc file present at startup, so the
    // file already lives in the index. The mutation below introduces
    // the malformed directive that should trigger the lint finding.
    write_source(
        &root,
        "plan.md",
        "<!-- agent:exchange patch=append -->\n# plan\n<!-- /agent:exchange -->\n",
    );

    let (child, rx) = spawn_watch(&root, &[]);
    await_event(&rx, Instant::now() + TIMEOUT, |v| v["type"] == "ready");

    thread::sleep(Duration::from_millis(50));
    // Introduce the malformed `archive PATH` (no `=`).
    write_source(
        &root,
        "plan.md",
        "<!-- agent:exchange patch=append -->\n<!-- agent:done archive PATH -->\n<!-- /agent:done -->\n<!-- /agent:exchange -->\n",
    );

    // Allow a short retry window: notify timing on Linux can defer
    // the second event beyond the first debounce window.
    let evt = await_event(&rx, Instant::now() + TIMEOUT, |v| {
        v["type"] == "lint_finding" && v["rule"] == "agent-doc/malformed-attr"
    });
    assert_eq!(evt["dialect"], "agent-doc");
    kill(child);
}

#[cfg(unix)]
#[test]
fn sigint_emits_shutdown_event_before_exit() {
    let root = make_project("sigint");
    write_source(&root, "src/foo.rs", "fn create_user() {}\n");

    let (mut child, rx) = spawn_watch(&root, &[]);
    await_event(&rx, Instant::now() + TIMEOUT, |v| v["type"] == "ready");

    // Send SIGINT.
    let pid = child.id() as i32;
    unsafe {
        libc::kill(pid, libc::SIGINT);
    }

    let evt = await_event(&rx, Instant::now() + TIMEOUT, |v| v["type"] == "shutdown");
    assert!(evt["reason"].is_string());

    let status = child.wait().expect("wait");
    assert!(
        status.success(),
        "watch should exit 0 on SIGINT: {status:?}"
    );
}

#[test]
fn second_instance_refuses_to_start() {
    let root = make_project("lock");
    write_source(&root, "src/foo.rs", "fn create_user() {}\n");

    let (child, rx) = spawn_watch(&root, &[]);
    await_event(&rx, Instant::now() + TIMEOUT, |v| v["type"] == "ready");

    // Now try to start a second instance. Should exit 1 with a clear
    // stderr message and emit no NDJSON.
    let output = Command::new(bin())
        .arg("watch")
        .arg(&root)
        .arg("--once")
        .env("TAGPATH_QUIET", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("spawn second watch");
    assert!(!output.status.success(), "second watch should exit nonzero");
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("another tagpath watch is already running"),
        "expected lockfile error in stderr, got: {stderr}",
    );

    kill(child);
}

#[test]
fn pid_file_removed_on_clean_shutdown() {
    let root = make_project("pidfile");
    write_source(&root, "src/foo.rs", "fn create_user() {}\n");

    let pid_path = root.join(".naming/watch.pid");

    // --once exits cleanly after the initial pass; PID file should
    // not survive.
    let mut child = Command::new(bin())
        .arg("watch")
        .arg(&root)
        .arg("--once")
        .env("TAGPATH_QUIET", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn --once");
    let status = child.wait().expect("wait");
    assert!(status.success(), "--once should exit 0");
    assert!(
        !pid_path.exists(),
        "pid file should be cleaned up after clean exit, but exists at {}",
        pid_path.display()
    );
}

/// Helper: ensure tests don't accidentally include `write!` macros via
/// missing imports.
#[allow(dead_code)]
fn _imports_touched(mut sink: impl Write) {
    let _ = sink.write_all(b"");
}
