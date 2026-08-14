use super::*;

/// Tests the parsing of command-line arguments.
#[test]
fn test_cli_args() {
    use clap::Parser;

    let args = Cli::parse_from(["program", "--path", "/test/path"]);
    assert_eq!(args.io.path, "/test/path");
    assert_eq!(args.io.out, "stdout"); // default value
    assert!(!args.io.sort); // default false

    let args = Cli::parse_from(["program", "--out", "stdout", "--sort"]);
    assert_eq!(args.io.path, "."); // default value
    assert_eq!(args.io.out, "stdout");
    assert!(args.io.sort);
}

/// Tests the parsing of replacement arguments from the command line.
#[test]
fn test_cli_replace_argument() {
    use clap::Parser;

    // Test parsing replacement arguments
    let args = Cli::parse_from([
        "program",
        "--replace",
        "namespace=snowflake",
        "--replace",
        "system=x86_64-linux",
    ]);

    assert_eq!(args.filter.replace.len(), 2);
    assert!(args
        .filter
        .replace
        .contains(&("namespace".to_string(), "snowflake".to_string())));
    assert!(args
        .filter
        .replace
        .contains(&("system".to_string(), "x86_64-linux".to_string())));

    // Convert to HashMap and verify
    let replacements: HashMap<String, String> = args.filter.replace.into_iter().collect();
    assert_eq!(
        replacements.get("namespace"),
        Some(&"snowflake".to_string())
    );
    assert_eq!(
        replacements.get("system"),
        Some(&"x86_64-linux".to_string())
    );
}

/// Tests that an invalid `--branch` ref name produces a `NixDocError`
/// rather than panicking.
#[test]
fn test_prepare_path_invalid_branch_name() {
    use clap::Parser;

    // `with_ref_name` validates locally, before any network traffic, so
    // this needs no remote - the URL only has to be syntactically valid
    // and not exist on disk.
    for branch in [
        "my branch", // InvalidByte
        "foo..bar",  // RepeatedDot
        "",          // Empty
        "trailing/", // EndsWithSlash
        "has*star",  // Asterisk
        ".leading",  // StartsWithDot
        "ends.lock", // LockFileSuffix
        "re@{flog",  // ReflogPortion
    ] {
        let cli = Cli::parse_from([
            "program",
            "--path",
            "https://example.invalid/owner/repo.git",
            "--branch",
            branch,
        ]);

        let err = prepare_path(&cli)
            .err()
            .unwrap_or_else(|| panic!("expected an error for branch {branch:?}"));

        let msg = err.to_string();
        assert!(
            msg.contains("Invalid branch or tag name"),
            "branch {branch:?} produced the wrong error: {msg}"
        );
        assert!(
            msg.contains(branch),
            "branch {branch:?} is not named in the error: {msg}"
        );
    }
}

/// Tests that a local path short-circuits before any git handling.
#[test]
fn test_prepare_path_local_path_ignores_branch(
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use clap::Parser;

    let temp_dir = TempDir::new()?;
    let cli = Cli::parse_from([
        "program",
        "--path",
        &temp_dir.path().to_string_lossy(),
        "--branch",
        "not a valid ref",
    ]);

    let (path, tmp) = prepare_path(&cli)?;
    assert_eq!(path, temp_dir.path());
    assert!(tmp.is_none());

    Ok(())
}

/// Regression test for #52: an absolute `--path` that does not exist must be
/// reported as a missing local path, not handed to the clone branch. This is
/// also the Windows-relevant case - `gix::url::parse` treats a Windows
/// absolute path (`C:\...`) as an `ssh` URL with host `"c"`, so the
/// `is_absolute()` check in `prepare_path` must run before the scheme check.
#[test]
fn test_prepare_path_nonexistent_absolute_path_is_not_a_clone_error(
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use clap::Parser;

    let temp = TempDir::new()?;
    let missing = temp.path().join("definitely-not-here");
    let cli = Cli::parse_from(["program", "--path", &missing.to_string_lossy()]);

    let err = prepare_path(&cli).expect_err("expected an error for a nonexistent absolute path");
    let msg = err.to_string();

    assert!(
        msg.contains("does not exist"),
        "expected a 'does not exist' message, got: {msg}"
    );
    assert!(
        msg.contains(&missing.to_string_lossy().to_string()),
        "expected the path to be named in the error, got: {msg}"
    );
    assert!(
        !msg.contains("clone"),
        "a missing local path must not be reported as a clone failure, got: {msg}"
    );

    Ok(())
}

/// Regression test for #52, on the `Scheme::File` branch: a relative
/// `--path` that does not exist must also be reported as a missing local
/// path rather than a clone failure. Covers both the `./foo` and bare `foo`
/// spellings, since `gix::url::parse` reaches `Scheme::File` via slightly
/// different paths for each and a typo commonly looks like the bare form.
#[test]
fn test_prepare_path_nonexistent_relative_path_is_not_a_clone_error() {
    use clap::Parser;

    for missing in [
        "./nix-options-doc-issue-52-missing",
        "nix-options-doc-issue-52-missing",
    ] {
        let cli = Cli::parse_from(["program", "--path", missing]);

        let err = prepare_path(&cli)
            .err()
            .unwrap_or_else(|| panic!("expected an error for relative path {missing:?}"));
        let msg = err.to_string();

        assert!(
            msg.contains("does not exist"),
            "path {missing:?} produced the wrong error: {msg}"
        );
        assert!(
            msg.contains(missing),
            "path {missing:?} is not named in the error: {msg}"
        );
        assert!(
            !msg.contains("clone"),
            "path {missing:?} must not be reported as a clone failure: {msg}"
        );
    }
}

/// Regression test against a plausible wrong fix for #52: tightening the
/// local branch to require `path.is_dir()`, which would break the existing
/// `--path some/module.nix` feature (a single `.nix` file is a valid,
/// documented `--path` value).
#[test]
fn test_prepare_path_existing_file_is_still_accepted(
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use clap::Parser;

    let temp_dir = TempDir::new()?;
    let file = temp_dir.path().join("module.nix");
    std::fs::write(&file, "{ }")?;

    let cli = Cli::parse_from(["program", "--path", &file.to_string_lossy()]);
    let (path, tmp) = prepare_path(&cli)?;

    assert_eq!(path, file);
    assert!(tmp.is_none());

    Ok(())
}

/// Regression test against an over-broad local-path heuristic for #52: a
/// value that actually parses as a remote git URL must still take the clone
/// branch, not get reported as a missing local path. Reuses the no-network
/// trick from `test_prepare_path_invalid_branch_name` - an invalid
/// `--branch` is validated locally by `with_ref_name` before any network
/// round-trip, so the assertion that this reaches the clone branch requires
/// no network traffic.
#[test]
fn test_prepare_path_url_shaped_value_still_takes_the_clone_branch() {
    use clap::Parser;

    for url in [
        "https://example.invalid/owner/repo.git",
        "git@example.invalid:owner/repo.git",
        "ssh://git@example.invalid/owner/repo.git",
        "git://example.invalid/owner/repo.git",
    ] {
        let cli = Cli::parse_from(["program", "--path", url, "--branch", "my branch"]);

        let err = prepare_path(&cli)
            .err()
            .unwrap_or_else(|| panic!("expected an error for url {url:?}"));
        let msg = err.to_string();

        assert!(
            msg.contains("Invalid branch or tag name"),
            "url {url:?} did not take the clone branch: {msg}"
        );
        assert!(
            !msg.contains("does not exist"),
            "url {url:?} must not be reported as a missing local path: {msg}"
        );
    }
}
