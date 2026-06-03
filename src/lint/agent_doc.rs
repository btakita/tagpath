//! Agent-doc dialect lint for `tagpath lint`.
//!
//! Validates session-document HTML-comment tags used by the `agent-doc`
//! family (Claude Code / Codex / OpenCode / direct harnesses). The dialect
//! enforces the directive grammar so that malformed forms like
//! `<!-- agent:done archive PATH -->` (missing `=`) fail at lint time
//! rather than deep inside `finalize`.
//!
//! Rule IDs are namespaced under `agent-doc/`.

use serde::Serialize;
use std::path::{Path, PathBuf};

/// Severity of an agent-doc finding.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LintSeverity {
    Error,
    Warning,
}

/// A single agent-doc lint finding.
#[derive(Debug, Clone, Serialize)]
pub struct LintFinding {
    pub path: PathBuf,
    pub line: usize,
    pub col: usize,
    pub rule: String,
    pub severity: LintSeverity,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix_hint: Option<String>,
}

/// Configuration flags for the agent-doc dialect.
#[derive(Debug, Clone, Default)]
pub struct AgentDocOptions {
    /// Enable filesystem-dependent rules (archive target existence,
    /// done/backlog id cross-reference).
    pub fs_checks: bool,
    /// Optional set of rule IDs to restrict findings to. Empty means all.
    pub rule_filter: Vec<String>,
}

const KNOWN_COMPONENTS: &[&str] = &[
    "agent:exchange",
    "agent:status",
    "agent:backlog",
    "agent:done",
    "agent:icebox",
    "agent:queue",
    "agent:review",
];

/// Legacy component names that agent-doc still accepts as migration inputs.
/// Recognized by the lint so legacy session docs do not fail closed before
/// `agent-doc migrate` runs. See agent-doc's `src/migrate.rs` for the canonical
/// rename map (agent:pending → agent:backlog, agent:pending-done /
/// agent:backlog-done → agent:done).
const LEGACY_COMPONENTS: &[&str] = &[
    "agent:pending",
    "agent:pending-done",
    "agent:backlog-done",
];

fn is_known_component(name: &str) -> bool {
    KNOWN_COMPONENTS.contains(&name) || LEGACY_COMPONENTS.contains(&name)
}

/// Returns true if the file *looks like* an agent-doc session document.
/// Used by callers for auto-detection when no `--dialect` flag is given.
pub fn looks_like_agent_doc(text: &str) -> bool {
    text.contains("<!-- agent:exchange") || text.contains("<!--agent:exchange")
}

/// Lint an agent-doc session document. Returns findings (possibly empty).
pub fn lint_agent_doc(path: &Path, text: &str, opts: &AgentDocOptions) -> Vec<LintFinding> {
    let mut findings: Vec<LintFinding> = Vec::new();
    let comments = scan_comments(text);

    // First pass: per-comment structural / attribute rules.
    for c in &comments {
        check_single_comment(path, c, &mut findings);
    }

    // Second pass: pairing rules + backlog id collisions + done id cross-ref.
    check_pairing(path, &comments, &mut findings);
    check_backlog_ids(path, text, &comments, &mut findings);

    // FS-dependent rules.
    if opts.fs_checks {
        check_archive_targets(path, &comments, &mut findings);
        check_done_ids_in_backlog(path, text, &comments, &mut findings);
    }

    // Apply rule filter if any.
    if !opts.rule_filter.is_empty() {
        findings.retain(|f| opts.rule_filter.iter().any(|r| r == &f.rule));
    }
    findings
}

/// A scanned HTML comment with its inner body and source location.
#[derive(Debug, Clone)]
struct Comment {
    line: usize,
    col: usize,
    /// Inner text between `<!--` and `-->` with surrounding whitespace
    /// stripped.
    body: String,
    /// Byte offset of the opening `<` for this comment in the source.
    byte_start: usize,
    /// Byte offset just past `-->`.
    byte_end: usize,
}

/// Scan all `<!-- ... -->` comments in the source.
///
/// Code-span filtering suppresses comments inside inline code spans, but
/// only for short same-line spans. Multi-line code spans created by
/// unpaired backticks in queue/review/backlog content must not suppress
/// real structural component tags.
fn scan_comments(text: &str) -> Vec<Comment> {
    let code_ranges = find_code_and_fence_ranges(text);
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i + 3 < bytes.len() {
        if &bytes[i..i + 4] == b"<!--" {
            let start = i;
            let body_start = i + 4;
            if let Some(rel_end) = text[body_start..].find("-->") {
                let body = text[body_start..body_start + rel_end].trim().to_string();
                let byte_end = body_start + rel_end + 3;
                let in_code = in_code_range(&code_ranges, i);
                let suppress = if in_code {
                    should_suppress_in_code(text, i, &code_ranges)
                } else {
                    false
                };
                if !suppress {
                    let (line, col) = byte_to_line_col(text, start);
                    out.push(Comment {
                        line,
                        col,
                        body,
                        byte_start: start,
                        byte_end,
                    });
                }
                i = byte_end;
                continue;
            } else {
                break;
            }
        }
        i += 1;
    }
    out
}

fn should_suppress_in_code(text: &str, pos: usize, code_ranges: &[(usize, usize)]) -> bool {
    for &(start, end) in code_ranges {
        if pos >= start && pos < end {
            let span_text = &text[start..end.min(text.len())];
            let is_fence = span_text.starts_with("```");
            if is_fence {
                return true;
            }
            let newlines = span_text.chars().filter(|&c| c == '\n').count();
            return newlines == 0;
        }
    }
    false
}

