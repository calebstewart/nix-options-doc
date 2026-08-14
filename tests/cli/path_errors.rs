//! Process-level tests for #52: a `--path` that names a local path
//! discriminates correctly between "does not exist" and "failed to clone".

use crate::common::bin;
use std::process::Command;
use tempfile::TempDir;

/// Regression test for the bug itself (#52): a nonexistent `--path` used to
/// fall through to the git-clone branch and surface as "Failed to clone
/// repository", pointing the user at git instead of their argument. Asserts
/// the process exits non-zero, stderr names the path and says it does not
/// exist, stderr never mentions cloning, the error is printed via `Display`
/// (not `Debug` - same check as `unreadable_root_is_fatal_and_leaves_out_untouched`
/// in `fs_errors.rs`), and an existing `--out` file is left byte-for-byte
/// untouched.
#[test]
fn nonexistent_path_names_the_path_and_does_not_mention_cloning() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let missing = temp_dir.path().join("missing-dir");

    let out_path = temp_dir.path().join("out.md");
    std::fs::write(&out_path, "GOOD RESULT FROM AN EARLIER RUN")
        .expect("failed to seed good output file");

    let output = Command::new(bin())
        .env_remove("RUST_LOG")
        .arg("--path")
        .arg(&missing)
        .arg("--out")
        .arg(&out_path)
        .output()
        .expect("failed to run nix-options-doc");

    assert!(
        !output.status.success(),
        "expected a non-zero exit status, got {:?}; stderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("does not exist"),
        "expected 'does not exist' on stderr, got: {stderr:?}"
    );
    assert!(
        stderr.contains(&missing.to_string_lossy().to_string()),
        "expected the missing path to be named on stderr, got: {stderr:?}"
    );
    assert!(
        !stderr.contains("clone"),
        "a missing local path must not be reported as a clone failure, got: {stderr:?}"
    );
    assert!(
        !stderr.contains("Os {"),
        "expected the error to be printed via Display, not Debug, got: {stderr:?}"
    );

    let contents = std::fs::read_to_string(&out_path).expect("output file should still exist");
    assert_eq!(
        contents, "GOOD RESULT FROM AN EARLIER RUN",
        "a nonexistent --path must leave an existing --out file untouched"
    );
}
