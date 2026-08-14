//! The output-writing contract from #9: every run writes `--out`.

use crate::common::{bin, temp_dir_with_nix_file};
use std::process::Command;

/// Regression test for #9 (the bug itself): a path whose only `.nix` file
/// declares no options used to exit `0` without writing `--out` at all,
/// leaving any stale file from a previous run silently in place, and the
/// "No NixOS options found" warning was invisible by default (`env_logger`'s
/// default filter is `error`). This asserts all three are fixed: the
/// process still exits `0` (an option-less tree is not an error - the fix
/// is not "make it fail"), the output file is overwritten with a
/// well-formed empty document, and the warning appears on stderr without
/// any `RUST_LOG` set.
#[test]
fn no_options_found_still_writes_output_and_warns() {
    let temp_dir =
        temp_dir_with_nix_file("{ config, ... }: {\n  config.services.foo.enable = true;\n}\n");
    let out_path = temp_dir.path().join("out.md");
    std::fs::write(&out_path, "stale").expect("failed to seed stale output file");

    let output = Command::new(bin())
        .env_remove("RUST_LOG")
        .arg("--path")
        .arg(temp_dir.path())
        .arg("--out")
        .arg(&out_path)
        .output()
        .expect("failed to run nix-options-doc");

    assert!(
        output.status.success(),
        "expected exit 0, got {:?}; stderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );

    let contents = std::fs::read_to_string(&out_path).expect("output file should exist");
    assert!(
        !contents.contains("stale"),
        "expected the stale file contents to be overwritten, got: {contents:?}"
    );
    assert!(
        contents.contains("# NixOS Module Options"),
        "expected a well-formed empty Markdown document, got: {contents:?}"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("No NixOS options found"),
        "expected the warning to be visible on stderr by default, got: {stderr:?}"
    );
}

/// Regression test for #9's second early-return: filters that match no
/// options used to exit `0` without writing `--out`. This asserts the
/// output file is instead overwritten with a valid empty JSON array and
/// the "no options match" warning is visible on stderr by default.
#[test]
fn filters_matching_nothing_still_write_output() {
    let temp_dir = temp_dir_with_nix_file(
        "{ lib, ... }: {\n  options.services.foo.enable = lib.mkOption { type = lib.types.bool; description = \"x\"; };\n}\n",
    );
    let out_path = temp_dir.path().join("out.json");
    std::fs::write(&out_path, "stale").expect("failed to seed stale output file");

    let output = Command::new(bin())
        .env_remove("RUST_LOG")
        .arg("--path")
        .arg(temp_dir.path())
        .arg("--filter-by-prefix")
        .arg("zzz.nonexistent")
        .arg("--format")
        .arg("json")
        .arg("--out")
        .arg(&out_path)
        .output()
        .expect("failed to run nix-options-doc");

    assert!(
        output.status.success(),
        "expected exit 0, got {:?}; stderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );

    let contents = std::fs::read_to_string(&out_path).expect("output file should exist");
    assert_eq!(contents.trim(), "[]");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("No options match the specified filters"),
        "expected the warning to be visible on stderr by default, got: {stderr:?}"
    );
}

/// Guards against collateral damage from the `!options.is_empty() &&` guard
/// added around the second warning: a normal run that actually finds and
/// keeps an option must still succeed, produce the right JSON, and emit
/// neither empty-result warning.
#[test]
fn options_found_path_is_unchanged() {
    let temp_dir = temp_dir_with_nix_file(
        "{ lib, ... }: {\n  options.services.foo.enable = lib.mkOption { type = lib.types.bool; description = \"x\"; };\n}\n",
    );
    let out_path = temp_dir.path().join("out.json");

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
    assert!(!stderr.contains("No NixOS options found"));
    assert!(!stderr.contains("No options match"));
}