fn find_code_and_fence_ranges(text: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i < len {
        if bytes[i] != b'`' {
            i += 1;
            continue;
        }
        // Count opening backtick run
        let run_start = i;
        let mut run_len = 0usize;
        while i + run_len < len && bytes[i + run_len] == b'`' {
            run_len += 1;
        }
        if run_len >= 3 && (run_start == 0 || bytes[run_start - 1] == b'\n') {
            // Fenced code block
            let fence_start = run_start;
            let fence_len = run_len;
            let mut j = run_start + fence_len;
            while j + fence_len <= len {
                if (j == 0 || bytes[j - 1] == b'\n')
                    && bytes[j..j + fence_len] == bytes[fence_start..fence_start + fence_len]
                {
                    ranges.push((fence_start, j + fence_len));
                    i = j + fence_len;
                    break;
                }
                j += 1;
            }
            if j + fence_len > len {
                ranges.push((fence_start, len));
                break;
            }
            continue;
        }
        if run_len >= 1 {
            // Inline code span: find matching closing run of same length
            let mut j = run_start + run_len;
            while j + run_len <= len {
                if bytes[j..j + run_len] == bytes[run_start..run_start + run_len] {
                    let span_end = j + run_len;
                    if span_end > run_start + run_len {
                        ranges.push((run_start, span_end));
                    }
                    i = span_end;
                    break;
                }
                j += 1;
            }
            if j + run_len > len {
                i = run_start + run_len;
            }
            continue;
        }
        i += 1;
    }
    ranges
}

fn in_code_range(ranges: &[(usize, usize)], pos: usize) -> bool {
    ranges.iter().any(|&(start, end)| pos >= start && pos < end)
}

fn byte_to_line_col(text: &str, byte_offset: usize) -> (usize, usize) {
    let mut line = 1usize;
    let mut col = 1usize;
    for (i, ch) in text.char_indices() {
        if i >= byte_offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

/// Classification of a comment's first token.
#[derive(Debug, Clone)]
enum CommentKind<'a> {
    /// Open component, e.g. `agent:exchange [attrs...]`.
    OpenComponent {
        name: &'a str,
        rest: &'a str,
    },
    /// Close component, e.g. `/agent:exchange`.
    CloseComponent {
        name: &'a str,
    },
    /// Boundary directive, e.g. `agent:boundary:HEXID`.
    Boundary {
        hex: &'a str,
    },
    /// Suppression marker like `no-pending-done-guard`.
    Marker {
        name: &'a str,
    },
    /// Patch-marker open (`patch:exchange`) or close (`/patch:exchange`).
    PatchOpen {
        name: &'a str,
        rest: &'a str,
    },
    PatchClose,
    /// Replace marker (`replace:icebox` etc.) — recognized but not validated
    /// beyond presence.
    Replace,
    /// Anything else; ignored by this lint.
    Other,
}

fn classify(body: &str) -> CommentKind<'_> {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return CommentKind::Other;
    }

    // Patch markers.
    if let Some(rest) = trimmed.strip_prefix("patch:") {
        // `patch:exchange [attrs]` — name terminated by whitespace.
        let (name, rest_attrs) = split_first_token(rest);
        return CommentKind::PatchOpen {
            name,
            rest: rest_attrs,
        };
    }
    if trimmed.starts_with("/patch:") {
        return CommentKind::PatchClose;
    }

    // Boundary directive.
    if let Some(rest) = trimmed.strip_prefix("agent:boundary:") {
        // boundary may have trailing whitespace already trimmed.
        return CommentKind::Boundary { hex: rest.trim() };
    }

    // Replace markers (replace:icebox etc).
    if trimmed.starts_with("replace:") {
        return CommentKind::Replace;
    }

    // Close component.
    if let Some(rest) = trimmed.strip_prefix("/agent:") {
        let (name_part, _) = split_first_token(rest);
        // Re-attach the agent: prefix for ergonomic comparisons.
        // Use Box::leak to keep lifetime tied to the input? Easier: store
        // just the name without the prefix and compare against the suffix
        // list everywhere.
        return CommentKind::CloseComponent { name: name_part };
    }

    // Open component (`agent:NAME ...`).
    if let Some(rest) = trimmed.strip_prefix("agent:") {
        let (name_part, rest_attrs) = split_first_token(rest);
        return CommentKind::OpenComponent {
            name: name_part,
            rest: rest_attrs,
        };
    }

    // Single-token marker like `no-pending-done-guard`.
    let (first, rest) = split_first_token(trimmed);
    if rest.trim().is_empty() {
        return CommentKind::Marker { name: first };
    }

    CommentKind::Other
}

/// Split off the first whitespace-delimited token.
fn split_first_token(s: &str) -> (&str, &str) {
    let s = s.trim_start();
    match s.find(|c: char| c.is_whitespace()) {
        Some(idx) => (&s[..idx], &s[idx..]),
        None => (s, ""),
    }
}

/// Markers that are *recognized* but require no further validation here.
const KNOWN_MARKERS: &[&str] = &["no-pending-done-guard"];

