//! Unix-only filesystem permission error handling. This whole module is
//! gated by `#[cfg(unix)] mod fs_errors;` in `main.rs`, so the tests here do
//! not repeat the attribute themselves.

use crate::common::{bin, permissions_are_enforced, set_mode, temp_dir_with_nix_file};
use std::process::Command;
use tempfile::TempDir;

/// Regression test for the bug in #41: an unreadable traversal *root* used to
/// be swallowed as an ordinary traversal warning, so `collect_options`
/// returned `Ok(vec![])`, which `main` cannot distinguish from a tree that
/// genuinely declares no options. That empty result then overwrote `--out`
/// with an empty document and exited `0`, destroying a good result from an
/// earlier run. This asserts the fix: the process exits non-zero, the
/// existing `--out` file is left byte-for-byte untouched, and the failure is
/// reported via `Display` (not `Debug` - see `run`/`main` in `src/main.rs`).
#[test]
fn unreadable_root_is_fatal_and_leaves_out_untouched() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let modules = temp_dir.path().join("modules");
    std::fs::create_dir(&modules).expect("failed to create modules dir");
    std::fs::write(
        modules.join("module.nix"),
        "{ lib, ... }: {\n  options.services.foo.enable = lib.mkOption { type = lib.types.bool; description = \"x\"; };\n}\n",
    )
    .expect("failed to write module.nix");

    let out_path = temp_dir.path().join("out.md");
    std::fs::write(&out_path, "GOOD RESULT FROM AN EARLIER RUN")
        .expect("failed to seed good output file");

    set_mode(&modules, 0o000);
    if !permissions_are_enforced(&modules) {
        set_mode(&modules, 0o755);
        eprintln!("skipping: permissions are not enforced for this process (root?)");
        return;
    }

    let output = Command::new(bin())
        .env_remove("RUST_LOG")
        .arg("--path")
        .arg(&modules)
        .arg("--out")
        .arg(&out_path)
        .output()
        .expect("failed to run nix-options-doc");

    set_mode(&modules, 0o755);

    assert!(
        !output.status.success(),
        "expected a non-zero exit status, got {:?}; stderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );

    let contents = std::fs::read_to_string(&out_path).expect("output file should still exist");
    assert_eq!(
        contents, "GOOD RESULT FROM AN EARLIER RUN",
        "an unreadable root must leave an existing --out file untouched"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Permission denied"),
        "expected the failure reason on stderr, got: {stderr:?}"
    );
    assert!(
        !stderr.contains("Os {"),
        "expected the error to be printed via Display, not Debug, got: {stderr:?}"
    );
}

/// Regression test against the obvious over-fix: treating *any* traversal
/// error as fatal, which would break the documented graceful degradation for
/// an unreadable subdirectory below the root. Asserts the run still succeeds,
/// still documents the readable part of the tree, and still warns.
#[test]
fn unreadable_subdirectory_is_skipped_not_fatal() {
    let temp_dir = temp_dir_with_nix_file(
        "{ lib, ... }: {\n  options.services.foo.enable = lib.mkOption { type = lib.types.bool; description = \"x\"; };\n}\n",
    );
    let locked = temp_dir.path().join("locked");
    std::fs::create_dir(&locked).expect("failed to create locked dir");
    std::fs::write(
        locked.join("hidden.nix"),
        "{ lib, ... }: {\n  options.services.bar.enable = lib.mkOption { type = lib.types.bool; description = \"y\"; };\n}\n",
    )
    .expect("failed to write hidden.nix");

    let out_path = temp_dir.path().join("out.json");

    set_mode(&locked, 0o000);
    if !permissions_are_enforced(&locked) {
        set_mode(&locked, 0o755);
        eprintln!("skipping: permissions are not enforced for this process (root?)");
        return;
    }

    let output = Command::new(bin())
        .env_remove("RUST_LOG")
        .arg("--path")
        .arg(temp_dir.path())
        .arg("--format")
        .arg("json")
        .arg("--out")
        .arg(&out_path)
        .output()
        .expect("failed to run nix-options-doc");

    set_mode(&locked, 0o755);

    assert!(
        output.status.success(),
        "expected exit 0, got {:?}; stderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );

    let contents = std::fs::read_to_string(&out_path).expect("output file should exist");
    let parsed: serde_json::Value =
        serde_json::from_str(&contents).expect("output should be valid JSON");
    let array = parsed.as_array().expect("output should be a JSON array");
    assert_eq!(array.len(), 1);
    assert_eq!(array[0]["name"], "options.services.foo.enable");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("skipping directory"),
        "expected the skip warning on stderr, got: {stderr:?}"
    );
}
