//! Process-level CLI tests for #9: warnings are invisible by default, and
//! the "no options found" / "no options match the filters" paths used to
//! `return Ok(())` before ever calling `generate_doc`, so `--out` was left
//! untouched (and, on repeat runs, a stale file from an earlier invocation
//! silently persisted).
//!
//! These behaviors only exist at the level of `main` - process exit status,
//! whether `--out` gets written, and default log visibility - which the
//! `include!`d unit tests in `src/tests/tests.rs` cannot observe because
//! they call library functions directly and never spawn the binary. Hence
//! this is a normal (non-`include!`d) integration test target that builds
//! and runs the actual `nix-options-doc` binary via
//! `env!("CARGO_BIN_EXE_nix-options-doc")`.

use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

/// Creates a fresh temp directory containing a single `.nix` file with the
/// given contents, and returns the directory (kept alive for the caller).
fn temp_dir_with_nix_file(contents: &str) -> TempDir {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    std::fs::write(temp_dir.path().join("module.nix"), contents)
        .expect("failed to write temp nix file");
    temp_dir
}

/// Path to the built `nix-options-doc` binary under test.
fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_nix-options-doc"))
}

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

/// Regression test for a plausible wrong fix: hardcoding the log level
/// (e.g. `Builder::new().filter_level(LevelFilter::Warn)`) instead of using
/// `default_filter_or`, which would break the documented `RUST_LOG`
/// override workflow. Asserts `RUST_LOG=error` still suppresses the `warn`-
/// level "no options found" message.
#[test]
fn rust_log_still_overrides_the_default_filter() {
    let temp_dir =
        temp_dir_with_nix_file("{ config, ... }: {\n  config.services.foo.enable = true;\n}\n");
    let out_path = temp_dir.path().join("out.md");

    let output = Command::new(bin())
        .env("RUST_LOG", "error")
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

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("No NixOS options found"),
        "RUST_LOG=error should suppress the warn-level message, got: {stderr:?}"
    );
}

/// Regression test for #3: crate metadata feeds clap's `--version`/`--help`
/// via `#[command(author, version, about)]`. clap 4 does not render `author`
/// in help output, so fixing the manifest is expected to leave this output
/// unchanged - this pins that, and pins that neither surface ever names the
/// upstream project.
#[test]
fn version_and_help_do_not_mention_the_upstream_project() {
    for flag in ["--version", "--help"] {
        let output = Command::new(bin())
            .arg(flag)
            .output()
            .unwrap_or_else(|e| panic!("failed to run nix-options-doc {flag}: {e}"));
        assert!(output.status.success(), "{flag} should exit 0");

        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            !text.contains("Thunderbottom"),
            "{flag} output should not name the upstream repo, got: {text:?}"
        );
        assert!(
            !text.contains("Chinmay"),
            "{flag} output should not name the upstream author, got: {text:?}"
        );
    }
}