fn check_single_comment(path: &Path, c: &Comment, out: &mut Vec<LintFinding>) {
    let kind = classify(&c.body);
    match kind {
        CommentKind::OpenComponent { name, rest } => {
            let full = format!("agent:{name}");
            if !is_known_component(&full) {
                out.push(LintFinding {
                    path: path.to_path_buf(),
                    line: c.line,
                    col: c.col,
                    rule: "agent-doc/unknown-component".to_string(),
                    severity: LintSeverity::Error,
                    message: format!("unknown agent-doc component `{full}`"),
                    fix_hint: Some(format!("valid components: {}", KNOWN_COMPONENTS.join(", "))),
                });
                return;
            }
            // Attribute checks.
            check_attrs(path, c, &full, rest, out);
        }
        CommentKind::CloseComponent { name } => {
            let full = format!("agent:{name}");
            if !is_known_component(&full) {
                out.push(LintFinding {
                    path: path.to_path_buf(),
                    line: c.line,
                    col: c.col,
                    rule: "agent-doc/unknown-component".to_string(),
                    severity: LintSeverity::Error,
                    message: format!("unknown agent-doc close tag `/{full}`"),
                    fix_hint: None,
                });
            }
        }
        CommentKind::Boundary { hex } => {
            // Canonical binary format: `<hex>` or `<hex>:<slug>`.
            // - hex: non-empty ASCII hex (binary emits 8 chars; older docs may have more)
            // - slug (optional): non-empty lowercase alphanumeric + dashes,
            //   produced by `new_boundary_id_with_summary`
            let (id, slug) = match hex.split_once(':') {
                Some((id, slug)) => (id, Some(slug)),
                None => (hex, None),
            };
            let id_ok = !id.is_empty() && id.chars().all(|c| c.is_ascii_hexdigit());
            let slug_ok = slug.is_none_or(|s| {
                !s.is_empty()
                    && s.chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
            });
            if !id_ok || !slug_ok {
                out.push(LintFinding {
                    path: path.to_path_buf(),
                    line: c.line,
                    col: c.col,
                    rule: "agent-doc/malformed-boundary".to_string(),
                    severity: LintSeverity::Error,
                    message: format!("agent:boundary expects `<hex>` or `<hex>:<slug>`, got `{hex}`"),
                    fix_hint: Some(
                        "use `<!-- agent:boundary:<hex> -->` or `<!-- agent:boundary:<hex>:<slug> -->` (slug: lowercase a-z0-9-)".to_string(),
                    ),
                });
            }
        }
        CommentKind::Marker { name } => {
            if !KNOWN_MARKERS.contains(&name) {
                // Bare single-token marker that isn't recognized — treat as
                // `Other` (ignored) to avoid false positives on prose
                // comments like `<!-- TODO -->`.
            }
        }
        CommentKind::PatchOpen { name, rest } => {
            // Only patch:exchange / patch:status are well-known today.
            if !matches!(name, "exchange" | "status" | "backlog" | "review") {
                out.push(LintFinding {
                    path: path.to_path_buf(),
                    line: c.line,
                    col: c.col,
                    rule: "agent-doc/unknown-patch-marker".to_string(),
                    severity: LintSeverity::Warning,
                    message: format!("unknown patch marker `patch:{name}`"),
                    fix_hint: Some(
                        "expected one of patch:exchange, patch:status, patch:backlog, patch:review"
                            .to_string(),
                    ),
                });
            }
            // Patch markers may carry attributes today only rarely; do the
            // malformed-attr check (no `=` is wrong) but not unknown-attr.
            check_attrs_malformed_only(path, c, &format!("patch:{name}"), rest, out);
        }
        CommentKind::PatchClose => {}
        CommentKind::Replace => {}
        CommentKind::Other => {}
    }
}

/// Parse and validate the attribute portion of an open-component tag.
fn check_attrs(path: &Path, c: &Comment, component: &str, rest: &str, out: &mut Vec<LintFinding>) {
    // Special-case `agent:queue`: allows a bare `auto` / `manual` token.
    if component == "agent:queue" {
        let tokens: Vec<&str> = rest.split_whitespace().collect();
        if tokens.is_empty() {
            return;
        }
        // Detect `mode=auto` / `mode=manual` (rule queue-mode-token).
        if let Some(first) = tokens.first()
            && first.starts_with("mode=")
        {
            out.push(LintFinding {
                path: path.to_path_buf(),
                line: c.line,
                col: c.col,
                rule: "agent-doc/queue-mode-token".to_string(),
                severity: LintSeverity::Error,
                message: format!(
                    "agent:queue expects a bare `auto` or `manual` token, got `{first}`"
                ),
                fix_hint: Some(
                    "use `<!-- agent:queue auto -->` or `<!-- agent:queue manual -->`".to_string(),
                ),
            });
            return;
        }
        // First token must be `auto`, `manual`, or a key=value attribute.
        let first = tokens[0];
        if matches!(first, "auto" | "manual") {
            // Skip the bare token; continue with the remainder as attrs.
            let rest_after = rest.trim_start();
            let rest_after = rest_after.strip_prefix(first).unwrap_or(rest_after);
            check_attr_pairs(path, c, component, rest_after, out);
            return;
        }
        // Otherwise fall through and treat as key=value attrs.
    }

    check_attr_pairs(path, c, component, rest, out);
}

fn check_attrs_malformed_only(
    path: &Path,
    c: &Comment,
    component: &str,
    rest: &str,
    out: &mut Vec<LintFinding>,
) {
    for tok in tokenize_attrs(rest) {
        if !tok.contains('=') {
            out.push(LintFinding {
                path: path.to_path_buf(),
                line: c.line,
                col: c.col,
                rule: "agent-doc/malformed-attr".to_string(),
                severity: LintSeverity::Error,
                message: format!("attribute `{tok}` on `{component}` is missing `=value`"),
                fix_hint: Some(format!("try `{tok}=<value>`")),
            });
        }
    }
}

fn check_attr_pairs(
    path: &Path,
    c: &Comment,
    component: &str,
    rest: &str,
    out: &mut Vec<LintFinding>,
) {
    let allowed = allowed_attrs(component);
    for tok in tokenize_attrs(rest) {
        match tok.split_once('=') {
            None => {
                // A bare `queue` token on `agent:backlog` / `agent:icebox` is the
                // default-append backlog→queue sync attribute (#backlog-queue-sync-attr),
                // and a bare `priority` token on backlog/icebox/queue is the
                // priority-ordering attribute (#backlog-priority-attribute) — neither
                // is a malformed `key=value`.
                if (tok == "queue" && is_backlog_sync_component(component))
                    || (tok == "priority" && is_priority_component(component))
                    || (matches!(tok.as_str(), "auto" | "manual") && component == "agent:queue")
                {
                    continue;
                }
                out.push(LintFinding {
                    path: path.to_path_buf(),
                    line: c.line,
                    col: c.col,
                    rule: "agent-doc/malformed-attr".to_string(),
                    severity: LintSeverity::Error,
                    message: format!("attribute `{tok}` on `{component}` is missing `=value`"),
                    fix_hint: Some(format!("try `{tok}=<value>`")),
                });
            }
            Some((k, v)) => {
                let key = k.trim();
                let val = v.trim().trim_matches('"');
                if val.is_empty() {
                    out.push(LintFinding {
                        path: path.to_path_buf(),
                        line: c.line,
                        col: c.col,
                        rule: "agent-doc/empty-attr-value".to_string(),
                        severity: LintSeverity::Error,
                        message: format!("attribute `{key}=` on `{component}` has empty value"),
                        fix_hint: Some(format!("provide a value: `{key}=<value>`")),
                    });
                    continue;
                }
                // Validate the backlog→queue sync mode value. The key is allowed
                // (see `allowed_attrs`); only an unrecognized mode is a finding.
                if key == "queue" && is_backlog_sync_component(component) {
                    if !matches!(val, "sync" | "append" | "prepend") {
                        out.push(LintFinding {
                            path: path.to_path_buf(),
                            line: c.line,
                            col: c.col,
                            rule: "agent-doc/invalid-attr-value".to_string(),
                            severity: LintSeverity::Warning,
                            message: format!(
                                "queue sync mode `{val}` on `{component}` is not recognized"
                            ),
                            fix_hint: Some(
                                "use `queue=sync`, `queue=append`, or `queue=prepend` (bare `queue` = append)".to_string(),
                            ),
                        });
                    }
                    continue;
                }
                if !allowed.contains(&key) {
                    out.push(LintFinding {
                        path: path.to_path_buf(),
                        line: c.line,
                        col: c.col,
                        rule: "agent-doc/unknown-attr".to_string(),
                        severity: LintSeverity::Warning,
                        message: format!("attribute `{key}` is not recognized on `{component}`"),
                        fix_hint: if allowed.is_empty() {
                            Some(format!("`{component}` accepts no attributes"))
                        } else {
                            Some(format!("recognized attributes: {}", allowed.join(", ")))
                        },
                    });
                }
            }
        }
    }
}

