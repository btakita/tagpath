use std::process::Command;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_tagpath")
}

fn run_ok(args: &[&str]) -> String {
    let output = Command::new(bin())
        .args(args)
        .output()
        .expect("run tagpath");
    assert!(
        output.status.success(),
        "tagpath {args:?} failed: status={:?}\nstderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("stdout utf8")
}

#[test]
fn parse_cli_output_stays_available_from_facade_binary() {
    let stdout = run_ok(&["parse", "create_user_profile"]);
    for expected in [
        "name:       create_user_profile",
        "convention: snake_case",
        "tags:       [create, user, profile]",
        "role:       factory",
        "canonical:  create_user_profile",
    ] {
        assert!(
            stdout.contains(expected),
            "stdout missing `{expected}`:\n{stdout}"
        );
    }
}

#[test]
fn core_backed_cli_commands_stay_available_from_facade_binary() {
    let alias_stdout = run_ok(&["alias", "person_name", "--convention", "camelCase"]);
    assert!(alias_stdout.contains("camelCase:"));
    assert!(alias_stdout.contains("personName"));

    let family_stdout = run_ok(&["family", "auth0__user__validate"]);
    assert!(family_stdout.contains("canonical: auth0_user_validate"));
    assert!(family_stdout.contains("role:      validator"));

    let prose_stdout = run_ok(&["prose", "create_user_profile"]);
    assert_eq!(prose_stdout.trim(), "Creates a user profile");

    let query_stdout = run_ok(&["normalize-query", "Find raw_symbol output"]);
    assert!(query_stdout.contains("raw\tweight:2.0"));
    assert!(query_stdout.contains("symbol\tweight:2.0"));
}
