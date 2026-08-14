//! `--help` / `--version` output.

use crate::common::bin;
use std::process::Command;

/// nix-options-doc#42: `--follow-symlinks` can reach into hidden directories
/// through a non-hidden link, which is specified behavior rather than a leak.
/// The only mitigation shipped is that the flag says so, so the caveat living
/// in `--help` is the fix itself and is pinned here. Long help is only
/// reachable from the process (`--help`, not `-h`), which is why this is an
/// integration test rather than a unit test.
#[test]
fn follow_symlinks_help_documents_hidden_directory_reach() {
    let output = Command::new(bin())
        .arg("--help")
        .output()
        .expect("failed to run nix-options-doc --help");
    assert!(output.status.success(), "--help should exit 0");

    let text = String::from_utf8_lossy(&output.stdout);
    let flattened = text.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        flattened.contains("Hidden-directory pruning applies to the tree being walked"),
        "--help should explain that pruning does not follow link targets, got: {text}"
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