/// Components that accept the backlog→queue sync `queue` attribute
/// (`#backlog-queue-sync-attr`): bare `queue` (= append) or
/// `queue=sync|append|prepend`.
fn is_backlog_sync_component(component: &str) -> bool {
    matches!(component, "agent:backlog" | "agent:icebox")
}

/// Components that accept the bare `priority` ordering attribute
/// (`#backlog-priority-attribute`).
fn is_priority_component(component: &str) -> bool {
    matches!(component, "agent:backlog" | "agent:icebox" | "agent:queue")
}

/// Per-component allowed attribute keys.
fn allowed_attrs(component: &str) -> &'static [&'static str] {
    match component {
        "agent:status" => &["patch"],
        "agent:exchange" => &["patch"],
        "agent:backlog" => &["patch", "queue", "priority"],
        "agent:review" => &["patch"],
        "agent:done" => &["archive", "patch"],
        "agent:icebox" => &["patch", "queue", "priority"],
        "agent:queue" => &["patch", "priority", "auto", "preset"],
        _ => &[],
    }
}

/// Split an attribute string into whitespace-delimited tokens, treating
/// `key="quoted value"` as a single token.
fn tokenize_attrs(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut buf = String::new();
    let mut in_quote = false;
    for ch in s.chars() {
        if ch == '"' {
            in_quote = !in_quote;
            buf.push(ch);
            continue;
        }
        if ch.is_whitespace() && !in_quote {
            if !buf.is_empty() {
                out.push(std::mem::take(&mut buf));
            }
            continue;
        }
        buf.push(ch);
    }
    if !buf.is_empty() {
        out.push(buf);
    }
    out
}

/// Component pairing (open/close), duplicate-open, orphan-close.
fn check_pairing(path: &Path, comments: &[Comment], out: &mut Vec<LintFinding>) {
    // Stack of open components: (name, line, col).
    let mut stack: Vec<(String, usize, usize)> = Vec::new();
    // Track which components are currently open to detect duplicates within
    // the same outer scope.
    for c in comments {
        match classify(&c.body) {
            CommentKind::OpenComponent { name, .. } => {
                let full = format!("agent:{name}");
                if !is_known_component(&full) {
                    continue;
                }
                // Duplicate-component if already on stack without intervening
                // close. (We allow nesting of *different* components only;
                // real session docs do not nest the same component.)
                if stack.iter().any(|(n, _, _)| n == &full) {
                    out.push(LintFinding {
                        path: path.to_path_buf(),
                        line: c.line,
                        col: c.col,
                        rule: "agent-doc/duplicate-component".to_string(),
                        severity: LintSeverity::Error,
                        message: format!("`{full}` opened again before its previous close"),
                        fix_hint: Some(format!(
                            "close the prior `{full}` with `<!-- /{full} -->` first"
                        )),
                    });
                }
                stack.push((full, c.line, c.col));
            }
            CommentKind::CloseComponent { name } => {
                let full = format!("agent:{name}");
                if !is_known_component(&full) {
                    continue;
                }
                // Pop the matching open. If the top doesn't match, look for
                // the nearest matching open and treat intervening opens as
                // unclosed.
                match stack.iter().rposition(|(n, _, _)| n == &full) {
                    None => {
                        out.push(LintFinding {
							path: path.to_path_buf(),
							line: c.line,
							col: c.col,
							rule: "agent-doc/orphan-close".to_string(),
							severity: LintSeverity::Error,
							message: format!(
								"close tag `/{full}` has no matching open"
							),
							fix_hint: Some(format!(
								"add `<!-- {full} -->` earlier in the document, or remove this close"
							)),
						});
                    }
                    Some(idx) => {
                        // Anything above idx is an unclosed component between
                        // the matching open and this close — flag each.
                        for (name, line, col) in stack.drain(idx + 1..) {
                            out.push(LintFinding {
                                path: path.to_path_buf(),
                                line,
                                col,
                                rule: "agent-doc/unclosed-component".to_string(),
                                severity: LintSeverity::Error,
                                message: format!("`{name}` opened but never closed"),
                                fix_hint: Some(format!(
                                    "add `<!-- /{name} -->` before the close of `{full}`"
                                )),
                            });
                        }
                        stack.pop();
                    }
                }
            }
            _ => {}
        }
    }
    for (full, line, col) in stack {
        out.push(LintFinding {
            path: path.to_path_buf(),
            line,
            col,
            rule: "agent-doc/unclosed-component".to_string(),
            severity: LintSeverity::Error,
            message: format!("`{full}` opened but never closed"),
            fix_hint: Some(format!("add `<!-- /{full} -->` before end of file")),
        });
    }
}

