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
