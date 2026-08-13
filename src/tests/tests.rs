use super::*;
use crate::generate::generate_html;
use crate::generate::generate_markdown;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

/// Creates a test file with the specified filename and content in the given directory.
///
/// # Arguments
/// - `dir`: The directory in which to create the file.
/// - `filename`: The name of the file to create.
/// - `content`: The content to write into the file.
///
/// # Returns
/// A Result indicating success or an I/O error.
fn create_test_file(dir: &Path, filename: &str, content: &str) -> Result<(), std::io::Error> {
    fs::write(dir.join(filename), content)
}

/// Tests that a simple option is parsed correctly from a Nix file.
#[test]
fn test_basic_option_parsing() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;
    let content = r#"
{
  options.test.simple = {
    enable = lib.mkEnableOption "Simple test option";
  };
}
"#;
    create_test_file(temp_dir.path(), "flake.nix", content)?;

    let options = collect_options(temp_dir.path(), &[], &HashMap::new(), false, false)?;

    assert_eq!(options.len(), 1);
    assert_eq!(options[0].name, "options.test.simple.enable");
    assert_eq!(options[0].nix_type.to_string(), "boolean");
    assert_eq!(
        options[0].description,
        Some("Whether to enable Simple test option.".to_string())
    );
    assert_eq!(options[0].default_value, Some("false".to_string()));

    Ok(())
}

/// Tests parsing of complex options including nested attributes.
#[test]
fn test_complex_option_parsing() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;
    let content = r#"
{
  options.test.complex = {
    stringOpt = lib.mkOption {
      type = lib.types.str;
      default = "test";
      description = "A string option";
    };

    nested.value = lib.mkOption {
      type = lib.types.int;
      description = "A nested number option";
    };
  };
}
"#;
    create_test_file(temp_dir.path(), "test.nix", content)?;

    let options = collect_options(temp_dir.path(), &[], &HashMap::new(), false, false)?;

    assert_eq!(options.len(), 2);

    let string_opt = options
        .iter()
        .find(|o| o.name == "options.test.complex.stringOpt")
        .unwrap();
    assert_eq!(string_opt.nix_type.to_string(), "string");
    assert_eq!(string_opt.description, Some("A string option".to_string()));
    assert_eq!(string_opt.default_value, Some("\"test\"".to_string()));

    let nested_opt = options
        .iter()
        .find(|o| o.name == "options.test.complex.nested.value")
        .unwrap();
    assert_eq!(nested_opt.nix_type.to_string(), "signed integer");
    assert_eq!(
        nested_opt.description,
        Some("A nested number option".to_string())
    );
    assert_eq!(nested_opt.default_value, None);

    Ok(())
}

/// Tests the generation of Markdown documentation from a set of option definitions.
#[test]
fn test_markdown_generation() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let options = vec![
        OptionDoc {
            name: "options.test.opt1".to_string(),
            description: Some("Test option 1".to_string()),
            nix_type: "boolean".to_string(),
            default_value: Some("false".to_string()),
            example: None,
            renamed_to: None,
            declarations: vec![Declaration {
                file_path: "test.nix".to_string(),
                line_number: 1,
                description: None,
                condition: None,
            }],
        },
        OptionDoc {
            name: "options.test.opt2".to_string(),
            description: Some("Test option 2".to_string()),
            nix_type: "lib.types.str".to_string(),
            default_value: None,
            example: None,
            renamed_to: None,
            declarations: vec![Declaration {
                file_path: "test.nix".to_string(),
                line_number: 2,
                description: None,
                condition: None,
            }],
        },
    ];

    // Generate markdown
    let markdown = generate_markdown(&options)?;

    // Validate markdown content
    assert!(markdown.contains("# NixOS Module Options"));
    assert!(markdown.contains("## [`options.test.opt1`](test.nix#L1)"));
    assert!(markdown.contains("## [`options.test.opt2`](test.nix#L2)"));
    assert!(markdown.contains("Test option 1"));
    assert!(markdown.contains("Test option 2"));
    assert!(markdown.contains("**Type:** `boolean`"));
    assert!(markdown.contains("**Type:** `lib.types.str`"));
    assert!(markdown.contains("**Default:** `false`"));

    // Test sorted output
    let mut sorted_options = options.clone();
    sorted_options.sort_by(|a, b| a.name.cmp(&b.name));
    let markdown_sorted = generate_markdown(&sorted_options)?;
    let opt1_pos = markdown_sorted.find("options.test.opt1").unwrap();
    let opt2_pos = markdown_sorted.find("options.test.opt2").unwrap();
    assert!(opt1_pos < opt2_pos);

    Ok(())
}

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
    assert!(options.iter().any(|o| o.name == "options.rooted.top.enable"));
    assert!(options
        .iter()
        .any(|o| o.name == "options.rooted.nested.enable"));

    Ok(())
}

/// Tests that the traversal predicate accepts a root whose walkdir file
/// name is the literal `"."`.
///
/// `DirEntry::file_name` falls back to the whole path when the path has no
/// final component, so the root of `WalkDir::new(".")` - the default
/// `--path` - reports the file name `"."` and looks hidden. Only the
/// depth-0 exemption saves it, and no `collect_options` test can cover
/// this without mutating the process working directory.
#[test]
fn test_dot_root_entry_is_traversable() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Take only the first yielded entry: that is the root itself, so this
    // does not actually walk the crate directory.
    let root = walkdir::WalkDir::new(".")
        .into_iter()
        .next()
        .expect("walkdir always yields the root entry")?;

    assert_eq!(root.depth(), 0);
    assert_eq!(root.file_name().to_string_lossy(), ".");
    assert!(utils::should_traverse_entry(&root));

    Ok(())
}

/// Tests the parsing of multi-line description in option definition.
#[test]
fn test_multiline_description_parsing() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;
    let content = r#"
{
  options.test.complex = {
    packages = lib.mkOption {
      type = with lib.types; listOf str;
      description = ''
        A multi-line description
        with multiple lines
        and some indentation.
      '';
      default = [];
    };
    values = lib.mkOption {
      type = with lib.types; listOf int;
      description = ''
        A multi-line description
        with multiple lines.

        And some more text across
        another paragraph.
      '';
      default = [1, 2];
    };
  };
}
"#;
    create_test_file(temp_dir.path(), "flake.nix", content)?;

    let options = collect_options(temp_dir.path(), &[], &HashMap::new(), false, false)?;

    assert_eq!(options.len(), 2);

    // Sort the options by name to ensure consistent order
    let mut sorted_options = options.clone();
    sorted_options.sort_by(|a, b| a.name.cmp(&b.name));

    assert_eq!(sorted_options[0].name, "options.test.complex.packages");
    assert_eq!(sorted_options[1].name, "options.test.complex.values");

    assert_eq!(sorted_options[0].nix_type.to_string(), "list of string");
    assert_eq!(
        sorted_options[1].nix_type.to_string(),
        "list of signed integer"
    );

    // Check multi-line description - trim any extra whitespace at beginning/end
    let desc0 = sorted_options[0]
        .description
        .as_ref()
        .map(|s| s.trim().to_string());
    let desc1 = sorted_options[1]
        .description
        .as_ref()
        .map(|s| s.trim().to_string());

    assert_eq!(
        desc0,
        Some("A multi-line description\nwith multiple lines\nand some indentation.".to_string())
    );
    assert_eq!(
        desc1,
        Some("A multi-line description\nwith multiple lines.\n\nAnd some more text across\nanother paragraph.".to_string())
    );
    assert_eq!(sorted_options[0].default_value, Some("[]".to_string()));
    assert_eq!(sorted_options[1].default_value, Some("[1, 2]".to_string()));

    Ok(())
}

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

/// Tests variable replacement functionality in option names and descriptions.
#[test]
fn test_variable_replacements() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;

    // Create a file with variable interpolation
    let content = r#"
{
  options.${namespace}.hardware.bluetooth = {
    enable = lib.mkEnableOption "Enable ${namespace} bluetooth";
  };
  
  options.${system}.networking = {
    enable = lib.mkEnableOption "Enable networking for ${system}";
  };
}
"#;
    create_test_file(temp_dir.path(), "config.nix", content)?;

    // Set up replacements
    let mut replacements = HashMap::new();
    replacements.insert("namespace".to_string(), "snowflake".to_string());
    replacements.insert("system".to_string(), "x86_64-linux".to_string());

    let options = collect_options(temp_dir.path(), &[], &replacements, false, false)?;

    // Check if options contain the replaced values
    let bluetooth_options: Vec<_> = options
        .iter()
        .filter(|o| o.name.contains("bluetooth"))
        .collect();

    let networking_options: Vec<_> = options
        .iter()
        .filter(|o| o.name.contains("networking"))
        .collect();

    if !bluetooth_options.is_empty() {
        let bluetooth_opt = &bluetooth_options[0];
        assert!(bluetooth_opt.name.contains("snowflake"));
        assert!(!bluetooth_opt.name.contains("${namespace}"));

        // Check if description also had replacements
        if let Some(desc) = &bluetooth_opt.description {
            assert!(desc.contains("snowflake"));
            assert!(!desc.contains("${namespace}"));
        }
    }

    if !networking_options.is_empty() {
        let networking_opt = &networking_options[0];
        assert!(networking_opt.name.contains("x86_64-linux"));
        assert!(!networking_opt.name.contains("${system}"));

        // Check if description also had replacements
        if let Some(desc) = &networking_opt.description {
            assert!(desc.contains("x86_64-linux"));
            assert!(!desc.contains("${system}"));
        }
    }

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

#[test]
fn test_admonition_conversion() {
    let input = r#"
Here is some text.

::: {.important}
This is an important notice.
With multiple lines.
:::

More text here.

::: {.warning}
This is a warning.
:::

::: {.note}
This is a note with code:
```rust
fn main() {
    println!("Hello");
}
```
:::
"#;

    let expected = r#"
Here is some text.

> [!IMPORTANT]  
> This is an important notice.
> With multiple lines.

More text here.

> [!WARNING]  
> This is a warning.

> [!NOTE]  
> This is a note with code:
> ```rust
> fn main() {
>     println!("Hello");
> }
> ```
"#;

    assert_eq!(utils::convert_admonitions(input), expected);
}