/// Backlog id collisions + patch-marker-outside-cycle warning.
fn check_backlog_ids(path: &Path, text: &str, comments: &[Comment], out: &mut Vec<LintFinding>) {
    let backlog_spans = collect_component_spans(comments, "agent:backlog");
    let exchange_spans = collect_component_spans(comments, "agent:exchange");

    // Backlog id collision: only the LEADING `[#id]` of each backlog ITEM line
    // is an item-defining id. Bracket-id tokens that appear later in an item's
    // description (e.g. `do [#id]` example syntax, or a referenced sibling id)
    // are references, not definitions, and must not be parsed as colliding ids
    // (#free-text-queue-head-consume session: a `do [#id]` example in two item
    // descriptions falsely tripped this collision and blocked finalize).
    let item_id_re =
        regex::Regex::new(r"^\s*[-*]\s*\[[ xX/]\]\s*(\[#([A-Za-z0-9_\-]+)\])").unwrap();
    for (start, end) in &backlog_spans {
        let span = &text[*start..*end];
        let mut seen: std::collections::HashMap<String, (usize, usize)> = Default::default();
        let mut line_off = 0usize;
        for line in span.split_inclusive('\n') {
            if let Some(cap) = item_id_re.captures(line) {
                let id = cap.get(2).unwrap().as_str().to_string();
                let m = cap.get(1).unwrap();
                let global_off = start + line_off + m.start();
                let (line_no, col) = byte_to_line_col(text, global_off);
                if let Some((prev_line, _prev_col)) = seen.get(&id) {
                    out.push(LintFinding {
                        path: path.to_path_buf(),
                        line: line_no,
                        col,
                        rule: "agent-doc/backlog-id-collision".to_string(),
                        severity: LintSeverity::Error,
                        message: format!(
                            "backlog id `[#{id}]` duplicates earlier entry on line {prev_line}"
                        ),
                        fix_hint: Some(format!("rename one of the `[#{id}]` ids")),
                    });
                } else {
                    seen.insert(id, (line_no, col));
                }
            }
            line_off += line.len();
        }
    }

    // Patch-marker-outside-cycle warning.
    for c in comments {
        if let CommentKind::PatchOpen { name, .. } = classify(&c.body)
            && name == "exchange"
        {
            let inside = exchange_spans
                .iter()
                .any(|(s, e)| c.byte_start >= *s && c.byte_end <= *e);
            if !inside {
                out.push(LintFinding {
                    path: path.to_path_buf(),
                    line: c.line,
                    col: c.col,
                    rule: "agent-doc/patch-marker-outside-cycle".to_string(),
                    severity: LintSeverity::Warning,
                    message: "`patch:exchange` appears outside an `agent:exchange` block"
                        .to_string(),
                    fix_hint: Some(
                        "patch markers should sit inside the exchange they patch".to_string(),
                    ),
                });
            }
        }
    }
}

/// Find inclusive byte spans for each open/close pair of the given
/// component. Robust to unclosed components (skipped).
fn collect_component_spans(comments: &[Comment], component: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut open: Option<usize> = None;
    for c in comments {
        match classify(&c.body) {
            CommentKind::OpenComponent { name, .. } => {
                let full = format!("agent:{name}");
                if full == component && open.is_none() {
                    open = Some(c.byte_end);
                }
            }
            CommentKind::CloseComponent { name } => {
                let full = format!("agent:{name}");
                if full == component
                    && let Some(s) = open.take()
                {
                    out.push((s, c.byte_start));
                }
            }
            _ => {}
        }
    }
    out
}

/// FS-dependent: validate `archive=<path>` targets on `agent:done`.
fn check_archive_targets(path: &Path, comments: &[Comment], out: &mut Vec<LintFinding>) {
    let base = path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
    // Project root: walk up to first ancestor containing `.git` or stop at
    // the filesystem root. Fall back to current dir.
    let project_root = find_project_root(path);

    for c in comments {
        if let CommentKind::OpenComponent { name, rest } = classify(&c.body) {
            let full = format!("agent:{name}");
            if full != "agent:done" {
                continue;
            }
            for tok in tokenize_attrs(rest) {
                if let Some((k, v)) = tok.split_once('=') {
                    if k.trim() != "archive" {
                        continue;
                    }
                    let raw = v.trim().trim_matches('"').to_string();
                    if raw.is_empty() {
                        continue;
                    }
                    if !raw.ends_with(".done.md") {
                        out.push(LintFinding {
                            path: path.to_path_buf(),
                            line: c.line,
                            col: c.col,
                            rule: "agent-doc/done-archive-missing-target".to_string(),
                            severity: LintSeverity::Error,
                            message: format!(
                                "agent:done archive target `{raw}` does not end in `.done.md`"
                            ),
                            fix_hint: Some(
                                "rename the archive target to end with `.done.md`".to_string(),
                            ),
                        });
                    }
                    // Resolve relative to project root first, then to the
                    // session doc's directory.
                    let candidates: Vec<PathBuf> = vec![
                        project_root
                            .as_ref()
                            .map(|r| r.join(&raw))
                            .unwrap_or_default(),
                        base.join(&raw),
                        PathBuf::from(&raw),
                    ];
                    let exists = candidates
                        .iter()
                        .any(|p| !p.as_os_str().is_empty() && p.exists());
                    if !exists {
                        out.push(LintFinding {
                            path: path.to_path_buf(),
                            line: c.line,
                            col: c.col,
                            rule: "agent-doc/done-archive-missing-target".to_string(),
                            severity: LintSeverity::Error,
                            message: format!(
                                "agent:done archive target `{raw}` does not exist on disk"
                            ),
                            fix_hint: Some(format!("create `{raw}` or correct the path")),
                        });
                    }
                }
            }
        }
    }
}

