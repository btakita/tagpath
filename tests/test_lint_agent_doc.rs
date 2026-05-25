//! Integration tests for the agent-doc lint dialect.

use std::path::Path;
use tagpath::lint::{AgentDocOptions, LintSeverity, lint_agent_doc};

fn lint(text: &str) -> Vec<tagpath::lint::LintFinding> {
    lint_agent_doc(Path::new("doc.md"), text, &AgentDocOptions::default())
}

#[test]
fn rule_unknown_component_good_and_bad() {
    // good
    let good = "<!-- agent:backlog -->\n<!-- /agent:backlog -->\n";
    assert!(
        !lint(good)
            .iter()
            .any(|f| f.rule == "agent-doc/unknown-component")
    );
    // bad
    let bad = "<!-- agent:imaginary -->\n<!-- /agent:imaginary -->\n";
    assert!(
        lint(bad)
            .iter()
            .any(|f| f.rule == "agent-doc/unknown-component")
    );
}

#[test]
fn rule_unclosed_component() {
    let good = "<!-- agent:backlog -->\n<!-- /agent:backlog -->\n";
    assert!(
        !lint(good)
            .iter()
            .any(|f| f.rule == "agent-doc/unclosed-component")
    );
    let bad = "<!-- agent:backlog -->\nitem\n";
    assert!(
        lint(bad)
            .iter()
            .any(|f| f.rule == "agent-doc/unclosed-component")
    );
}

#[test]
fn rule_orphan_close() {
    let good = "<!-- agent:status patch=replace -->\n<!-- /agent:status -->\n";
    assert!(
        !lint(good)
            .iter()
            .any(|f| f.rule == "agent-doc/orphan-close")
    );
    let bad = "<!-- /agent:status -->\n";
    assert!(lint(bad).iter().any(|f| f.rule == "agent-doc/orphan-close"));
}

#[test]
fn rule_duplicate_component() {
    let good = "<!-- agent:backlog -->\nx\n<!-- /agent:backlog -->\n<!-- agent:backlog -->\ny\n<!-- /agent:backlog -->\n";
    assert!(
        !lint(good)
            .iter()
            .any(|f| f.rule == "agent-doc/duplicate-component")
    );
    let bad = "<!-- agent:backlog -->\n<!-- agent:backlog -->\n<!-- /agent:backlog -->\n<!-- /agent:backlog -->\n";
    assert!(
        lint(bad)
            .iter()
            .any(|f| f.rule == "agent-doc/duplicate-component")
    );
}

#[test]
fn rule_malformed_attr_regression_for_done_archive() {
    // This is the EXACT bug class that motivated the feature.
    let bad = "<!-- agent:done archive tasks/x.done.md -->\n<!-- /agent:done -->\n";
    let findings = lint(bad);
    let f = findings
        .iter()
        .find(|f| f.rule == "agent-doc/malformed-attr")
        .expect("malformed-attr should fire on `archive PATH` (missing `=`)");
    let hint = f.fix_hint.as_deref().unwrap_or("");
    assert!(
        hint.contains('='),
        "fix hint must suggest key=value form, got: {hint}"
    );

    // Good form should not fire.
    let good = "<!-- agent:done archive=tasks/x.done.md -->\n<!-- /agent:done -->\n";
    assert!(
        !lint(good)
            .iter()
            .any(|f| f.rule == "agent-doc/malformed-attr")
    );
}

#[test]
fn rule_empty_attr_value() {
    let good = "<!-- agent:done archive=x.done.md -->\n<!-- /agent:done -->\n";
    assert!(
        !lint(good)
            .iter()
            .any(|f| f.rule == "agent-doc/empty-attr-value")
    );
    let bad = "<!-- agent:done archive= -->\n<!-- /agent:done -->\n";
    assert!(
        lint(bad)
            .iter()
            .any(|f| f.rule == "agent-doc/empty-attr-value")
    );
    let bad2 = "<!-- agent:done archive=\"\" -->\n<!-- /agent:done -->\n";
    assert!(
        lint(bad2)
            .iter()
            .any(|f| f.rule == "agent-doc/empty-attr-value")
    );
}

#[test]
fn rule_unknown_attr() {
    let good = "<!-- agent:status patch=replace -->\n<!-- /agent:status -->\n";
    assert!(
        !lint(good)
            .iter()
            .any(|f| f.rule == "agent-doc/unknown-attr")
    );
    let bad = "<!-- agent:status flim=flam -->\n<!-- /agent:status -->\n";
    assert!(lint(bad).iter().any(|f| f.rule == "agent-doc/unknown-attr"));
}