#[test]
fn test_clean_description_with_admonitions() {
    let input = r#"
This is a description with {code}`example` and an admonition:

::: {.important}
Critical security information.
:::
"#;

    let expected = r#"
This is a description with `example` and an admonition:

> [!IMPORTANT]  
> Critical security information.
"#;

    assert_eq!(utils::clean_description(input), expected);
}

/// Tests that common type combinators are formatted into human-readable
/// descriptions instead of being dumped as raw source text.
#[test]
fn test_type_formatter_combinators() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;
    let content = r#"
{
  options.test = {
    a = lib.mkOption { type = lib.types.nullOr lib.types.bool; };
    b = lib.mkOption { type = lib.types.listOf lib.types.str; };
    c = lib.mkOption { type = lib.types.attrsOf lib.types.int; };
    d = lib.mkOption { type = lib.types.either lib.types.str lib.types.int; };
    e = lib.mkOption { type = lib.types.enum [ "a" "b" "c" ]; };
    f = lib.mkOption { type = lib.types.functionTo lib.types.str; };
  };
}
"#;
    create_test_file(temp_dir.path(), "types.nix", content)?;

    let options = collect_options(temp_dir.path(), &[], &HashMap::new(), false, false)?;
    let find = |name: &str| {
        options
            .iter()
            .find(|o| o.name == format!("options.test.{name}"))
            .unwrap()
            .nix_type
            .clone()
    };

    assert_eq!(find("a"), "null or boolean");
    assert_eq!(find("b"), "list of string");
    assert_eq!(find("c"), "attribute set of signed integer");
    assert_eq!(find("d"), "string or signed integer");
    assert_eq!(find("e"), "one of \"a\", \"b\", \"c\"");
    assert_eq!(find("f"), "function that evaluates to string");

    Ok(())
}

/// Tests that inline submodules (including behind `attrsOf`/`listOf`) are
/// recursed into so their nested options show up in the output, using a
/// `<name>` placeholder segment for container types.
#[test]
fn test_inline_submodule_recursion() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;
    let content = r#"
{
  options.test.settings = lib.mkOption {
    type = lib.types.attrsOf (lib.types.submodule {
      options = {
        weight = lib.mkOption {
          type = lib.types.int;
          default = 1;
          description = "The weight.";
        };
      };
    });
    default = { };
    description = "Per-entry settings.";
  };

  options.test.server = lib.mkOption {
    type = lib.types.submodule {
      options = {
        host = lib.mkOption {
          type = lib.types.str;
          description = "The host.";
        };
      };
    };
    description = "Server config.";
  };
}
"#;
    create_test_file(temp_dir.path(), "submodule.nix", content)?;

    let options = collect_options(temp_dir.path(), &[], &HashMap::new(), false, false)?;

    assert!(options
        .iter()
        .any(|o| o.name == "options.test.settings" && o.nix_type == "attribute set of submodule"));
    let weight = options
        .iter()
        .find(|o| o.name == "options.test.settings.<name>.weight")
        .expect("nested attrsOf-submodule option should be present");
    assert_eq!(weight.nix_type, "signed integer");
    assert_eq!(weight.default_value, Some("1".to_string()));

    // A non-container submodule's nested options are not placed behind
    // a `<name>` placeholder.
    let host = options
        .iter()
        .find(|o| o.name == "options.test.server.host")
        .expect("nested plain-submodule option should be present");
    assert_eq!(host.nix_type, "string");

    Ok(())
}

/// Tests that `mkDefault`/`mkForce`/`mkIf` wrappers around a default value
/// are unwrapped to the guarded value rather than shown as raw source text.
#[test]
fn test_default_wrapper_unwrapping() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;
    let content = r#"
{
  options.test = {
    a = lib.mkOption {
      type = lib.types.str;
      default = lib.mkDefault "hello";
    };
    b = lib.mkOption {
      type = lib.types.int;
      default = lib.mkIf true 8080;
    };
    c = lib.mkOption {
      type = lib.types.int;
      default = lib.mkForce 42;
    };
  };
}
"#;
    create_test_file(temp_dir.path(), "wrappers.nix", content)?;

    let options = collect_options(temp_dir.path(), &[], &HashMap::new(), false, false)?;
    let find = |name: &str| {
        options
            .iter()
            .find(|o| o.name == format!("options.test.{name}"))
            .unwrap()
            .default_value
            .clone()
    };

    assert_eq!(find("a"), Some("\"hello\"".to_string()));
    assert_eq!(find("b"), Some("8080".to_string()));
    assert_eq!(find("c"), Some("42".to_string()));

    Ok(())
}

/// Tests that calls through a locally-aliased binding (`let mkOpt =
/// lib.mkOption; in ...`) are still recognized as `mkOption`.
#[test]
fn test_alias_resolution() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;
    let content = r#"
{ lib, ... }:
let
  mkOpt = lib.mkOption;
in
{
  options.test.aliased = mkOpt {
    type = lib.types.bool;
    default = false;
    description = "An option declared through an alias.";
  };
}
"#;
    create_test_file(temp_dir.path(), "alias.nix", content)?;

    let options = collect_options(temp_dir.path(), &[], &HashMap::new(), false, false)?;
    let aliased = options
        .iter()
        .find(|o| o.name == "options.test.aliased")
        .expect("option declared via aliased mkOption should be recognized");

    assert_eq!(aliased.nix_type, "boolean");
    assert_eq!(aliased.default_value, Some("false".to_string()));

    Ok(())
}

/// Tests `mkPackageOption`, both with and without overrides.
#[test]
fn test_mk_package_option() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;
    let content = r#"
{
  options.test = {
    plain = lib.mkPackageOption pkgs "hello" { };
    withOverrides = lib.mkPackageOption pkgs "flask" {
      default = [ "python3Packages" "flask" ];
      example = "pkgs.python3Packages.flask";
    };
  };
}
"#;
    create_test_file(temp_dir.path(), "package.nix", content)?;

    let options = collect_options(temp_dir.path(), &[], &HashMap::new(), false, false)?;

    let plain = options
        .iter()
        .find(|o| o.name == "options.test.plain")
        .unwrap();
    assert_eq!(plain.nix_type, "package");
    assert_eq!(plain.default_value, Some("pkgs.hello".to_string()));
    assert_eq!(plain.description, Some("The hello package to use.".to_string()));

    let overridden = options
        .iter()
        .find(|o| o.name == "options.test.withOverrides")
        .unwrap();
    assert_eq!(
        overridden.default_value,
        Some("pkgs.python3Packages.flask".to_string())
    );

    Ok(())
}

/// Tests that an option declared more than once (e.g. across separate
/// module fragments) keeps all of its declarations instead of silently
/// dropping every one after the first.
#[test]
fn test_multiple_declarations_are_merged() -> Result<(), Box<dyn std::error::Error + Send + Sync>>
{
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
    assert_eq!(shared[0].declarations.len(), 2, "should keep both declarations");

    let files: std::collections::HashSet<_> = shared[0]
        .declarations
        .iter()
        .map(|d| d.file_path.as_str())
        .collect();
    assert!(files.contains("a.nix"));
    assert!(files.contains("b.nix"));

    Ok(())
}

/// Tests that Markdown rendering links the heading to the first
/// declaration and lists any others under "Also declared in:", showing
/// a declaration's own description only when it differs from the
/// primary one.
#[test]
fn test_markdown_also_declared_in() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let options = vec![OptionDoc {
        name: "options.test.shared".to_string(),
        description: Some("Primary description.".to_string()),
        nix_type: "boolean".to_string(),
        default_value: Some("false".to_string()),
        example: None,
        renamed_to: None,
        declarations: vec![
            Declaration {
                file_path: "a.nix".to_string(),
                line_number: 1,
                description: None,
                condition: None,
            },
            Declaration {
                file_path: "b.nix".to_string(),
                line_number: 5,
                description: Some("Different description in b.nix.".to_string()),
                condition: None,
            },
        ],
    }];

    let markdown = generate_markdown(&options)?;

    // Heading links to the first (primary) declaration.
    assert!(markdown.contains("## [`options.test.shared`](a.nix#L1)"));
    // The second declaration is listed separately, with its own
    // (differing) description shown alongside it.
    assert!(markdown.contains("**Also declared in:**"));
    assert!(markdown.contains("[`b.nix`](b.nix#L5)"));
    assert!(markdown.contains("Different description in b.nix."));

    Ok(())
}

/// Tests that options declared entirely through a top-level `with lib;`
/// (the common home-manager style, wrapping the whole module body rather
/// than just the `options` attrset) are still found, including nested
/// `with types;` inside a type expression and `mkPackageOption` used
/// without a `lib.`/`pkgs.` prefix.
#[test]
fn test_top_level_with_lib() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;
    let content = r#"
{ config, lib, pkgs, ... }:
with lib;
{
  options.foo = {
    enable = mkEnableOption "foo";
    bar = mkOption {
      type = with types; nullOr (listOf str);
      default = null;
      description = "A list of strings, or null.";
    };
    pkg = mkPackageOption pkgs "foo" { };
  };
}
"#;
    create_test_file(temp_dir.path(), "module.nix", content)?;

    let options = collect_options(temp_dir.path(), &[], &HashMap::new(), false, false)?;

    let enable = options
        .iter()
        .find(|o| o.name == "options.foo.enable")
        .expect("mkEnableOption under top-level `with lib;` should be found");
    assert_eq!(enable.nix_type, "boolean");

    let bar = options
        .iter()
        .find(|o| o.name == "options.foo.bar")
        .expect("mkOption with a nested `with types;` type should be found");
    assert_eq!(bar.nix_type, "null or list of string");

    let pkg = options
        .iter()
        .find(|o| o.name == "options.foo.pkg")
        .expect("mkPackageOption under top-level `with lib;` should be found");
    assert_eq!(pkg.nix_type, "package");

    Ok(())
}

