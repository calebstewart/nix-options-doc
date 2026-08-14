//! Default log visibility (#9) and `RUST_LOG` override handling.

use crate::common::{bin, temp_dir_with_nix_file};
use std::process::Command;

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

/// Pins the behavior described in the logger comment in `src/main.rs` (#43):
/// `default_filter_or("warn")` applies only when `RUST_LOG` is *unset*.
/// `RUST_LOG=""` is set-but-empty, so `env_logger` uses it, parses zero
/// directives out of it, and falls back to `env_filter`'s own `error` default -
/// which silences the very warnings #9 made visible. This is stock `env_logger`
/// behavior that we deliberately do not special-case, so the test asserts the
/// warning is *absent*; if a future `env_logger` bump changes it, this test
/// fails and the comment in `src/main.rs` must be updated to match.
///
/// Unix-only: `Command::env("RUST_LOG", "")` writes a bare `RUST_LOG=` entry
/// into the Windows environment block, where empty-valued variables are not
/// reliably distinguishable from unset ones. The behavior under test is a
/// property of `env_logger`, not of the platform, so pinning it on Unix is
/// enough and keeps the Windows leg of the CI matrix deterministic.
#[cfg(unix)]
#[test]
fn empty_rust_log_falls_back_to_env_loggers_own_error_default() {
    let temp_dir =
        temp_dir_with_nix_file("{ config, ... }: {\n  config.services.foo.enable = true;\n}\n");
    let out_path = temp_dir.path().join("out.md");

    let output = Command::new(bin())
        .env("RUST_LOG", "")
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

    // The document is still written - only the *logging* is suppressed.
    let contents = std::fs::read_to_string(&out_path).expect("output file should exist");
    assert!(
        contents.contains("# NixOS Module Options"),
        "expected a well-formed empty Markdown document, got: {contents:?}"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("No NixOS options found"),
        "RUST_LOG=\"\" is set-but-empty, so env_logger's own `error` default \
         applies and the warn-level message should be suppressed; got: {stderr:?}"
    );
}
