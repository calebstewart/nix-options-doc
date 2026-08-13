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
