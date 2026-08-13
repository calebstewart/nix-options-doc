use super::*;

/// Tests that hidden files are correctly excluded from processing.
#[test]
fn test_hidden_files_exclusion() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;
    let content = r#"
{
  options.test.hidden = {
    enable = lib.mkEnableOption "Hidden test option";
  };
}
"#;
    create_test_file(temp_dir.path(), ".hidden.nix", content)?;

    let options = collect_options(temp_dir.path(), &[], &HashMap::new(), false, false)?;

    assert_eq!(options.len(), 0);

    Ok(())
}

/// Tests that `.nix` files inside hidden directories are not documented.
///
/// Only the entry being tested is checked for a leading dot, so before
/// hidden directories were pruned during traversal a perfectly
/// ordinary-looking `secret.nix` inside `.direnv`/`.git` passed the
/// per-file filter and was documented (nix-options-doc#8).
#[test]
fn test_hidden_directories_are_pruned() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;

    fs::create_dir_all(temp_dir.path().join("modules"))?;
    fs::create_dir_all(temp_dir.path().join(".direnv"))?;
    fs::create_dir_all(temp_dir.path().join(".git").join("objects"))?;

    create_test_file(
        temp_dir.path().join("modules").as_path(),
        "real.nix",
        r#"{ options.real.thing.enable = lib.mkEnableOption "Real option"; }"#,
    )?;
    create_test_file(
        temp_dir.path().join(".direnv").as_path(),
        "secret.nix",
        r#"{ options.hidden.thing.enable = lib.mkEnableOption "Hidden option"; }"#,
    )?;
    // Nested two levels deep: pruning must stop the descent, not just skip
    // the hidden directory node itself.
    create_test_file(
        temp_dir.path().join(".git").join("objects").as_path(),
        "deep.nix",
        r#"{ options.git.thing.enable = lib.mkEnableOption "Git option"; }"#,
    )?;

    let options = collect_options(temp_dir.path(), &[], &HashMap::new(), false, false)?;

    assert_eq!(options.len(), 1);
    assert_eq!(options[0].name, "options.real.thing.enable");
    assert!(!options
        .iter()
        .any(|o| o.name == "options.hidden.thing.enable"));
    assert!(!options.iter().any(|o| o.name == "options.git.thing.enable"));
    assert!(!options.iter().any(|o| o
        .declarations
        .iter()
        .any(|d| d.file_path.contains(".direnv") || d.file_path.contains(".git"))));

    Ok(())
}

/// Tests that a hidden directory passed as the root is still processed.
///
/// Pruning hidden entries must exempt depth 0: `--path` defaults to `"."`,
/// and `walkdir` reports the root of `WalkDir::new(".")` with the file
/// name `"."`, so a predicate without a depth check silently produces zero
/// options for the default invocation and for an explicit hidden root such
/// as `--path ./.config/nixos`.
#[test]
fn test_hidden_root_directory_is_still_processed(
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;
    let hidden_root = temp_dir.path().join(".config");
    fs::create_dir_all(hidden_root.join("modules"))?;

    create_test_file(
        hidden_root.as_path(),
        "top.nix",
        r#"{ options.rooted.top.enable = lib.mkEnableOption "Top option"; }"#,
    )?;
    create_test_file(
        hidden_root.join("modules").as_path(),
        "nested.nix",
        r#"{ options.rooted.nested.enable = lib.mkEnableOption "Nested option"; }"#,
    )?;

    let options = collect_options(hidden_root.as_path(), &[], &HashMap::new(), false, false)?;

    assert_eq!(options.len(), 2);
    assert!(options
        .iter()
        .any(|o| o.name == "options.rooted.top.enable"));
    assert!(options
        .iter()
        .any(|o| o.name == "options.rooted.nested.enable"));

    Ok(())
}

/// Tests that duplicate option definitions are handled correctly.
#[test]
fn test_duplicate_prevention() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;
    let content = r#"
{
  options.test = {
    enable = lib.mkEnableOption "Test option";
    enable = lib.mkEnableOption "Duplicate test option";
  };
}
"#;
    create_test_file(temp_dir.path(), "test.nix", content)?;

    let options = collect_options(temp_dir.path(), &[], &HashMap::new(), false, false)?;

    let enable_options: Vec<_> = options
        .iter()
        .filter(|o| o.name == "options.test.enable")
        .collect();

    assert_eq!(
        enable_options.len(),
        1,
        "Should only have one enable option"
    );

    Ok(())
}

/// Tests that options in excluded directories are not included in the results.
#[test]
fn test_exclude_dir() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;

    // Create a structure with files in subdirectories
    let main_content = r#"
{
  options.main = {
    enable = lib.mkEnableOption "Main option";
  };
}
"#;

    let excluded_content = r#"
{
  options.excluded = {
    enable = lib.mkEnableOption "Excluded option";
  };
}
"#;

    // Create directories and files
    fs::create_dir_all(temp_dir.path().join("modules"))?;
    fs::create_dir_all(temp_dir.path().join("excluded"))?;

    create_test_file(temp_dir.path(), "main.nix", main_content)?;
    create_test_file(
        temp_dir.path().join("excluded").as_path(),
        "excluded.nix",
        excluded_content,
    )?;

    // Test without exclusion
    let all_options = collect_options(temp_dir.path(), &[], &HashMap::new(), false, false)?;

    assert!(!all_options.is_empty()); // At least the main option
    assert!(all_options.iter().any(|o| o.name == "options.main.enable"));

    // Test with exclusion
    let exclude_dirs = vec![temp_dir
        .path()
        .join("excluded")
        .to_string_lossy()
        .to_string()];

    let filtered_options = collect_options(
        temp_dir.path(),
        &exclude_dirs,
        &HashMap::new(),
        false,
        false,
    )?;

    assert!(filtered_options
        .iter()
        .any(|o| o.name == "options.main.enable"));
    assert!(!filtered_options
        .iter()
        .any(|o| o.name == "options.excluded.enable"));

    Ok(())
}