/// Tests that options declared underneath `mkIf`/`mkMerge` record the
/// guarding condition(s) on their declaration, including nested `mkIf`s
/// combining with `&&`, while options declared outside any `mkIf` (even
/// alongside one inside the same `mkMerge` list) get no condition at all.
#[test]
fn test_mkif_condition_tracking() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;
    let content = r#"
{
  options.test = lib.mkMerge [
    (lib.mkIf cfg.enable {
      guarded = lib.mkOption {
        type = lib.types.bool;
        default = false;
      };
    })
    (lib.mkIf cfg.enable (lib.mkIf cfg.extraFeature {
      doublyGuarded = lib.mkOption {
        type = lib.types.bool;
        default = false;
      };
    }))
    {
      unguarded = lib.mkOption {
        type = lib.types.bool;
        default = false;
      };
    }
  ];
}
"#;
    create_test_file(temp_dir.path(), "conditional.nix", content)?;

    let options = collect_options(temp_dir.path(), &[], &HashMap::new(), false, false)?;
    let find = |name: &str| {
        options
            .iter()
            .find(|o| o.name == format!("options.test.{name}"))
            .unwrap_or_else(|| panic!("option {name} should be found"))
            .declarations[0]
            .condition
            .clone()
    };

    assert_eq!(find("guarded"), Some("cfg.enable".to_string()));
    assert_eq!(
        find("doublyGuarded"),
        Some("cfg.enable && cfg.extraFeature".to_string())
    );
    assert_eq!(find("unguarded"), None);

    Ok(())
}

/// Tests that `mkRenamedOptionModule`/`mkRemovedOptionModule` shims
/// (typically found in `imports`, not `options`) surface as synthetic
/// entries at the old option's name, pointing at the replacement or
/// explaining the removal.
#[test]
fn test_deprecated_options() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;
    let content = r#"
{
  imports = [
    (lib.mkRenamedOptionModule [ "services" "oldName" ] [ "services" "newName" ])
    (lib.mkRemovedOptionModule [ "services" "goneName" ] "Use services.newName instead.")
    (lib.mkRemovedOptionModule [ "services" "silentlyGone" ] "")
  ];
}
"#;
    create_test_file(temp_dir.path(), "deprecations.nix", content)?;

    let options = collect_options(temp_dir.path(), &[], &HashMap::new(), false, false)?;

    // The shim's own name is prefixed with "options." to match how real
    // options in the rest of the document are named (from the literal
    // attrset structure, which by convention starts with "options"),
    // even though mkRenamedOptionModule/mkRemovedOptionModule's own
    // arguments are bare config paths.
    let renamed = options
        .iter()
        .find(|o| o.name == "options.services.oldName")
        .expect("renamed option shim should be found");
    assert_eq!(renamed.nix_type, "renamed option");
    assert_eq!(renamed.renamed_to.as_deref(), Some("services.newName"));
    // collect_options alone leaves the mention as plain text - turning
    // it into a link happens in filter_options (see
    // test_renamed_option_link_resolution), since the target's actual
    // anchor depends on whether --strip-prefix is used, which isn't
    // known yet at this point.
    assert!(renamed
        .description
        .as_deref()
        .unwrap()
        .contains("Use `services.newName` instead."));

    let removed = options
        .iter()
        .find(|o| o.name == "options.services.goneName")
        .expect("removed option shim should be found");
    assert_eq!(removed.nix_type, "removed option");
    assert!(removed
        .description
        .as_deref()
        .unwrap()
        .contains("Use services.newName instead."));

    let silently_removed = options
        .iter()
        .find(|o| o.name == "options.services.silentlyGone")
        .expect("removed option shim with an empty message should still be found");
    assert!(silently_removed
        .description
        .as_deref()
        .unwrap()
        .contains("This option has been removed."));

    Ok(())
}

/// Tests that a blockquote written as `> **Warning:** ...` (a common
/// informal convention outside nixpkgs' own Pandoc-based docs tooling)
/// is recognized as an admonition, not left as a plain blockquote.
#[test]
fn test_blockquote_admonition_conversion() {
    let input = "Expose the service to the internet.\n\n> **Warning:** Do *not* enable this without setting up authentication first!\n";

    let expected = "Expose the service to the internet.\n\n> [!WARNING]\n> Do *not* enable this without setting up authentication first!\n";

    assert_eq!(utils::convert_blockquote_admonitions(input), expected);

    // Descriptions with no such blockquote are returned byte-for-byte
    // unchanged (not just semantically equivalent).
    let plain = "Just a normal description.\n";
    assert_eq!(utils::convert_blockquote_admonitions(plain), plain);
}

/// Tests that `freeformType` on a submodule (with or without an explicit
/// `options` attrset alongside it) surfaces as a `<freeform>` placeholder
/// entry, rather than silently dropping the fact that undeclared options
/// are also accepted there.
#[test]
fn test_freeform_type() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;
    let content = r#"
{
  options.test.settings = lib.mkOption {
    type = lib.types.submodule {
      freeformType = lib.types.attrsOf lib.types.str;
      options = {
        enable = lib.mkEnableOption "the setting";
      };
    };
    default = { };
  };

  options.test.freeformOnly = lib.mkOption {
    type = lib.types.submodule {
      freeformType = lib.types.attrsOf lib.types.int;
    };
    default = { };
  };
}
"#;
    create_test_file(temp_dir.path(), "freeform.nix", content)?;

    let options = collect_options(temp_dir.path(), &[], &HashMap::new(), false, false)?;

    // The explicitly declared option is still found alongside freeformType.
    assert!(options
        .iter()
        .any(|o| o.name == "options.test.settings.enable"));

    let freeform = options
        .iter()
        .find(|o| o.name == "options.test.settings.<freeform>")
        .expect("freeformType alongside explicit options should be surfaced");
    assert_eq!(freeform.nix_type, "attribute set of string");

    let freeform_only = options
        .iter()
        .find(|o| o.name == "options.test.freeformOnly.<freeform>")
        .expect("freeformType with no explicit options attrset should still be surfaced");
    assert_eq!(freeform_only.nix_type, "attribute set of signed integer");

    Ok(())
}

/// Tests that `options.foo = let x = ...; in { ... };` (a local helper
/// binding before the options attrset) is still walked into, instead of
/// silently dropping every option inside it.
#[test]
fn test_let_in_attrset_value() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;
    let content = r#"
{
  options.test =
    let
      settingsFormat = { };
    in
    {
      enable = lib.mkEnableOption "the test module";
      port = lib.mkOption {
        type = lib.types.port;
        default = 8080;
      };
    };
}
"#;
    create_test_file(temp_dir.path(), "letin.nix", content)?;

    let options = collect_options(temp_dir.path(), &[], &HashMap::new(), false, false)?;

    assert!(options.iter().any(|o| o.name == "options.test.enable"));
    assert!(options.iter().any(|o| o.name == "options.test.port"));

    Ok(())
}

/// Tests that `<option-constructor> // { field = ...; }` (the standard
/// nixpkgs idiom for e.g. an enable option that defaults to true) is
/// parsed as the base option with the override attrset's fields applied
/// on top, rather than being dropped entirely.
#[test]
fn test_attrset_update_override() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;
    let content = r#"
{
  options.test = {
    assertUnique = lib.mkEnableOption "" // {
      default = true;
    };
    named = lib.mkEnableOption "the named thing" // {
      default = true;
      example = false;
    };
  };
}
"#;
    create_test_file(temp_dir.path(), "override.nix", content)?;

    let options = collect_options(temp_dir.path(), &[], &HashMap::new(), false, false)?;

    let assert_unique = options
        .iter()
        .find(|o| o.name == "options.test.assertUnique")
        .expect("mkEnableOption \"\" // { default = true; } should still be found");
    assert_eq!(assert_unique.default_value, Some("true".to_string()));
    // With no explicit description, falls back to the option's own leaf
    // name instead of nixpkgs' bare "Whether to enable ." text.
    assert_eq!(
        assert_unique.description,
        Some("Whether to enable `assertUnique`.".to_string())
    );

    let named = options
        .iter()
        .find(|o| o.name == "options.test.named")
        .expect("mkEnableOption with a description, overridden, should still be found");
    assert_eq!(named.default_value, Some("true".to_string()));
    assert_eq!(named.example, Some("false".to_string()));
    assert_eq!(
        named.description,
        Some("Whether to enable the named thing.".to_string())
    );

    Ok(())
}

/// Tests that a submodule type bound to a local `let` variable and
/// referenced by name (`type = listOf includeModule;`, where
/// `includeModule = types.submodule { ... };` is defined elsewhere) is
/// still recursed into, the same as an inline `types.submodule {...}`
/// would be. This is the pattern home-manager's programs/git.nix uses
/// for its `includes` option (see nix-options-doc#5).
#[test]
fn test_let_bound_submodule_type() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;
    let content = r#"
{ lib, ... }:
let
  includeModule = lib.types.submodule {
    options = {
      condition = lib.mkOption {
        type = lib.types.nullOr lib.types.str;
        default = null;
      };
      path = lib.mkOption {
        type = lib.types.str;
      };
    };
  };
in
{
  options.programs.git.includes = lib.mkOption {
    type = lib.types.listOf includeModule;
    default = [ ];
  };
}
"#;
    create_test_file(temp_dir.path(), "letbound.nix", content)?;

    let options = collect_options(temp_dir.path(), &[], &HashMap::new(), false, false)?;

    assert!(options
        .iter()
        .any(|o| o.name == "options.programs.git.includes.<name>.condition"));
    assert!(options
        .iter()
        .any(|o| o.name == "options.programs.git.includes.<name>.path"));

    Ok(())
}

