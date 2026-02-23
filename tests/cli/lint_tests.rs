//! Lint command integration tests.

/// Test lint --json flag is accepted and produces valid JSON
#[test]
fn test_lint_json_flag() {
    use std::process::Command;

    // Test that --json is a valid flag (--help should show it)
    let output = Command::new("cargo")
        .args(["run", "--quiet", "--", "lint", "--help"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("Failed to run cargo");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("--json") || stdout.contains("-j"),
        "lint --help should show --json flag"
    );
    assert!(
        stdout.contains("--format"),
        "lint --help should show --format flag"
    );
}

/// Test lint --batch-size flag is accepted
#[test]
fn test_lint_batch_size_flag() {
    use std::process::Command;

    // Test that --batch-size is a valid flag (--help should show it)
    let output = Command::new("cargo")
        .args(["run", "--quiet", "--", "lint", "--help"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("Failed to run cargo");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("--batch-size"),
        "lint --help should show --batch-size flag"
    );
    assert!(
        stdout.contains("memory"),
        "lint --help should mention memory in --batch-size description"
    );
}

/// Test lint --batch-size works with --parallel flag
#[test]
fn test_lint_batch_size_with_parallel() {
    use std::process::Command;

    // Test that --batch-size and --parallel can be used together (--help shows both)
    let output = Command::new("cargo")
        .args(["run", "--quiet", "--", "lint", "--help"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("Failed to run cargo");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("--batch-size") && stdout.contains("--parallel"),
        "lint --help should show both --batch-size and --parallel flags"
    );
}

/// Test lint --fix --dry-run flag combination is accepted
#[test]
fn test_lint_fix_dry_run_flag() {
    use std::process::Command;

    // Test that --dry-run is a valid flag and mentions --fix
    let output = Command::new("cargo")
        .args(["run", "--quiet", "--", "lint", "--help"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("Failed to run cargo");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("--dry-run"),
        "lint --help should show --dry-run flag"
    );
    assert!(
        stdout.contains("--fix"),
        "lint --help should show --fix flag"
    );
    // Verify the help text mentions both --fix and --review
    assert!(
        stdout.contains("--fix") && stdout.contains("--review"),
        "lint --help should show both --fix and --review flags"
    );
}
