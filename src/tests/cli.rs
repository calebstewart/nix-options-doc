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
