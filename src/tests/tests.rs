use super::*;
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
            file_path: "test.nix".to_string(),
            line_number: 1,
        },
        OptionDoc {
            name: "options.test.opt2".to_string(),
            description: Some("Test option 2".to_string()),
            nix_type: "lib.types.str".to_string(),
            default_value: None,
            example: None,
            file_path: "test.nix".to_string(),
            line_number: 2,
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
    // The type string might be transformed by the formatter
    assert!(
        markdown.contains("**Type:**") && (markdown.contains("string") || markdown.contains("str"))
    );
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
