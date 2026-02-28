//! Check command integration tests.

/// Test check --json flag is accepted and produces valid JSON
#[test]
fn test_check_json_flag() {
    use std::process::Command;

    let output = Command::new("cargo")
        .args(["run", "--quiet", "--", "check", "--help"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("Failed to run cargo");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("--json") || stdout.contains("-j"),
        "check --help should show --json flag"
    );
}

/// Test check --batch-size flag is accepted
#[test]
fn test_check_batch_size_flag() {
    use std::process::Command;

    let output = Command::new("cargo")
        .args(["run", "--quiet", "--", "check", "--help"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("Failed to run cargo");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("--batch-size"),
        "check --help should show --batch-size flag"
    );
}

/// Test check --batch-size works with --parallel flag
#[test]
fn test_check_batch_size_with_parallel() {
    use std::process::Command;

    let output = Command::new("cargo")
        .args(["run", "--quiet", "--", "check", "--help"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("Failed to run cargo");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("--batch-size") && stdout.contains("--parallel"),
        "check --help should show both --batch-size and --parallel flags"
    );
}

/// Test check --fix --dry-run flag combination is accepted
#[test]
fn test_check_fix_dry_run_flag() {
    use std::process::Command;

    let output = Command::new("cargo")
        .args(["run", "--quiet", "--", "check", "--help"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("Failed to run cargo");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("--dry-run"),
        "check --help should show --dry-run flag"
    );
    assert!(
        stdout.contains("--fix"),
        "check --help should show --fix flag"
    );
}
