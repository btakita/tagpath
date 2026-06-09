use std::collections::{BTreeMap, BTreeSet};
use std::process::Command;

fn manifest_dir() -> &'static str {
    env!("CARGO_MANIFEST_DIR")
}

#[test]
fn benchmark_plan_matches_spec_budget_table() {
    let output = Command::new("bash")
        .arg("scripts/benchmark-current-performance.sh")
        .arg("--plan")
        .current_dir(manifest_dir())
        .output()
        .expect("run benchmark plan");
    assert!(
        output.status.success(),
        "benchmark --plan failed: status={:?}\nstderr={}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("plan utf8");
    let mut plan = BTreeMap::new();
    for line in stdout.lines().skip(1) {
        let mut parts = line.splitn(3, ',');
        let case = parts.next().expect("case").to_string();
        let budget = parts
            .next()
            .expect("budget")
            .parse::<u64>()
            .expect("numeric budget");
        let command = parts.next().expect("command").to_string();
        plan.insert(case, (budget, command));
    }

    let expected: BTreeSet<&str> = [
        "noop_sidecar_update",
        "one_changed_file_update",
        "full_reindex",
        "watch_save_burst",
        "mcp_indexed_project_query",
        "mcp_family_by_path_read",
    ]
    .into_iter()
    .collect();
    assert_eq!(
        plan.keys().map(String::as_str).collect::<BTreeSet<_>>(),
        expected,
        "benchmark plan cases changed without updating the test"
    );

    let spec = std::fs::read_to_string(format!("{}/SPEC.md", manifest_dir())).expect("SPEC.md");
    for (case, (budget, command)) in plan {
        assert!(
            spec.contains(&format!("`{case}`")),
            "SPEC missing case {case}"
        );
        assert!(
            spec.contains(&format!("<= {budget} ms")),
            "SPEC missing budget for {case}"
        );
        let first_word = command.split_whitespace().next().expect("command word");
        assert!(
            spec.contains(first_word),
            "SPEC missing command marker {first_word:?} for {case}"
        );
    }
}