/// Tests error handling for invalid paths and malformed files.
#[test]
fn test_error_handling() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;

    // Test non-existent path
    let non_existent = temp_dir.path().join("non-existent");
    let result = collect_options(&non_existent, &[], &HashMap::new(), false, false);
    assert!(result.is_err(), "Non-existent paths should return an error");

    // Create a file with invalid Nix syntax
    let invalid_content = r#"
{
  options.test = {
    # Missing closing brace
    invalid = lib.mkEnableOption "Invalid option"
  ;
}
"#;
    create_test_file(temp_dir.path(), "invalid.nix", invalid_content)?;

    // File processing should continue even with parse errors
    let result = collect_options(temp_dir.path(), &[], &HashMap::new(), false, false);
    assert!(
        result.is_ok(),
        "Processing should continue even with parse errors"
    );

    // Create a file with valid Nix syntax alongside the invalid one
    let valid_content = r#"
{
  options.test.valid = {
    enable = lib.mkEnableOption "Valid option";
  };
}
"#;
    create_test_file(temp_dir.path(), "valid.nix", valid_content)?;

    // We should still find the valid option
    // even when there's an invalid file in the same directory
    let options_with_valid = collect_options(temp_dir.path(), &[], &HashMap::new(), false, false)?;
    assert!(
        !options_with_valid.is_empty(),
        "Valid options should be found even when some files have errors"
    );

    // Test a directory with .nix extension
    let dir_with_nix_ext = temp_dir.path().join("not-readable.nix");
    std::fs::create_dir(&dir_with_nix_ext)?;

    // Should not error out even with the unreadable "file"
    let result = collect_options(temp_dir.path(), &[], &HashMap::new(), false, false);
    assert!(
        result.is_ok(),
        "Should handle directories with .nix extensions"
    );

    Ok(())
}

/// Tests that an option declared more than once (e.g. across separate
/// module fragments) keeps all of its declarations instead of silently
/// dropping every one after the first.
#[test]
fn test_multiple_declarations_are_merged() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;
    create_test_file(
        temp_dir.path(),
        "a.nix",
        r#"
{
  options.test.shared = lib.mkOption {
    type = lib.types.bool;
    default = false;
  };
}
"#,
    )?;
    create_test_file(
        temp_dir.path(),
        "b.nix",
        r#"
{
  options.test.shared = lib.mkOption {
    type = lib.types.bool;
    default = false;
  };
}
"#,
    )?;

    let options = collect_options(temp_dir.path(), &[], &HashMap::new(), false, false)?;
    let shared: Vec<_> = options
        .iter()
        .filter(|o| o.name == "options.test.shared")
        .collect();

    assert_eq!(shared.len(), 1, "should merge into a single entry");
    assert_eq!(
        shared[0].declarations.len(),
        2,
        "should keep both declarations"
    );

    let files: std::collections::HashSet<_> = shared[0]
        .declarations
        .iter()
        .map(|d| d.file_path.as_str())
        .collect();
    assert!(files.contains("a.nix"));
    assert!(files.contains("b.nix"));

    Ok(())
}

/// Guards the `match` → `if let`/`else` rewrite of the option-merge loop in
/// `collect_options` (`src/lib.rs`, `clippy::single_match_else` fix). That
/// rewrite must preserve first-wins semantics exactly: when the same option
/// name is declared in more than one file, the primary `OptionDoc` (name,
/// description, etc.) comes from whichever file `nix_files.sort()` placed
/// first, and a per-declaration `description` is only populated on
/// `Declaration`s whose description differs from that primary one.
#[test]
fn test_duplicate_option_merge_keeps_first_declaration_primary(
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;
    create_test_file(
        temp_dir.path(),
        "a.nix",
        r#"
{
  options.test.shared = lib.mkOption {
    type = lib.types.bool;
    default = false;
    description = "Description from a.nix.";
  };
}
"#,
    )?;
    create_test_file(
        temp_dir.path(),
        "b.nix",
        r#"
{
  options.test.shared = lib.mkOption {
    type = lib.types.bool;
    default = false;
    description = "Description from b.nix.";
  };
}
"#,
    )?;

    let options = collect_options(temp_dir.path(), &[], &HashMap::new(), false, false)?;
    let shared: Vec<_> = options
        .iter()
        .filter(|o| o.name == "options.test.shared")
        .collect();

    assert_eq!(shared.len(), 1, "should merge into a single entry");
    assert_eq!(
        shared[0].description,
        Some("Description from a.nix.".to_string()),
        "first-found (a.nix, per nix_files.sort()) should win as primary"
    );

    let declarations = &shared[0].declarations;
    assert_eq!(declarations.len(), 2);
    assert!(
        declarations[0].file_path.ends_with("a.nix"),
        "declarations[0] should be a.nix: {:?}",
        declarations[0].file_path
    );
    assert!(
        declarations[1].file_path.ends_with("b.nix"),
        "declarations[1] should be b.nix: {:?}",
        declarations[1].file_path
    );
    assert_eq!(
        declarations[0].description, None,
        "primary declaration's description matches the OptionDoc's own, so \
         it should not be repeated per-declaration"
    );
    assert_eq!(
        declarations[1].description,
        Some("Description from b.nix.".to_string()),
        "the differing declaration should carry its own description"
    );

    Ok(())
}