/// Tests that `anchor_slug` produces a stable, name-derived slug, and
/// that Markdown output places an explicit anchor ahead of each
/// heading using that same slug - so a link built against the HTML
/// output's `id` and one built against the Markdown output's anchor
/// land on the same option, and both stay stable across regenerations.
#[test]
fn test_anchor_slug_and_markdown_anchor() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    assert_eq!(
        utils::anchor_slug("services.nginx.enable"),
        "services-nginx-enable"
    );
    assert_eq!(utils::anchor_slug("a:b.c"), "a-b-c");

    let options = vec![OptionDoc {
        name: "services.nginx.enable".to_string(),
        description: None,
        nix_type: "boolean".to_string(),
        default_value: Some("false".to_string()),
        example: None,
        renamed_to: None,
        declarations: vec![Declaration {
            file_path: "nginx.nix".to_string(),
            line_number: 1,
            description: None,
            condition: None,
        }],
    }];

    let markdown = generate_markdown(&options)?;
    assert!(markdown.contains("<a id=\"services-nginx-enable\"></a>"));
    // The anchor comes before the heading it belongs to.
    let anchor_pos = markdown.find("<a id=\"services-nginx-enable\"></a>").unwrap();
    let heading_pos = markdown
        .find("## [`services.nginx.enable`](nginx.nix#L1)")
        .unwrap();
    assert!(anchor_pos < heading_pos);

    Ok(())
}

/// Tests that filter_options resolves a renamed-option shim's mention
/// into a real link to the new option's anchor, and that the link stays
/// correct whether or not --strip-prefix is used - the whole point being
/// that the two must never drift apart, since a link built against the
/// unstripped anchor scheme would 404 once --strip-prefix changes what
/// the real target's anchor actually is.
#[test]
fn test_renamed_option_link_resolution() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;
    create_test_file(
        temp_dir.path(),
        "deprecations.nix",
        r#"
{
  imports = [
    (lib.mkRenamedOptionModule [ "services" "oldName" ] [ "services" "newName" ])
  ];
}
"#,
    )?;

    let options = collect_options(temp_dir.path(), &[], &HashMap::new(), false, false)?;

    // Without --strip-prefix, the target keeps its options. prefix, so
    // the link's anchor needs it too.
    let cli = Cli::parse_from(["program"]);
    let filtered = filter_options(&options, &cli);
    let renamed = filtered
        .iter()
        .find(|o| o.name == "options.services.oldName")
        .expect("renamed option shim should be found");
    assert!(renamed
        .description
        .as_deref()
        .unwrap()
        .contains("Use [`services.newName`](#options-services-newName) instead."));

    // With --strip-prefix (the default, stripping "options."), the
    // target's own anchor loses that prefix once its name is stripped -
    // the link must follow, not point at the now-nonexistent
    // options-prefixed anchor.
    let cli = Cli::parse_from(["program", "--strip-prefix"]);
    let filtered = filter_options(&options, &cli);
    let renamed = filtered
        .iter()
        .find(|o| o.name == "services.oldName")
        .expect("renamed option shim should be found, with options. stripped from its own name");
    assert!(renamed
        .description
        .as_deref()
        .unwrap()
        .contains("Use [`services.newName`](#services-newName) instead."));

    Ok(())
}

/// A submodule type bound via `let` that refers back to itself (a
/// tree-shaped config, e.g. nested filters) used to recurse forever
/// through `types::find_submodule_body` / `parser::parse_attrset` and
/// crash the whole run with a stack overflow, since nothing tracked
/// which submodule bodies were already being expanded (see
/// nix-options-doc#6). This is the issue's own repro: the fix must stop
/// expanding at the point the recursive type is encountered again,
/// while still documenting the option that used it.
#[test]
fn test_recursive_let_bound_submodule_terminates(
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;
    let content = r#"
{ lib, ... }:
let
  filterModule = lib.types.submodule {
    options = {
      name = lib.mkOption {
        type = lib.types.str;
        description = "Filter name";
      };
      children = lib.mkOption {
        type = lib.types.listOf filterModule;
        default = [ ];
        description = "Nested child filters";
      };
    };
  };
in
{
  options.services.demo.filters = lib.mkOption {
    type = lib.types.listOf filterModule;
    default = [ ];
    description = "Tree of filters";
  };
}
"#;
    create_test_file(temp_dir.path(), "recursive.nix", content)?;

    let options = collect_options(temp_dir.path(), &[], &HashMap::new(), false, false)?;

    assert_eq!(options.len(), 3);

    let filters = options
        .iter()
        .find(|o| o.name == "options.services.demo.filters")
        .expect("filters option should be present");
    assert_eq!(filters.nix_type, "list of filterModule");

    let name = options
        .iter()
        .find(|o| o.name == "options.services.demo.filters.<name>.name")
        .expect("filters.<name>.name should be present");
    assert_eq!(name.nix_type, "string");

    let children = options
        .iter()
        .find(|o| o.name == "options.services.demo.filters.<name>.children")
        .expect("filters.<name>.children should be present");
    assert_eq!(children.nix_type, "list of filterModule");

    assert!(!options
        .iter()
        .any(|o| o.name.starts_with("options.services.demo.filters.<name>.children.")));

    Ok(())
}

/// Two `let`-bound submodule types that reference *each other*
/// (`nodeModule` -> `edgeModule` -> `nodeModule` -> ...) is the same
/// unbounded-recursion hazard as the single self-referential case above,
/// just spread across two bindings instead of one. Guarding on the
/// resolved body's text range (rather than the binding name) is what
/// catches this (nix-options-doc#6).
#[test]
fn test_mutually_recursive_submodule_types_terminate(
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;
    let content = r#"
{ lib, ... }:
let
  nodeModule = lib.types.submodule {
    options = {
      label = lib.mkOption {
        type = lib.types.str;
        description = "Node label.";
      };
      edge = lib.mkOption {
        type = lib.types.nullOr edgeModule;
        default = null;
        description = "Outgoing edge, if any.";
      };
    };
  };
  edgeModule = lib.types.submodule {
    options = {
      weight = lib.mkOption {
        type = lib.types.int;
        default = 1;
        description = "Edge weight.";
      };
      target = lib.mkOption {
        type = lib.types.nullOr nodeModule;
        default = null;
        description = "Target node.";
      };
    };
  };
in
{
  options.services.demo.root = lib.mkOption {
    type = lib.types.nullOr nodeModule;
    default = null;
    description = "Root of the graph.";
  };
}
"#;
    create_test_file(temp_dir.path(), "mutual.nix", content)?;

    let options = collect_options(temp_dir.path(), &[], &HashMap::new(), false, false)?;

    assert_eq!(options.len(), 5);

    let names = [
        "options.services.demo.root",
        "options.services.demo.root.label",
        "options.services.demo.root.edge",
        "options.services.demo.root.edge.weight",
        "options.services.demo.root.edge.target",
    ];
    for name in names {
        assert!(
            options.iter().any(|o| o.name == name),
            "expected option {name} to be present"
        );
    }

    assert!(!options
        .iter()
        .any(|o| o.name.starts_with("options.services.demo.root.edge.target.")));

    Ok(())
}

/// A cyclic `let` binding chain used purely as a *type alias*
/// (`let a = b; b = a;`), with no `submodule` anywhere in sight, hits a
/// separate unguarded recursion path than the submodule-expansion cases
/// above: `types::find_submodule_body`'s bare-identifier arm jumps
/// straight to whatever node is bound in `let_bindings`, so a cyclic
/// alias chain loops forever purely inside `types.rs`, before
/// `parser.rs` is ever involved (nix-options-doc#6). The option itself
/// must still be documented, using the raw identifier as its type
/// (`format_ident`'s fallback), with no submodule expansion attempted.
#[test]
fn test_cyclic_let_binding_type_reference() -> Result<(), Box<dyn std::error::Error + Send + Sync>>
{
    let temp_dir = TempDir::new()?;
    let content = r#"
{ lib, ... }:
let
  a = b;
  b = a;
in
{
  options.services.demo.thing = lib.mkOption {
    type = lib.types.listOf a;
    default = [ ];
    description = "Uses a cyclic let-bound type alias.";
  };
}
"#;
    create_test_file(temp_dir.path(), "cyclic.nix", content)?;

    let options = collect_options(temp_dir.path(), &[], &HashMap::new(), false, false)?;

    assert_eq!(options.len(), 1);
    assert_eq!(options[0].name, "options.services.demo.thing");
    assert_eq!(options[0].nix_type, "list of a");

    Ok(())
}

