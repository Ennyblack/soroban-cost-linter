use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use tempfile::tempdir;

fn get_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_cargo-cost-lint"))
}

#[test]
fn test_missing_dylint_preflight() {
    let output = Command::new(get_bin()).env("PATH", "").output().unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("cargo-dylint is not installed"));
    assert!(stderr.contains("cargo install cargo-dylint dylint-link"));
}

#[test]
fn test_flag_forwarding_and_budget() {
    let dir = tempdir().unwrap();
    let cargo_mock_path = dir
        .path()
        .join(if cfg!(windows) { "cargo.bat" } else { "cargo" });

    if cfg!(windows) {
        fs::write(&cargo_mock_path, "@echo off\necho %* >> args.txt").unwrap();
    } else {
        fs::write(&cargo_mock_path, "#!/bin/sh\necho \"$@\" >> args.txt").unwrap();
        // Need to make executable on unix, but we can skip that if only testing on Windows,
        // or just rely on Windows for this local run.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&cargo_mock_path, fs::Permissions::from_mode(0o755)).unwrap();
        }
    }

    fs::write(
        dir.path().join("budget.toml"),
        "[lints]\nsoroban_storage_in_loop = \"warn\"\n",
    )
    .unwrap();

    let output = Command::new(get_bin())
        .env("PATH", dir.path())
        .current_dir(dir.path())
        .arg("--workspace")
        .arg("--manifest-path")
        .arg("foo/Cargo.toml")
        .arg("-p")
        .arg("bar")
        .arg("--")
        .arg("--extra")
        .output()
        .unwrap();

    let args_txt = fs::read_to_string(dir.path().join("args.txt")).unwrap();

    // We expect 2 lines, one for `dylint --version`, one for `dylint --lib ...`
    assert!(args_txt.contains("dylint"));
    assert!(args_txt.contains("--lib"));
    assert!(args_txt.contains("soroban_cost_lints"));
    assert!(args_txt.contains("--workspace"));
    assert!(args_txt.contains("--manifest-path"));
    assert!(args_txt.contains("foo/Cargo.toml"));
    assert!(args_txt.contains("--package"));
    assert!(args_txt.contains("bar"));
    assert!(args_txt.contains("--extra"));
}

#[test]
fn test_json_output() {
    let bin_path = env!("CARGO_BIN_EXE_cargo-cost-lint");

    // Construct path to the fixture directory from the workspace
    let mut fixture_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    fixture_dir.pop(); // Go up to workspace root
    fixture_dir.push("soroban_cost_lints");
    fixture_dir.push("test_fixtures");
    fixture_dir.push("real_sdk");

    assert!(
        fixture_dir.exists(),
        "Fixture directory not found: {:?}",
        fixture_dir
    );

    // Find the workspace target directory dynamically based on the binary path
    let mut target_dir = PathBuf::from(env!("CARGO_BIN_EXE_cargo-cost-lint"));
    target_dir.pop(); // Remove the binary name, leaving the profile directory (e.g., target/debug)

    // Build the soroban_cost_lints cdylib first from its own directory so it picks up .cargo/config.toml
    let mut lint_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    lint_dir.pop();
    lint_dir.push("soroban_cost_lints");

    let status = Command::new(env!("CARGO"))
        .arg("build")
        .current_dir(&lint_dir)
        .status()
        .expect("Failed to build soroban_cost_lints");
    assert!(status.success(), "Failed to build soroban_cost_lints");

    // Run the built wrapper binary in the fixture directory with --format json
    let output = Command::new(bin_path)
        .arg("--format")
        .arg("json")
        .current_dir(fixture_dir)
        .env("DYLINT_LIBRARY_PATH", target_dir)
        .output()
        .expect("Failed to execute cargo-cost-lint");

    let stdout_str = String::from_utf8(output.stdout).expect("Stdout is not valid UTF-8");
    let lines: Vec<&str> = stdout_str.lines().filter(|l| !l.is_empty()).collect();

    let stderr_str = String::from_utf8(output.stderr).expect("Stderr is not valid UTF-8");
    if lines.is_empty() {
        println!("Stderr output:\n{}", stderr_str);
    }
    // The fixture should have some lint violations.
    assert!(
        !lines.is_empty(),
        "Expected JSON output, but stdout was empty. Stderr: {}",
        stderr_str
    );

    let mut found_storage_in_loop = false;
    for line in lines {
        // Assert that the line is valid JSON conforming to our schema
        let json: serde_json::Value =
            serde_json::from_str(line).expect("Output line is not valid JSON");

        assert!(json.get("name").is_some(), "JSON missing 'name' field");
        assert!(json.get("level").is_some(), "JSON missing 'level' field");
        assert!(json.get("file").is_some(), "JSON missing 'file' field");
        assert!(json.get("span").is_some(), "JSON missing 'span' field");
        assert!(
            json.get("message").is_some(),
            "JSON missing 'message' field"
        );

        if json["name"] == "soroban_storage_in_loop" {
            found_storage_in_loop = true;
        }
    }

    assert!(
        found_storage_in_loop,
        "Expected to find 'soroban_storage_in_loop' lint, but it was not present"
    );
}