fn find_project_root(path: &Path) -> Option<PathBuf> {
    let mut dir = if path.is_file() {
        path.parent()?.to_path_buf()
    } else {
        path.to_path_buf()
    };
    loop {
        if dir.join(".git").exists() {
            return Some(dir);
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// FS-dependent: every `agent:done` line's `[#id]` should also exist in the
/// `agent:backlog` block (or in a migrated-prefix marker). We approximate
/// "appears in backlog" by: id was present in any backlog span, OR the done
/// block opens with a `<!-- migrated -->` marker on its first non-blank
/// inner line.
fn check_done_ids_in_backlog(
    path: &Path,
    text: &str,
    comments: &[Comment],
    out: &mut Vec<LintFinding>,
) {
    let id_re = regex::Regex::new(r"\[#([A-Za-z0-9_\-]+)\]").unwrap();

    // Collect all backlog ids ever mentioned.
    let backlog_spans = collect_component_spans(comments, "agent:backlog");
    let mut backlog_ids: std::collections::HashSet<String> = Default::default();
    for (s, e) in &backlog_spans {
        for cap in id_re.captures_iter(&text[*s..*e]) {
            backlog_ids.insert(cap.get(1).unwrap().as_str().to_string());
        }
    }

    let done_spans = collect_component_spans(comments, "agent:done");
    for (s, e) in &done_spans {
        let span = &text[*s..*e];
        // Migrated marker shortcut: look for `<!-- migrated -->` or
        // `# migrated` early in the span.
        if span.contains("<!-- migrated -->") || span.starts_with("# migrated") {
            continue;
        }
        for cap in id_re.captures_iter(span) {
            let id = cap.get(1).unwrap().as_str().to_string();
            if !backlog_ids.contains(&id) {
                let m = cap.get(0).unwrap();
                let (line, col) = byte_to_line_col(text, s + m.start());
                out.push(LintFinding {
					path: path.to_path_buf(),
					line,
					col,
					rule: "agent-doc/done-id-not-in-backlog".to_string(),
					severity: LintSeverity::Warning,
					message: format!(
						"done item `[#{id}]` has no matching id in backlog"
					),
					fix_hint: Some(
						"either restore the backlog entry or add `<!-- migrated -->` to mark this done block as imported".to_string(),
					),
				});
            }
        }
    }
}

/// Format findings as text suitable for stdout.
pub fn format_findings_text(findings: &[LintFinding]) -> String {
    let mut s = String::new();
    for f in findings {
        let sev = match f.severity {
            LintSeverity::Error => "error",
            LintSeverity::Warning => "warning",
        };
        s.push_str(&format!(
            "{}:{}:{} {}: {} [{}]\n",
            f.path.display(),
            f.line,
            f.col,
            sev,
            f.message,
            f.rule,
        ));
        if let Some(hint) = &f.fix_hint {
            s.push_str(&format!("  hint: {hint}\n"));
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lint_str(text: &str) -> Vec<LintFinding> {
        lint_agent_doc(Path::new("test.md"), text, &AgentDocOptions::default())
    }

    #[test]
    fn malformed_done_archive_attr_fires() {
        // The motivating bug: missing `=` between `archive` and the path.
        let text = "<!-- agent:done archive tasks/x.done.md -->\n<!-- /agent:done -->\n";
        let findings = lint_str(text);
        let f = findings
            .iter()
            .find(|f| f.rule == "agent-doc/malformed-attr")
            .expect("expected malformed-attr finding");
        // Fix hint should point at the corrected key=value form.
        let hint = f.fix_hint.as_deref().unwrap_or("");
        assert!(
            hint.contains("=") && (hint.contains("archive") || hint.contains("tasks")),
            "fix hint should suggest key=value: got {hint}"
        );
    }

    #[test]
    fn unknown_component_fires() {
        let text = "<!-- agent:bogus -->\n<!-- /agent:bogus -->\n";
        let findings = lint_str(text);
        assert!(
            findings
                .iter()
                .any(|f| f.rule == "agent-doc/unknown-component")
        );
    }

    #[test]
    fn unclosed_component_fires() {
        let text = "<!-- agent:backlog -->\n- item\n";
        let findings = lint_str(text);
        assert!(
            findings
                .iter()
                .any(|f| f.rule == "agent-doc/unclosed-component")
        );
    }

    #[test]
    fn orphan_close_fires() {
        let text = "<!-- /agent:backlog -->\n";
        let findings = lint_str(text);
        assert!(findings.iter().any(|f| f.rule == "agent-doc/orphan-close"));
    }

    #[test]
    fn duplicate_component_fires() {
        let text = "<!-- agent:backlog -->\n<!-- agent:backlog -->\n<!-- /agent:backlog -->\n<!-- /agent:backlog -->\n";
        let findings = lint_str(text);
        assert!(
            findings
                .iter()
                .any(|f| f.rule == "agent-doc/duplicate-component")
        );
    }

    #[test]
    fn empty_attr_value_fires() {
        let text = "<!-- agent:done archive= -->\n<!-- /agent:done -->\n";
        let findings = lint_str(text);
        assert!(
            findings
                .iter()
                .any(|f| f.rule == "agent-doc/empty-attr-value")
        );
    }

    #[test]
    fn unknown_attr_fires() {
        let text = "<!-- agent:status nonsense=yes -->\n<!-- /agent:status -->\n";
        let findings = lint_str(text);
        assert!(findings.iter().any(|f| f.rule == "agent-doc/unknown-attr"));
    }

    #[test]
    fn queue_mode_token_fires() {
        let text = "<!-- agent:queue mode=auto -->\n<!-- /agent:queue -->\n";
        let findings = lint_str(text);
        assert!(
            findings
                .iter()
                .any(|f| f.rule == "agent-doc/queue-mode-token")
        );
    }

    #[test]
    fn queue_bare_auto_ok() {
        let text = "<!-- agent:queue auto -->\nx\n<!-- /agent:queue -->\n";
        let findings = lint_str(text);
        assert!(!findings.iter().any(|f| f.severity == LintSeverity::Error));
    }

    #[test]
    fn backlog_bare_queue_attr_ok() {
        // #backlog-queue-sync-attr: bare `queue` (= append) is valid on backlog.
        let text = "<!-- agent:backlog queue -->\n- [ ] [#a] one\n<!-- /agent:backlog -->\n";
        let findings = lint_str(text);
        assert!(
            findings.is_empty(),
            "bare `queue` on backlog must be clean: {findings:#?}"
        );
    }

    #[test]
    fn backlog_queue_mode_values_ok() {
        for mode in ["sync", "append", "prepend"] {
            let text = format!(
                "<!-- agent:backlog queue={mode} -->\n- [ ] [#a] one\n<!-- /agent:backlog -->\n"
            );
            let findings = lint_str(&text);
            assert!(
                findings.is_empty(),
                "queue={mode} on backlog must be clean: {findings:#?}"
            );
        }
    }

    #[test]
    fn icebox_queue_attr_ok() {
        let text = "<!-- agent:icebox queue=prepend -->\n- [ ] [#a] one\n<!-- /agent:icebox -->\n";
        let findings = lint_str(text);
        assert!(findings.is_empty(), "queue on icebox must be clean: {findings:#?}");
    }

    #[test]
    fn priority_attr_ok_on_backlog_icebox_queue() {
        // #backlog-priority-attribute: bare `priority` is valid on these markers.
        for text in [
            "<!-- agent:backlog priority -->\n- [ ] [#a] x\n<!-- /agent:backlog -->\n",
            "<!-- agent:backlog priority queue -->\n- [ ] [#a] x\n<!-- /agent:backlog -->\n",
            "<!-- agent:icebox priority -->\n- [ ] [#a] x\n<!-- /agent:icebox -->\n",
            "<!-- agent:queue priority -->\n- do [#a]\n<!-- /agent:queue -->\n",
        ] {
            let findings = lint_str(text);
            assert!(
                findings.is_empty(),
                "priority attr must be clean: {text}\n{findings:#?}"
            );
        }
    }

    #[test]
    fn queue_priority_and_auto_combine_in_any_order() {
        // The queue marker may carry both `priority` and `auto`; `auto`/`manual`
        // are valid bare tokens regardless of position (not just first).
        for text in [
            "<!-- agent:queue priority auto -->\n- do [#a]\n<!-- /agent:queue -->\n",
            "<!-- agent:queue auto priority -->\n- do [#a]\n<!-- /agent:queue -->\n",
            "<!-- agent:queue priority manual -->\n- do [#a]\n<!-- /agent:queue -->\n",
        ] {
            let findings = lint_str(text);
            assert!(
                !findings.iter().any(|f| f.severity == LintSeverity::Error),
                "priority+auto/manual must not error: {text}\n{findings:#?}"
            );
        }
    }

    #[test]
    fn backlog_invalid_queue_mode_warns() {
        let text = "<!-- agent:backlog queue=nope -->\n- [ ] [#a] one\n<!-- /agent:backlog -->\n";
        let findings = lint_str(text);
        let finding = findings
            .iter()
            .find(|f| f.rule == "agent-doc/invalid-attr-value")
            .expect("invalid queue mode should warn");
        assert_eq!(finding.severity, LintSeverity::Warning);
        assert!(finding.message.contains("nope"));
    }

    #[test]
    fn backlog_id_collision_fires() {
        let text =
            "<!-- agent:backlog -->\n- [ ] [#dup] one\n- [ ] [#dup] two\n<!-- /agent:backlog -->\n";
        let findings = lint_str(text);
        assert!(
            findings
                .iter()
                .any(|f| f.rule == "agent-doc/backlog-id-collision")
        );
    }

    #[test]
    fn backlog_id_collision_ignores_bracket_ids_in_descriptions() {
        // A `[#id]` appearing in two item DESCRIPTIONS (example syntax / a
        // referenced sibling id) is not a definition and must not collide.
        let text = concat!(
            "<!-- agent:backlog -->\n",
            "- [ ] [#alpha] uses `do [#id]` syntax in its description\n",
            "- [ ] [#beta] also mentions a `do [#id]` example here\n",
            "<!-- /agent:backlog -->\n",
        );
        let findings = lint_str(text);
        assert!(
            !findings
                .iter()
                .any(|f| f.rule == "agent-doc/backlog-id-collision"),
            "bracket-ids inside descriptions must not trip the collision rule: {findings:?}"
        );
    }

    #[test]
    fn patch_marker_outside_cycle_warns() {
        // `patch:exchange` not inside an agent:exchange block.
        let text = "<!-- patch:exchange -->\n";
        let findings = lint_str(text);
        assert!(
            findings
                .iter()
                .any(|f| f.rule == "agent-doc/patch-marker-outside-cycle")
        );
    }

    #[test]
    fn patch_marker_inside_cycle_ok() {
        let text = "<!-- agent:exchange patch=append -->\n<!-- patch:exchange -->\nx\n<!-- /agent:exchange -->\n";
        let findings = lint_str(text);
        assert!(
            !findings
                .iter()
                .any(|f| f.rule == "agent-doc/patch-marker-outside-cycle")
        );
    }

    #[test]
    fn done_archive_missing_target_fires_with_fs_checks() {
        let text = "<!-- agent:done archive=/nonexistent/path.done.md -->\n<!-- /agent:done -->\n";
        let opts = AgentDocOptions {
            fs_checks: true,
            ..Default::default()
        };
        let findings = lint_agent_doc(Path::new("test.md"), text, &opts);
        assert!(
            findings
                .iter()
                .any(|f| f.rule == "agent-doc/done-archive-missing-target")
        );
    }

    #[test]
    fn done_archive_wrong_extension_fires_with_fs_checks() {
        let text = "<!-- agent:done archive=tasks/x.txt -->\n<!-- /agent:done -->\n";
        let opts = AgentDocOptions {
            fs_checks: true,
            ..Default::default()
        };
        let findings = lint_agent_doc(Path::new("test.md"), text, &opts);
        assert!(
            findings
                .iter()
                .any(|f| f.rule == "agent-doc/done-archive-missing-target"
                    && f.message.contains(".done.md"))
        );
    }

    #[test]
    fn clean_doc_emits_no_findings() {
        let text = "\
<!-- agent:status patch=replace -->
ok
<!-- /agent:status -->

<!-- agent:exchange patch=append -->
<!-- agent:boundary:deadbeef -->
content
<!-- /agent:exchange -->

<!-- agent:queue auto -->
- do thing
<!-- /agent:queue -->

<!-- agent:backlog -->
- [ ] [#abc] hi
<!-- /agent:backlog -->
";
        let findings = lint_str(text);
        assert!(
            findings.is_empty(),
            "expected zero findings, got: {findings:#?}"
        );
    }

    #[test]
    fn looks_like_agent_doc_detects() {
        assert!(looks_like_agent_doc("<!-- agent:exchange patch=append -->"));
        assert!(!looks_like_agent_doc("# just a heading\n"));
    }

    #[test]
    fn legacy_agent_pending_component_accepted() {
        let text = "<!-- agent:pending -->\n- [ ] [#x] item\n<!-- /agent:pending -->\n";
        let findings = lint_str(text);
        assert!(
            !findings.iter().any(|f| f.rule == "agent-doc/unknown-component"),
            "legacy agent:pending should be accepted, got: {findings:#?}"
        );
    }

    #[test]
    fn legacy_pending_done_component_accepted() {
        let text = "<!-- agent:pending-done -->\nitem\n<!-- /agent:pending-done -->\n";
        let findings = lint_str(text);
        assert!(
            !findings.iter().any(|f| f.rule == "agent-doc/unknown-component"),
            "legacy agent:pending-done should be accepted, got: {findings:#?}"
        );
    }

    #[test]
    fn legacy_backlog_done_component_accepted() {
        let text = "<!-- agent:backlog-done -->\nitem\n<!-- /agent:backlog-done -->\n";
        let findings = lint_str(text);
        assert!(
            !findings.iter().any(|f| f.rule == "agent-doc/unknown-component"),
            "legacy agent:backlog-done should be accepted, got: {findings:#?}"
        );
    }

    #[test]
    fn unknown_component_still_fails() {
        let text = "<!-- agent:bogus -->\nitem\n<!-- /agent:bogus -->\n";
        let findings = lint_str(text);
        assert!(
            findings.iter().any(|f| f.rule == "agent-doc/unknown-component"),
            "truly unknown component should still fail, got: {findings:#?}"
        );
    }

    #[test]
    fn boundary_plain_hex_passes() {
        let text = "<!-- agent:exchange -->\n<!-- agent:boundary:a0cfeb34 -->\n<!-- /agent:exchange -->\n";
        let findings = lint_str(text);
        assert!(
            !findings.iter().any(|f| f.rule == "agent-doc/malformed-boundary"),
            "plain hex boundary should pass, got: {findings:#?}"
        );
    }

    #[test]
    fn boundary_hex_with_summary_slug_passes() {
        let text = "<!-- agent:exchange -->\n<!-- agent:boundary:a0cfeb34:boundary-fix -->\n<!-- /agent:exchange -->\n";
        let findings = lint_str(text);
        assert!(
            !findings.iter().any(|f| f.rule == "agent-doc/malformed-boundary"),
            "hex:slug boundary should pass, got: {findings:#?}"
        );
    }

    #[test]
    fn boundary_non_hex_id_fails() {
        let text = "<!-- agent:exchange -->\n<!-- agent:boundary:zzz -->\n<!-- /agent:exchange -->\n";
        let findings = lint_str(text);
        assert!(
            findings.iter().any(|f| f.rule == "agent-doc/malformed-boundary"),
            "non-hex id should fail, got: {findings:#?}"
        );
    }

    #[test]
    fn boundary_empty_id_fails() {
        let text = "<!-- agent:exchange -->\n<!-- agent:boundary: -->\n<!-- /agent:exchange -->\n";
        let findings = lint_str(text);
        assert!(
            findings.iter().any(|f| f.rule == "agent-doc/malformed-boundary"),
            "empty id should fail, got: {findings:#?}"
        );
    }

    #[test]
    fn boundary_uppercase_slug_fails() {
        let text = "<!-- agent:exchange -->\n<!-- agent:boundary:a0cfeb34:Boundary-Fix -->\n<!-- /agent:exchange -->\n";
        let findings = lint_str(text);
        assert!(
            findings.iter().any(|f| f.rule == "agent-doc/malformed-boundary"),
            "uppercase in slug should fail (binary lowercases), got: {findings:#?}"
        );
    }

    #[test]
    fn boundary_empty_slug_fails() {
        let text = "<!-- agent:exchange -->\n<!-- agent:boundary:a0cfeb34: -->\n<!-- /agent:exchange -->\n";
        let findings = lint_str(text);
        assert!(
            findings.iter().any(|f| f.rule == "agent-doc/malformed-boundary"),
            "trailing colon with empty slug should fail, got: {findings:#?}"
        );
    }

     #[test]
     fn rule_filter_restricts_findings() {
         let text = "<!-- agent:bogus -->\n<!-- agent:done archive bad -->\n<!-- /agent:done -->\n";
         let opts = AgentDocOptions {
             rule_filter: vec!["agent-doc/malformed-attr".to_string()],
             ..Default::default()
         };
         let findings = lint_agent_doc(Path::new("test.md"), text, &opts);
         assert!(
             findings
                 .iter()
                 .all(|f| f.rule == "agent-doc/malformed-attr")
         );
         assert!(!findings.is_empty());
     }

     #[test]
     fn html_comment_inside_inline_code_not_parsed() {
         let text = "<!-- agent:backlog priority queue -->\n- item with `<!-- agent:backlog queue -->` reference\n<!-- /agent:backlog -->\n";
         let findings = lint_str(text);
         assert!(
             findings.is_empty(),
             "backtick-wrapped component tag must not be treated as a real component: {findings:#?}"
         );
     }

     #[test]
     fn html_comment_inside_double_backtick_not_parsed() {
         let text = "<!-- agent:backlog priority queue -->\n- item with `` `<!-- agent:backlog queue -->` `` reference\n<!-- /agent:backlog -->\n";
         let findings = lint_str(text);
         assert!(
             findings.is_empty(),
             "double-backtick-wrapped component tag must not be treated as a real component: {findings:#?}"
         );
     }

     #[test]
     fn html_comment_inside_code_fence_not_parsed() {
         let text = "<!-- agent:exchange patch=append -->\n```\n<!-- agent:backlog queue -->\n```\n<!-- /agent:exchange -->\n";
         let findings = lint_str(text);
         assert!(
             findings.is_empty(),
             "fenced code block must not be treated as real components: {findings:#?}"
         );
     }

    #[test]
    fn backticks_in_queue_content_do_not_hide_component_tags() {
        let text = "\
<!-- agent:exchange patch=append -->
content
<!-- agent:boundary:abc12345 -->
<!-- /agent:exchange -->

<!-- agent:queue auto -->
- item with `unpaired backtick that would normally span
  across lines and cover the next component tag
- another `matched pair` here
<!-- /agent:queue -->

<!-- agent:review -->
- [/] [#test] description with `backticks` in it
<!-- /agent:review -->

<!-- agent:backlog -->
- [ ] [#abc] item
<!-- /agent:backlog -->
";
        let findings = lint_str(text);
        assert!(
            findings.is_empty(),
            "backticks in queue/review/backlog content must not hide component tags: {findings:#?}"
        );
    }
 }