#[test]
fn rule_queue_mode_token() {
    let good = "<!-- agent:queue auto -->\nx\n<!-- /agent:queue -->\n";
    assert!(
        !lint(good)
            .iter()
            .any(|f| f.rule == "agent-doc/queue-mode-token")
    );
    let bad = "<!-- agent:queue mode=manual -->\nx\n<!-- /agent:queue -->\n";
    assert!(
        lint(bad)
            .iter()
            .any(|f| f.rule == "agent-doc/queue-mode-token")
    );
}

#[test]
fn rule_done_archive_missing_target_with_fs_checks() {
    let opts = AgentDocOptions {
        fs_checks: true,
        ..Default::default()
    };

    // Missing file
    let bad = "<!-- agent:done archive=/no/such/file.done.md -->\n<!-- /agent:done -->\n";
    let findings = lint_agent_doc(Path::new("doc.md"), bad, &opts);
    assert!(
        findings
            .iter()
            .any(|f| f.rule == "agent-doc/done-archive-missing-target")
    );

    // Wrong extension
    let bad2 = "<!-- agent:done archive=tasks/x.txt -->\n<!-- /agent:done -->\n";
    let findings2 = lint_agent_doc(Path::new("doc.md"), bad2, &opts);
    assert!(
        findings2
            .iter()
            .any(|f| f.rule == "agent-doc/done-archive-missing-target")
    );
}

#[test]
fn rule_patch_marker_outside_cycle() {
    let good = "<!-- agent:exchange patch=append -->\n<!-- patch:exchange -->\nx\n<!-- /agent:exchange -->\n";
    assert!(
        !lint(good)
            .iter()
            .any(|f| f.rule == "agent-doc/patch-marker-outside-cycle")
    );
    let bad = "<!-- patch:exchange -->\n";
    assert!(
        lint(bad)
            .iter()
            .any(|f| f.rule == "agent-doc/patch-marker-outside-cycle")
    );
}

#[test]
fn rule_backlog_id_collision() {
    let good = "<!-- agent:backlog -->\n- [ ] [#a] one\n- [ ] [#b] two\n<!-- /agent:backlog -->\n";
    assert!(
        !lint(good)
            .iter()
            .any(|f| f.rule == "agent-doc/backlog-id-collision")
    );
    let bad =
        "<!-- agent:backlog -->\n- [ ] [#dup] one\n- [ ] [#dup] two\n<!-- /agent:backlog -->\n";
    assert!(
        lint(bad)
            .iter()
            .any(|f| f.rule == "agent-doc/backlog-id-collision")
    );
}

#[test]
fn rule_done_id_not_in_backlog_with_fs_checks() {
    let opts = AgentDocOptions {
        fs_checks: true,
        ..Default::default()
    };
    let good = "<!-- agent:backlog -->\n- [ ] [#abc] item\n<!-- /agent:backlog -->\n\
<!-- agent:done -->\n- [x] [#abc] done\n<!-- /agent:done -->\n";
    let findings = lint_agent_doc(Path::new("doc.md"), good, &opts);
    assert!(
        !findings
            .iter()
            .any(|f| f.rule == "agent-doc/done-id-not-in-backlog")
    );

    let bad = "<!-- agent:backlog -->\n- [ ] [#xyz] item\n<!-- /agent:backlog -->\n\
<!-- agent:done -->\n- [x] [#orphan] done\n<!-- /agent:done -->\n";
    let findings = lint_agent_doc(Path::new("doc.md"), bad, &opts);
    assert!(
        findings
            .iter()
            .any(|f| f.rule == "agent-doc/done-id-not-in-backlog")
    );
}

#[test]
fn round_trip_real_session_doc_is_clean() {
    // Round-trip against known-good doc. Skip if not present.
    let p = Path::new("/home/brian/work/btakita/agent-loop/tasks/software/tsift.md");
    if !p.exists() {
        eprintln!("skipping: {} not present", p.display());
        return;
    }
    let text = std::fs::read_to_string(p).unwrap();
    let findings = lint_agent_doc(p, &text, &AgentDocOptions::default());
    let errors: Vec<_> = findings
        .iter()
        .filter(|f| matches!(f.severity, LintSeverity::Error))
        .collect();
    assert!(
        errors.is_empty(),
        "expected zero errors on tsift.md, got: {errors:#?}"
    );
}