/// The cycle guard is path-scoped (a fresh copy per branch), not a
/// single shared visited set, precisely so that legitimate reuse of the
/// same let-bound submodule type - by independent sibling options, or
/// nested a level deeper through a second binding - keeps expanding
/// normally instead of being misdiagnosed as a cycle the second time it
/// is seen (nix-options-doc#6).
#[test]
fn test_reused_let_bound_submodule_not_flagged_as_cycle(
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;
    let content = r#"
{ lib, ... }:
let
  leafModule = lib.types.submodule {
    options = {
      leaf = lib.mkOption {
        type = lib.types.str;
        description = "A leaf value.";
      };
    };
  };
  midModule = lib.types.submodule {
    options = {
      down = lib.mkOption {
        type = lib.types.listOf leafModule;
        default = [ ];
        description = "Nested leaves, one level down.";
      };
    };
  };
in
{
  options.services.demo.reuseA = lib.mkOption {
    type = lib.types.listOf leafModule;
    default = [ ];
    description = "First sibling reusing leafModule.";
  };
  options.services.demo.reuseB = lib.mkOption {
    type = lib.types.listOf leafModule;
    default = [ ];
    description = "Second sibling reusing leafModule.";
  };
  options.services.demo.deep = lib.mkOption {
    type = midModule;
    description = "Deep nesting through midModule.";
  };
}
"#;
    create_test_file(temp_dir.path(), "reused.nix", content)?;

    let options = collect_options(temp_dir.path(), &[], &HashMap::new(), false, false)?;

    assert!(options
        .iter()
        .any(|o| o.name == "options.services.demo.reuseA.<name>.leaf"));
    assert!(options
        .iter()
        .any(|o| o.name == "options.services.demo.reuseB.<name>.leaf"));
    assert!(options
        .iter()
        .any(|o| o.name == "options.services.demo.deep.down.<name>.leaf"));

    Ok(())
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

/// `--strip-prefix` used `String::replace`, which removes the pattern
/// *everywhere* in the name, not just a leading match. A name that merely
/// contains the pattern mid-string (e.g. a nested submodule path that
/// happens to repeat `options.services.`) must be left with only its
/// leading occurrence removed, and a name that never starts with the
/// pattern must be left untouched entirely (see nix-options-doc#2).
#[test]
fn test_strip_prefix_matches_only_at_start(
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;
    create_test_file(
        temp_dir.path(),
        "strip.nix",
        r#"
{ lib, ... }:
{
  options.services.foo.options.services.bar = lib.mkOption {
    type = lib.types.str;
    default = "x";
    description = "Interior occurrence, also a leading match.";
  };

  options.programs.options.services.baz = lib.mkOption {
    type = lib.types.bool;
    default = false;
    description = "Interior occurrence only, no leading match.";
  };

  options.services.plain = lib.mkOption {
    type = lib.types.bool;
    default = false;
    description = "Control.";
  };
}
"#,
    )?;

    let options = collect_options(temp_dir.path(), &[], &HashMap::new(), false, false)?;
    let cli = Cli::parse_from(["program", "--strip-prefix", "options.services"]);
    let filtered = filter_options(&options, &cli);

    let names: Vec<&str> = filtered.iter().map(|o| o.name.as_str()).collect();
    assert!(
        names.contains(&"foo.options.services.bar"),
        "expected the leading occurrence to be stripped but the interior one preserved, got: {names:?}"
    );
    assert!(
        names.contains(&"options.programs.options.services.baz"),
        "expected a name with no leading match to be left untouched, got: {names:?}"
    );
    assert!(names.contains(&"plain"), "control option missing, got: {names:?}");

    assert!(!names.contains(&"foo.bar"), "interior occurrence was wrongly stripped too");
    assert!(
        !names.contains(&"options.programs.baz"),
        "non-leading occurrence was wrongly stripped"
    );

    Ok(())
}

/// A leading match must be stripped exactly once, not repeatedly - and a
/// genuine leading match must still be stripped even when what's left
/// over is short (e.g. `options.services` under the default pattern
/// `options.` strips down to `services`), guarding against an
/// over-correction that leaves such names untouched (see
/// nix-options-doc#2).
#[test]
fn test_strip_prefix_strips_only_one_leading_occurrence(
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;
    create_test_file(
        temp_dir.path(),
        "dup.nix",
        r#"
{ lib, ... }:
{
  options.options.foo = lib.mkOption {
    type = lib.types.str;
    default = "y";
    description = "Repeated prefix.";
  };

  options.services = lib.mkOption {
    type = lib.types.bool;
    default = false;
    description = "Name equals the pattern minus its trailing dot.";
  };
}
"#,
    )?;

    let options = collect_options(temp_dir.path(), &[], &HashMap::new(), false, false)?;
    // Default pattern is "options.".
    let cli = Cli::parse_from(["program", "--strip-prefix"]);
    let filtered = filter_options(&options, &cli);

    let names: Vec<&str> = filtered.iter().map(|o| o.name.as_str()).collect();
    assert!(
        names.contains(&"options.foo"),
        "expected only the leading `options.` to be stripped once, got: {names:?}"
    );
    assert!(
        names.contains(&"services"),
        "regression guard: a genuine leading match must still be stripped, got: {names:?}"
    );

    Ok(())
}

/// The `renamed_to` anchor resolution loop applies the same
/// `--strip-prefix` stripping as the option-name loop, so the two must
/// stay in lockstep: the shim's rendered link anchor must match the
/// target option's actual post-filter name, derived the same way the
/// target's own name was (see nix-options-doc#2).
#[test]
fn test_renamed_option_anchor_uses_leading_strip_only(
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;
    create_test_file(
        temp_dir.path(),
        "ren.nix",
        r#"
{ lib, ... }:
{
  imports = [
    (lib.mkRenamedOptionModule [ "services" "old" ] [ "services" "foo" "options" "services" "new" ])
  ];

  options.services.foo.options.services.new = lib.mkOption {
    type = lib.types.str;
    default = "z";
    description = "The rename target.";
  };
}
"#,
    )?;

    let options = collect_options(temp_dir.path(), &[], &HashMap::new(), false, false)?;
    let cli = Cli::parse_from(["program", "--strip-prefix", "options.services"]);
    let filtered = filter_options(&options, &cli);

    let target = filtered
        .iter()
        .find(|o| o.name == "foo.options.services.new")
        .expect("rename target should keep its interior `options.services.` occurrence");

    let expected = utils::anchor_slug(&target.name);

    let shim = filtered
        .iter()
        .find(|o| o.name == "old")
        .expect("renamed option shim should be found, with the leading prefix stripped");
    assert!(
        shim.description
            .as_deref()
            .unwrap()
            .contains(&format!("[`services.foo.options.services.new`](#{expected})")),
        "shim description did not link to the target's actual anchor `{expected}`: {:?}",
        shim.description
    );

    Ok(())
}

/// Regression test for #14: raw HTML and dangerous URL schemes embedded in
/// an option's `description` must not survive into the generated HTML
/// document verbatim - the HTML generator must render descriptions with
/// comrak's raw-HTML and dangerous-URL protections switched on (i.e. not
/// `render.unsafe = true`), while still rendering legitimate Markdown
/// (emphasis, code spans, tables, strikethrough, safe links, admonitions).
#[test]
fn test_html_escapes_raw_html_in_description() -> Result<(), Box<dyn std::error::Error + Send + Sync>>
{
    let temp_dir = TempDir::new()?;
    let content = r#"
{
  options.services.evil.enable = lib.mkOption {
    type = lib.types.bool;
    default = false;
    description = ''
      Valid raw HTML: <img src="x" onerror="alert(1)">
      <script>alert('raw-script')</script>

      [click me](javascript:alert(2))

      *markdown* **bold** `code` [link](https://example.com)

      | a | b |
      | --- | --- |
      | 1 | 2 |

      ~~strike~~

      ::: {.note}
      A note.
      :::
    '';
  };
}
"#;
    create_test_file(temp_dir.path(), "flake.nix", content)?;

    let options = collect_options(temp_dir.path(), &[], &HashMap::new(), false, false)?;
    assert_eq!(options.len(), 1);

    let html = generate_html(&options)?;

    // Scope the "must not render live" assertions to the rendered
    // description markup itself (`render.rs:152`/`:154`). The client-side
    // search index (out of scope for this fix - see CLAUDE.md and #14's
    // discussion) intentionally carries the raw, un-rendered description
    // text too, as a JSON-encoded string inside the page's own <script>
    // block; that occurrence is inert (never HTML-parsed) and must not be
    // confused with the comrak-rendered copy this test is checking.
    let desc_start = html
        .find(r#"<div class="option-desc">"#)
        .expect("option-desc block should be present");
    let desc_end = html[desc_start..]
        .find(r#"<div class="option-meta">"#)
        .map(|i| desc_start + i)
        .expect("option-meta block should follow option-desc");
    let desc_html = &html[desc_start..desc_end];

    // The raw <script> and <img onerror> must not appear live, and the
    // javascript: URL scheme must be filtered out of the rendered href.
    assert!(!desc_html.contains("<script>alert("));
    assert!(!desc_html.contains("<img"));
    assert!(!desc_html.contains("javascript:"));

    // The raw HTML source must still be visible as literal text rather
    // than silently dropped.
    assert!(desc_html.contains("&lt;script&gt;"));
    assert!(html.contains("&lt;script&gt;"));

    // Legitimate Markdown formatting must be preserved.
    assert!(html.contains("<em>markdown</em>"));
    assert!(html.contains("<strong>bold</strong>"));
    assert!(html.contains("<code>code</code>"));
    assert!(html.contains("<table>"));
    assert!(html.contains("<del>strike</del>"));
    assert!(html.contains("markdown-alert"));
    assert!(html.contains(r#"<a href="https://example.com">link</a>"#));

    Ok(())
}

/// Regression test for #14 covering the "also declared in" alternate
/// description path (`render.rs:196`), which the primary-description
/// test above does not exercise. Two files declare the same option name
/// with different descriptions; `nix_files.sort()` makes `a.nix` the
/// primary declaration, so `b.nix`'s hostile description lands on the
/// secondary `Declaration` and is rendered via the alt-desc branch.
#[test]
fn test_html_escapes_raw_html_in_alternate_declaration_description(
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;
    let content_a = r#"
{
  options.services.shared.enable = lib.mkOption {
    type = lib.types.bool;
    default = false;
    description = "Primary description.";
  };
}
"#;
    let content_b = r#"
{
  options.services.shared.enable = lib.mkOption {
    type = lib.types.bool;
    default = false;
    description = ''
      Alternate: <script>alert('alt')</script>
    '';
  };
}
"#;
    create_test_file(temp_dir.path(), "a.nix", content_a)?;
    create_test_file(temp_dir.path(), "b.nix", content_b)?;

    let options = collect_options(temp_dir.path(), &[], &HashMap::new(), false, false)?;
    assert_eq!(options.len(), 1);
    assert_eq!(options[0].declarations.len(), 2);

    let html = generate_html(&options)?;

    // The alt-desc branch is exercised at all.
    assert!(html.contains("alt-desc"));
    // The hostile script from the secondary declaration's description
    // must not appear live, only as escaped literal text.
    assert!(!html.contains("<script>alert('alt')"));
    assert!(html.contains("&lt;script&gt;"));

    Ok(())
}

/// Regression test for #15: a double quote in an attacker-controlled
/// value (here, a quoted Nix attribute key forming the option name) must
/// not be able to break out of a double-quoted HTML attribute (`id="..."`)
/// to inject arbitrary attributes such as event handlers. Also asserts
/// that the hardened `anchor_slug` (`src/utils.rs`) restricts its output
/// to `[A-Za-z0-9_-]`.
#[test]
fn test_html_escapes_quotes_in_attributes() -> Result<(), Box<dyn std::error::Error + Send + Sync>>
{
    let temp_dir = TempDir::new()?;
    let content = r#"
{
  options.services."evil\" onmouseover=alert(1) x=\"".enable = lib.mkOption {
    type = lib.types.bool;
    default = false;
    description = "An option with a hostile quoted attribute name.";
  };
}
"#;
    create_test_file(temp_dir.path(), "flake.nix", content)?;

    let options = collect_options(temp_dir.path(), &[], &HashMap::new(), false, false)?;
    assert_eq!(options.len(), 1);

    let html = generate_html(&options)?;

    // The quote must not terminate the id attribute early.
    assert!(!html.contains(r#"id="options-services-""#));

    // Scope the injection check to the `<article ...>` opening tag itself
    // (not the whole document): the hostile text legitimately appears
    // later as visible, harmless text-node content in
    // `<span class="path-prefix">` (encode_text correctly leaves quotes
    // alone there - they don't need escaping outside an attribute value)
    // and inside the JSON-encoded search index string. Only its presence
    // inside the tag's own attribute list would mean the quote broke out
    // of `id="..."` and got tokenized as a real `onmouseover` attribute.
    let article_start = html
        .find("<article")
        .expect("an <article> element should be present");
    let article_tag_end = html[article_start..]
        .find('>')
        .map(|i| article_start + i)
        .expect("the <article> opening tag should be closed");
    let article_open_tag = &html[article_start..=article_tag_end];
    // Check for the live attribute-assignment form specifically (not the
    // bare word "onmouseover"): the sanitized id value legitimately
    // contains "onmouseover" as harmless dash-separated slug text (the
    // hardened `anchor_slug` maps `=` to `-`, so "onmouseover=" can never
    // survive into it), and only "onmouseover=" would mean the quote
    // broke out and a browser tokenized this as a real event handler.
    assert!(!article_open_tag.contains("onmouseover="));

    // Extract the id attribute's value - from `article_open_tag`, not the
    // whole document: searching `html` directly would find the unrelated
    // static `id="theme-toggle"` in the masthead, which appears before
    // any option article and would make this assertion vacuous - and
    // confirm it is restricted to the anchor_slug character set, i.e. it
    // contains no stray quote, space, or `=` that could break out of the
    // attribute.
    let marker = "id=\"";
    let start = article_open_tag
        .find(marker)
        .expect("article's opening tag should have an id attribute")
        + marker.len();
    let end = article_open_tag[start..]
        .find('"')
        .expect("id attribute should be closed")
        + start;
    let id_value = &article_open_tag[start..end];
    assert!(
        id_value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-')),
        "id attribute value {id_value:?} contains characters outside [A-Za-z0-9_-]"
    );

    Ok(())
}

/// Regression test for the gap between #14 and #15: declaration file
/// paths are formatted by hand into `href`/link-target values
/// (`render.rs:144,175,188` and `markdown.rs:36,93`), so they never pass
/// through comrak's `javascript:`/`data:`/`vbscript:` filter, which only
/// runs on links written *inside* a description. A `.nix` file living in
/// a directory whose name is itself a URI scheme (every byte involved is
/// legal in a Unix path) would otherwise become a live, clickable link
/// that executes as that scheme, in both HTML and Markdown output. The
/// trailing `<!--` in the payload matters: browsers treat it as a
/// single-line comment that survives HTML-decoding and swallows the
/// `#L{line}` suffix this tool always appends after the href - without
/// it, that suffix alone would turn a naive payload into a JS syntax
/// error and defeat the exploit unassisted.
///
/// Also covers the *separate* attribute-injection vector (#15) at the
/// same href sites: a declaration file path containing a raw `"` must
/// still be rendered through `html_escape::encode_double_quoted_attribute`
/// (not `encode_text`, which leaves quotes untouched), or the quote
/// breaks out of `href="..."` and a browser tokenizes whatever follows -
/// here ` onmouseover=alert(1)` - as a real, live event-handler attribute
/// on the `<a>` element. `sanitize_link_target` does not (and should not)
/// help here: a bare `"` is not a URI scheme, so this is purely a test of
/// the attribute-escaping call, independent of the scheme-neutralization
/// this test also covers.
///
/// Built directly from `OptionDoc`/`Declaration` (as in
/// `test_markdown_also_declared_in` above) rather than through
/// `collect_options` writing a file to disk, since neither `:` nor `"` is
/// a legal path byte on Windows and this repo's CI runs there too.
#[test]
fn test_dangerous_url_scheme_in_declaration_path_is_neutralized(
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let scheme_payload =
        "javascript:document.querySelector('title').append('-PWNED')<!--/x.nix".to_string();
    let quote_payload = "q\" onmouseover=alert(1)/x.nix".to_string();

    let options = vec![OptionDoc {
        name: "options.test.evil".to_string(),
        description: None,
        nix_type: "boolean".to_string(),
        default_value: Some("false".to_string()),
        example: None,
        renamed_to: None,
        declarations: vec![
            Declaration {
                file_path: scheme_payload.clone(),
                line_number: 1,
                description: None,
                condition: None,
            },
            Declaration {
                file_path: scheme_payload.clone(),
                line_number: 2,
                description: None,
                condition: None,
            },
            Declaration {
                file_path: quote_payload.clone(),
                line_number: 3,
                description: None,
                condition: None,
            },
        ],
    }];

    let html = generate_html(&options)?;
    // Covers all three HTML href sites at once: the option-path heading
    // link and the primary "option-decl" link both use the first
    // (primary) declaration, and the "also declared in" list uses the
    // second and third.
    assert!(!html.to_lowercase().contains("href=\"javascript:"));

    // The quote-payload declaration's own `<a>` opening tag: locate it by
    // its distinctive href prefix, then check only that tag rather than
    // the whole document (the raw payload also appears, harmlessly, as
    // link *text* elsewhere).
    let quote_anchor_start = html
        .find(r#"<a href="q"#)
        .expect("the quote-payload declaration's <a> tag should be present");
    let quote_anchor_end = html[quote_anchor_start..]
        .find('>')
        .map(|i| quote_anchor_start + i)
        .expect("the <a> opening tag should be closed");
    let quote_anchor_tag = &html[quote_anchor_start..=quote_anchor_end];
    assert!(quote_anchor_tag.contains("&quot;"));
    // Note: the payload's " onmouseover=alert(1)" text is still present
    // literally in the escaped output - encode_double_quoted_attribute
    // only escapes `&`/`<`/`>`/`"`, not spaces or `=` - and that is fine:
    // once the payload's own `"` is entity-encoded to `&quot;`, that text
    // is inert, sitting entirely inside the href attribute's value, never
    // tokenized as a separate attribute. So `.contains(" onmouseover=")`
    // is not a meaningful check here (it holds on both correctly-escaped
    // *and* broken output). What actually matters is that the tag has
    // exactly the two raw `"` bytes delimiting `href="..."` and no third,
    // leaked one from the payload - which is exactly what would open a
    // real, separate `onmouseover` attribute for the browser to parse.
    assert_eq!(
        quote_anchor_tag.matches('"').count(),
        2,
        "expected exactly the two href-delimiting quotes in {quote_anchor_tag:?}; \
         a third means the payload's own `\"` leaked through unescaped and broke out \
         of the attribute"
    );

    let markdown = generate_markdown(&options)?;
    // Covers both Markdown link-target sites: the heading (primary
    // declaration) and the "Also declared in" list (second declaration).
    assert!(!markdown.to_lowercase().contains("](javascript:"));

    Ok(())
}

/// `sanitize_link_target`'s allow-list requires a real `http://`/`https://`
/// authority, not just the bare scheme name. A directory literally named
/// `http:` joined with the rest of a declaration's path produces exactly
/// one `/` after the colon (`http:/evil.example/x.nix`) - browsers still
/// resolve that as an authority for "special" schemes like http(s) even
/// with only one slash present (WHATWG URL Standard's leniency for
/// special-scheme authorities), so treating the bare scheme name as safe
/// would let this navigate off-site from what looks like a same-repo
/// source link. A genuine `https://...` URL - exactly what `--out-prefix`
/// produces (see README) - must still pass through unchanged, or that
/// flag stops working.
#[test]
fn test_link_target_requires_real_http_authority(
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let single_slash_payload = "http:/evil.example/x.nix".to_string();
    let real_https_url = "https://github.com/user/repo/blob/main/modules/foo.nix".to_string();

    let options = vec![
        OptionDoc {
            name: "options.test.singleSlash".to_string(),
            description: None,
            nix_type: "boolean".to_string(),
            default_value: Some("false".to_string()),
            example: None,
            renamed_to: None,
            declarations: vec![Declaration {
                file_path: single_slash_payload,
                line_number: 1,
                description: None,
                condition: None,
            }],
        },
        OptionDoc {
            name: "options.test.realHttps".to_string(),
            description: None,
            nix_type: "boolean".to_string(),
            default_value: Some("false".to_string()),
            example: None,
            renamed_to: None,
            declarations: vec![Declaration {
                file_path: real_https_url.clone(),
                line_number: 7,
                description: None,
                condition: None,
            }],
        },
    ];

    let html = generate_html(&options)?;
    // The bare-scheme, single-slash form must be neutralized...
    assert!(!html.contains(r#"href="http:/evil.example"#));
    // ...while a genuine https:// URL, as --out-prefix produces, must
    // survive untouched.
    assert!(html.contains(&format!(r#"href="{real_https_url}#L7""#)));

    Ok(())
}

/// `string_text` used to extract string content by `trim_matches(['"',
/// '\''])`, which strips *any run* of quote characters off either end,
/// not just the delimiters that actually opened/closed the string. Any
/// description that legitimately starts or ends with a quote character
/// (or has one immediately inside the delimiter) lost it, sometimes
/// losing the whole string (see the `"'"` case below). This test locks
/// down that exactly one delimiter pair is removed and nothing else.
#[test]
fn test_description_preserves_boundary_quotes(
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;
    let content = r#"
{
  options.test = {
    trailingQuote = lib.mkOption {
      type = lib.types.str;
      description = "The greeting to use, e.g. 'howdy'";
    };
    fullyQuoted = lib.mkOption {
      type = lib.types.str;
      description = "\"Fully quoted phrase\"";
    };
    indentedBoundaryQuotes = lib.mkOption {
      type = lib.types.str;
      description = ''"Quoted in an indented string"'';
    };
    middleQuotes = lib.mkOption {
      type = lib.types.str;
      description = "He said \"hi\" in the middle";
    };
  };
}
"#;
    create_test_file(temp_dir.path(), "flake.nix", content)?;

    let options = collect_options(temp_dir.path(), &[], &HashMap::new(), false, false)?;
    let find = |name: &str| {
        options
            .iter()
            .find(|o| o.name == format!("options.test.{name}"))
            .unwrap_or_else(|| panic!("option {name} not found"))
    };

    assert_eq!(
        find("trailingQuote").description.as_deref(),
        Some("The greeting to use, e.g. 'howdy'")
    );
    assert_eq!(
        find("fullyQuoted").description.as_deref(),
        Some("\"Fully quoted phrase\"")
    );
    assert_eq!(
        find("indentedBoundaryQuotes").description.as_deref(),
        Some("\"Quoted in an indented string\"")
    );
    assert_eq!(
        find("middleQuotes").description.as_deref(),
        Some("He said \"hi\" in the middle")
    );

    Ok(())
}

/// `string_text` used to hand back the raw source text between the
/// (mis-trimmed) delimiters, so Nix escape sequences like `\"`, `\\`,
/// `\n` and `\t` survived verbatim as two-character sequences instead of
/// being interpreted. This is the same underlying bug as the boundary
/// quote loss (raw source used as a value) - fixing one without the
/// other would still leave `He said \"hi\"` in the output.
#[test]
fn test_description_unescapes_escape_sequences(
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;
    // Use a raw Rust string literal so the backslashes below reach the
    // Nix source file verbatim (not interpreted by Rust first).
    let content = r#"
{
  options.test.escapes = lib.mkOption {
    type = lib.types.str;
    description = "Escapes: newline\nhere, tab\there, backslash\\here";
  };
}
"#;
    create_test_file(temp_dir.path(), "flake.nix", content)?;

    let options = collect_options(temp_dir.path(), &[], &HashMap::new(), false, false)?;
    let escapes = options
        .iter()
        .find(|o| o.name == "options.test.escapes")
        .unwrap();

    assert_eq!(
        escapes.description.as_deref(),
        Some("Escapes: newline\nhere, tab\there, backslash\\here")
    );

    Ok(())
}

/// Nix's `''`-string escape forms (`''$` for a literal `$`, `'''` for a
/// literal `''`) are distinct from double-quoted string escapes and were
/// equally unhandled by the old raw-text `string_text`. `normalized_parts`
/// handles both in the same pass as ordinary escapes.
#[test]
fn test_indented_description_unescapes_dollar_and_quote_escapes(
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;
    let content = r#"
{
  options.test.indentedEscapes = lib.mkOption {
    type = lib.types.str;
    description = ''Literal ''${escaped} and '''quotes''' here'';
  };
}
"#;
    create_test_file(temp_dir.path(), "flake.nix", content)?;

    let options = collect_options(temp_dir.path(), &[], &HashMap::new(), false, false)?;
    let indented = options
        .iter()
        .find(|o| o.name == "options.test.indentedEscapes")
        .unwrap();

    assert_eq!(
        indented.description.as_deref(),
        Some("Literal ${escaped} and ''quotes'' here")
    );

    Ok(())
}

/// The same `string_text` bug affects every call site that routes
/// through it, not just `mkOption`'s `description` field:
/// `mkEnableOption`'s subject text, `mkPackageOption`'s `extraDescription`
/// override, and `mkRemovedOptionModule`'s removal message.
#[test]
fn test_option_helper_descriptions_preserve_quotes(
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;
    let content = r#"
{
  options.test.enable = lib.mkEnableOption "support for 'foo'";

  options.test.pkg = lib.mkPackageOption pkgs "hello" {
    extraDescription = "Use \"hello\"";
  };

  imports = [
    (lib.mkRemovedOptionModule [ "services" "goneName" ] "See 'docs' for details.")
  ];
}
"#;
    create_test_file(temp_dir.path(), "flake.nix", content)?;

    let options = collect_options(temp_dir.path(), &[], &HashMap::new(), false, false)?;

    let enable = options
        .iter()
        .find(|o| o.name == "options.test.enable")
        .unwrap();
    assert_eq!(
        enable.description.as_deref(),
        Some("Whether to enable support for 'foo'.")
    );

    let pkg = options
        .iter()
        .find(|o| o.name == "options.test.pkg")
        .unwrap();
    assert!(
        pkg.description
            .as_deref()
            .unwrap()
            .ends_with("Use \"hello\""),
        "got: {:?}",
        pkg.description
    );

    let removed = options
        .iter()
        .find(|o| o.name == "options.services.goneName")
        .unwrap();
    assert!(
        removed
            .description
            .as_deref()
            .unwrap()
            .contains("See 'docs' for details."),
        "got: {:?}",
        removed.description
    );

    Ok(())
}

/// Locks the two intentional behavior changes from properly applying
/// Nix's own indentation semantics (via `normalized_parts`) instead of
/// `custom_dedent`, which dedented by the string's own *common* indent
/// across all lines after the first:
///
/// 1. A `''`-string whose content starts on its own line right after
///    `''` no longer carries a leading `\n` in the decoded value.
/// 2. A `''`-string whose content starts on the *same* line as the
///    opening `''` computes a minimum indentation of 0 (Nix considers
///    only *subsequent* lines when computing the common indent), so
///    later lines keep their indentation rather than being flattened -
///    matching what `nix eval` produces.
#[test]
fn test_indented_description_uses_nix_indentation_semantics(
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;
    let content = r#"
{
  options.test.leadingNewline = lib.mkOption {
    type = lib.types.str;
    description = ''
      Intro:
          code
    '';
  };
  options.test.sameLineStart = lib.mkOption {
    type = lib.types.str;
    description = ''First line
        second
    '';
  };
}
"#;
    create_test_file(temp_dir.path(), "flake.nix", content)?;

    let options = collect_options(temp_dir.path(), &[], &HashMap::new(), false, false)?;

    let leading = options
        .iter()
        .find(|o| o.name == "options.test.leadingNewline")
        .unwrap();
    assert_eq!(
        leading.description.as_deref(),
        Some("Intro:\n    code\n")
    );

    let same_line = options
        .iter()
        .find(|o| o.name == "options.test.sameLineStart")
        .unwrap();
    assert_eq!(
        same_line.description.as_deref(),
        Some("First line\n        second\n")
    );

    Ok(())
}

/// Guard test - passes both before and against the fix, but is the test
/// that fails (with a panic, not an assertion failure) if the
/// well-formedness guard is ever removed from `string_text`: rnix
/// 0.14's `ast::Str::normalized_parts` asserts on the error-recovery
/// nodes an unterminated string produces, which would turn this crate's
/// deliberate "unparseable input degrades to zero options" convention
/// into a process-wide panic. Keep this test even though it doesn't
/// currently distinguish old vs. new behavior - it's the regression
/// guard for the guard.
#[test]
fn test_malformed_string_degrades_without_panic(
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;
    let content = r#"
{
  options.test.broken = lib.mkOption {
    type = lib.types.str;
    description = "unterminated
"#;
    create_test_file(temp_dir.path(), "flake.nix", content)?;

    // Must not panic, regardless of how many (or zero) options it finds.
    let result = collect_options(temp_dir.path(), &[], &HashMap::new(), false, false);
    assert!(result.is_ok());

    Ok(())
}

/// Interpolations in a description (`${...}`) are statically
/// unresolvable, so `string_text` must re-emit them as their `${...}`
/// source text rather than dropping them - otherwise `--replace` (which
/// substitutes into that same `${var}` syntax downstream) would have
/// nothing left to match.
///
/// Like `test_malformed_string_degrades_without_panic`, this is a
/// lock-in guard rather than a regression test - it already passed
/// before the `string_text` rewrite. Keep it anyway: it's what would
/// catch a plausible *wrong* implementation of `normalized_parts` handling
/// that dropped `InterpolPart::Interpolation` instead of re-emitting it.
#[test]
fn test_description_interpolation_still_replaceable(
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;
    let content = r#"
{
  options.test.interpolated = lib.mkOption {
    type = lib.types.str;
    description = "Uses ${name} and ${config.foo}";
  };
}
"#;
    create_test_file(temp_dir.path(), "flake.nix", content)?;

    let mut replacements = HashMap::new();
    replacements.insert("name".to_string(), "NixOS".to_string());

    let options = collect_options(temp_dir.path(), &[], &replacements, false, false)?;
    let interpolated = options
        .iter()
        .find(|o| o.name == "options.test.interpolated")
        .unwrap();

    assert_eq!(
        interpolated.description.as_deref(),
        Some("Uses NixOS and ${config.foo}")
    );

    Ok(())
}

/// Extracts the JS array literal from `const <name> = …;` in the
/// generated page. A literal newline can never occur inside the JSON
/// (serde_json escapes them), so the first `;\n` after the declaration
/// is its terminator.
fn extract_js_array_literal<'a>(html: &'a str, decl: &str) -> &'a str {
    let start = html.find(decl).expect("declaration should be present") + decl.len();
    let end = start + html[start..].find(";\n").expect("declaration should be terminated");
    &html[start..end]
}

/// Regression test for #16, bug 1: `generate_html` used to splice the two
/// index arrays into the search script with two sequential
/// `String::replace` calls (`__SEARCH_INDEX__` then `__CATEGORY_INDEX__`).
/// The second `replace` scanned the *already-substituted* output, so a
/// description that merely mentions the literal text `__CATEGORY_INDEX__`
/// got matched by that second pass and had raw category-index JSON spliced
/// into the middle of the search index's JS string literal, corrupting it.
/// The fix (`split_once` over the pristine template) never rescans
/// inserted data, so the placeholder text must survive verbatim as inert
/// string content.
#[test]
fn test_html_search_index_survives_placeholder_text_in_description(
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;
    let content = r#"
{
  options.services.testA.enable = lib.mkOption {
    type = lib.types.bool;
    default = false;
    description = "See __CATEGORY_INDEX__ marker here";
  };
  options.services.testB.enable = lib.mkOption {
    type = lib.types.bool;
    default = false;
    description = "Second option description.";
  };
}
"#;
    create_test_file(temp_dir.path(), "flake.nix", content)?;

    let options = collect_options(temp_dir.path(), &[], &HashMap::new(), false, false)?;
    assert_eq!(options.len(), 2);

    let html = generate_html(&options)?;

    let literal = extract_js_array_literal(&html, "const searchText = ");
    // Pre-fix, the injected raw JSON in the middle of the literal breaks
    // JSON parsing entirely.
    let entries: Vec<String> =
        serde_json::from_str(literal).expect("searchText literal should be valid JSON");
    assert!(
        entries[0].contains("__CATEGORY_INDEX__"),
        "expected the placeholder text to survive verbatim in {:?}",
        entries[0]
    );

    Ok(())
}

/// Regression test for #16, bug 1: a description containing *both*
/// placeholder markers - plus a pre-escaped `<\/script>` sequence and
/// non-ASCII text, to make sure none of those interact badly with the
/// fix - must still round-trip through the search index exactly.
#[test]
fn test_html_search_index_placeholder_text_in_both_markers(
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;
    let content = r#"
{
  options.services.test.enable = lib.mkOption {
    type = lib.types.bool;
    default = false;
    description = "Mentions __SEARCH_INDEX__ and __CATEGORY_INDEX__ plus <\/script> and non-ascii é中.";
  };
}
"#;
    create_test_file(temp_dir.path(), "flake.nix", content)?;

    let options = collect_options(temp_dir.path(), &[], &HashMap::new(), false, false)?;
    assert_eq!(options.len(), 1);
    let description = options[0]
        .description
        .clone()
        .expect("description should be present");

    let html = generate_html(&options)?;

    let literal = extract_js_array_literal(&html, "const searchText = ");
    let entries: Vec<String> =
        serde_json::from_str(literal).expect("searchText literal should be valid JSON");
    assert!(
        entries[0].contains(&description),
        "expected the full description to round-trip verbatim in {:?}",
        entries[0]
    );

    Ok(())
}

/// Regression test for #16, bug 2 (reported in the issue's comments): a
/// description containing the literal sequence `<!--<script>` used to
/// drive the HTML tokenizer into script-data-double-escaped state. The
/// page's own closing `</script>` tag then only demoted the tokenizer
/// back to escaped state instead of ending the element, silently
/// absorbing the rest of the document - including the footer and every
/// later `<script>` - into the search script's text content, with no
/// console error. The old `.replace("</", "<\\/")` guard only addressed
/// the `</script` case; it did nothing for `<!--` or bare `<script`. The
/// fix escapes every `<` in the serialized JSON, so none of those
/// tokenizer-state-changing sequences can appear at all.
#[test]
fn test_html_search_index_escapes_angle_brackets(
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;
    let content = r#"
{
  options.services.test.enable = lib.mkOption {
    type = lib.types.bool;
    default = false;
    description = "Payload: <!--<script>";
  };
}
"#;
    create_test_file(temp_dir.path(), "flake.nix", content)?;

    let options = collect_options(temp_dir.path(), &[], &HashMap::new(), false, false)?;
    assert_eq!(options.len(), 1);

    let html = generate_html(&options)?;

    let search_literal = extract_js_array_literal(&html, "const searchText = ");
    let category_literal = extract_js_array_literal(&html, "const categoryIndex = ");

    // (a) The emitted JS literal must contain no raw `<` at all - that is
    // what makes `<!--`, `<script`, and `</script` unrepresentable.
    assert!(
        !search_literal.contains('<'),
        "searchText literal should contain no raw '<': {search_literal:?}"
    );
    assert!(
        !category_literal.contains('<'),
        "categoryIndex literal should contain no raw '<': {category_literal:?}"
    );

    // (b) The escape is data-preserving: the entry still parses and still
    // contains the literal payload text once JSON-decoded.
    let entries: Vec<String> =
        serde_json::from_str(search_literal).expect("searchText literal should be valid JSON");
    assert!(entries[0].contains("<!--<script>"));

    // (c) The dangerous sequence must not appear anywhere in the document
    // at all - i.e. it never reached the tokenizer as raw markup.
    assert!(!html.contains("<!--<script>"));

    Ok(())
}

/// Guards the `.expect()` calls at the `SEARCH_SCRIPT_TEMPLATE` injection
/// site in `generate_html`, which assume each placeholder occurs exactly
/// once so `split_once` cleanly separates the template into head/middle/
/// tail. If a future template edit accidentally duplicated a placeholder,
/// `split_once` would silently treat only the first occurrence as the
/// marker and leave the rest as literal dead text; this test turns that
/// into a loud, immediate test failure instead.
///
/// The injection site also assumes `__SEARCH_INDEX__` appears *before*
/// `__CATEGORY_INDEX__` - it calls `split_once("__SEARCH_INDEX__")` first,
/// then `split_once("__CATEGORY_INDEX__")` on the remainder. A template
/// edit that swapped their order would still pass the uniqueness checks
/// above yet panic at runtime (the second `split_once` would find nothing
/// to split on), so assert the order explicitly too.
#[test]
fn test_search_script_template_placeholders_are_unique() {
    assert_eq!(
        crate::generate::html::SEARCH_SCRIPT_TEMPLATE
            .matches("__SEARCH_INDEX__")
            .count(),
        1
    );
    assert_eq!(
        crate::generate::html::SEARCH_SCRIPT_TEMPLATE
            .matches("__CATEGORY_INDEX__")
            .count(),
        1
    );

    let search_pos = crate::generate::html::SEARCH_SCRIPT_TEMPLATE
        .find("__SEARCH_INDEX__")
        .expect("__SEARCH_INDEX__ should be present");
    let category_pos = crate::generate::html::SEARCH_SCRIPT_TEMPLATE
        .find("__CATEGORY_INDEX__")
        .expect("__CATEGORY_INDEX__ should be present");
    assert!(
        search_pos < category_pos,
        "generate_html's split_once chain assumes __SEARCH_INDEX__ precedes \
         __CATEGORY_INDEX__ in the template"
    );
}

/// Regression test for #9: `main` used to `return Ok(())` before calling
/// `generate_doc` at all when no options were found, so `--out` was never
/// written. This pins the other half of the fix - `generate_doc` itself
/// must produce a well-formed, non-empty Markdown document for an empty
/// slice, so that falling through to it in `main` is actually safe.
#[test]
fn test_generate_doc_empty_options_markdown() -> Result<(), Box<dyn std::error::Error + Send + Sync>>
{
    let output = generate_doc(&[], OutputFormat::Markdown, false)?;
    assert!(output.contains("# NixOS Module Options"));
    assert!(output.contains("*Generated with [nix-options-doc]"));
    Ok(())
}

/// Regression test for #9: pins the public JSON schema for the empty case.
/// A consumer of `--format json` must get a valid empty array, never prose
/// or an error, when zero options are found.
#[test]
fn test_generate_doc_empty_options_json() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let output = generate_doc(&[], OutputFormat::Json, false)?;
    assert_eq!(output.trim(), "[]");
    let parsed: Vec<serde_json::Value> = serde_json::from_str(&output)?;
    assert!(parsed.is_empty());
    Ok(())
}

/// Regression test for #9: pins "header row only, no filler row" for
/// `--format csv` over zero options.
#[test]
fn test_generate_doc_empty_options_csv() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let output = generate_doc(&[], OutputFormat::Csv, false)?;
    let mut lines = output.lines();
    assert_eq!(
        lines.next(),
        Some("Option,Type,Default,Example,Description,Declarations")
    );
    let non_empty_lines = output.lines().filter(|line| !line.is_empty()).count();
    assert_eq!(non_empty_lines, 1);
    Ok(())
}

/// Regression test for #9: guards the `split_once(...).expect(...)` calls
/// in the HTML search-index injection site against regressing on the
/// zero-option path - the splice must still happen (no leftover
/// placeholders) and both index arrays must serialize as an empty JSON
/// array rather than being omitted or malformed.
#[test]
fn test_generate_doc_empty_options_html() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let html = generate_doc(&[], OutputFormat::Html, false)?;
    assert!(html.contains("<!DOCTYPE html>"));
    assert!(html.contains("<strong>0</strong> options"));
    assert!(!html.contains("__SEARCH_INDEX__"));
    assert!(!html.contains("__CATEGORY_INDEX__"));

    let search_literal = extract_js_array_literal(&html, "const searchText = ");
    let category_literal = extract_js_array_literal(&html, "const categoryIndex = ");
    assert_eq!(search_literal, "[]");
    assert_eq!(category_literal, "[]");

    Ok(())
}
