use super::*;

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
    assert_eq!(options[0].nix_type, "boolean");
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
    assert_eq!(string_opt.nix_type, "string");
    assert_eq!(string_opt.description, Some("A string option".to_string()));
    assert_eq!(string_opt.default_value, Some("\"test\"".to_string()));

    let nested_opt = options
        .iter()
        .find(|o| o.name == "options.test.complex.nested.value")
        .unwrap();
    assert_eq!(nested_opt.nix_type, "signed integer");
    assert_eq!(
        nested_opt.description,
        Some("A nested number option".to_string())
    );
    assert_eq!(nested_opt.default_value, None);

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

    assert_eq!(sorted_options[0].nix_type, "list of string");
    assert_eq!(sorted_options[1].nix_type, "list of signed integer");

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
    assert_eq!(
        plain.description,
        Some("The hello package to use.".to_string())
    );

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
    (lib.mkRenamedOptionModule [ "services" "tickName" ] [ "services" "new`tick" ])
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

    // Regression guard for nix-options-doc#49 at the level of
    // `parser::find_deprecations` alone (before filter_options gets a
    // chance to touch it): a rename target containing a backtick must
    // widen the shim description's code span delimiter to two backticks,
    // rather than the fixed single backtick that would otherwise close
    // the span early. `renamed_to` keeps holding the raw, unmodified
    // target - the JSON contract is unaffected by this fix.
    let ticked = options
        .iter()
        .find(|o| o.name == "options.services.tickName")
        .expect("renamed option shim (backtick target) should be found");
    assert_eq!(ticked.renamed_to.as_deref(), Some("services.new`tick"));
    assert!(ticked
        .description
        .as_deref()
        .unwrap()
        .contains("Use ``services.new`tick`` instead."));

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

    assert!(!options.iter().any(|o| o
        .name
        .starts_with("options.services.demo.filters.<name>.children.")));

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

    assert!(!options.iter().any(|o| o
        .name
        .starts_with("options.services.demo.root.edge.target.")));

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
    assert_eq!(leading.description.as_deref(), Some("Intro:\n    code\n"));

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

/// A wide-but-shallow chain of distinct `let`-bound submodule types - each
/// `m{i}` declares three options of type `m{i-1}` - fans out combinatorially
/// (~3^depth options) while staying well under `MAX_SUBMODULE_DEPTH` and
/// never revisiting a body on any single path, so neither existing guard in
/// the submodule-expansion arm of `parse_attrset` fires
/// (nix-options-doc#21). Without a total-work budget this either runs for
/// tens of seconds or OOMs; this test asserts the emitted option count is
/// bounded, so a regression here fails fast instead of hanging.
#[test]
fn test_submodule_fanout_is_bounded() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;

    let mut content = String::from("{ lib, ... }:\nlet\n");
    content.push_str(
        "  m0 = lib.types.submodule {\n    options = {\n      leaf = lib.mkOption {\n        type = lib.types.str;\n        description = \"Leaf value.\";\n      };\n    };\n  };\n",
    );
    for i in 1..=9 {
        let prev = i - 1;
        content.push_str(&format!(
            "  m{i} = lib.types.submodule {{\n    options = {{\n      o0 = lib.mkOption {{ type = m{prev}; description = \"o0\"; }};\n      o1 = lib.mkOption {{ type = m{prev}; description = \"o1\"; }};\n      o2 = lib.mkOption {{ type = m{prev}; description = \"o2\"; }};\n    }};\n  }};\n"
        ));
    }
    content.push_str(
        "in\n{\n  options.services.demo.root = lib.mkOption {\n    type = m9;\n    description = \"Root option.\";\n  };\n}\n",
    );

    create_test_file(temp_dir.path(), "fanout.nix", &content)?;

    let options = collect_options(temp_dir.path(), &[], &HashMap::new(), false, false)?;

    // The budget really was the thing that stopped it: the input is
    // genuinely pathological, not accidentally small.
    assert!(options.len() >= crate::types::MAX_SUBMODULE_EXPANSION_OPTIONS);
    // The cap bounds the result, but not to a constant: once the budget is
    // exhausted no *new* submodule body is expanded, yet every frame already
    // in flight still emits its own body's remaining direct options as the
    // recursion unwinds. The cycle guard makes each in-flight frame a
    // distinct body, so the overshoot is bounded by the options declared in
    // the file - depth x breadth, linear in file size (nix-options-doc#46;
    // see the `budget.is_exhausted()` arm of `parse_attrset`). This input
    // declares only 29 options, so its overshoot is necessarily tiny; that
    // is a property of *this* input, not a guarantee. The general bound is
    // exercised by `test_submodule_fanout_overshoot_is_bounded_by_file_size`.
    let declared = content.matches("mkOption").count();
    assert!(options.len() <= crate::types::MAX_SUBMODULE_EXPANSION_OPTIONS + declared);

    assert!(options
        .iter()
        .any(|o| o.name == "options.services.demo.root"));

    Ok(())
}

/// The overshoot past `MAX_SUBMODULE_EXPANSION_OPTIONS` is depth x breadth,
/// not a constant (nix-options-doc#46): when the budget trips, no new
/// submodule body is expanded, but every already-in-flight frame still emits
/// its own body's remaining direct options as the recursion unwinds. Each
/// in-flight frame is a distinct body (the cycle guard in `parse_attrset`
/// ensures that), so the overshoot is bounded by the options declared in the
/// file - linear in file size.
///
/// `test_submodule_fanout_is_bounded` cannot show this: its input declares 29
/// options, so its overshoot is 11 no matter what. This one pads every
/// submodule body with plain options *after* the two options that drive the
/// fan-out, so the unwind path has real work left to do - it emits 11,219
/// options, an overshoot of 1,219. The assertion is the real invariant, so a
/// regression that made the overshoot super-linear (re-expanding bodies after
/// exhaustion, or resetting the budget per branch) fails here.
#[test]
fn test_submodule_fanout_overshoot_is_bounded_by_file_size(
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Kept small on purpose: this input already runs ~1.6 s in a debug test
    // binary, and the cost grows with both constants.
    const DEPTH: usize = 12;
    const BREADTH: usize = 100;

    let temp_dir = TempDir::new()?;

    let mut content = String::from("{ lib, ... }:\nlet\n");
    content.push_str("  m0 = lib.types.submodule {\n    options = {\n");
    for j in 0..BREADTH {
        content.push_str(&format!(
            "      p{j} = lib.mkOption {{ type = lib.types.str; description = \"p{j}\"; }};\n"
        ));
    }
    content.push_str("    };\n  };\n");
    for i in 1..=DEPTH {
        let prev = i - 1;
        content.push_str(&format!(
            "  m{i} = lib.types.submodule {{\n    options = {{\n"
        ));
        for k in 0..2 {
            content.push_str(&format!(
                "      d{k} = lib.mkOption {{ type = m{prev}; description = \"d{k}\"; }};\n"
            ));
        }
        for j in 0..BREADTH {
            content.push_str(&format!(
                "      p{j} = lib.mkOption {{ type = lib.types.str; description = \"p{j}\"; }};\n"
            ));
        }
        content.push_str("    };\n  };\n");
    }
    content.push_str(&format!(
        "in\n{{\n  options.services.demo.root = lib.mkOption {{ type = m{DEPTH}; description = \"root\"; }};\n}}\n"
    ));

    create_test_file(temp_dir.path(), "padded_fanout.nix", &content)?;

    let options = collect_options(temp_dir.path(), &[], &HashMap::new(), false, false)?;

    // The budget really fired - the input is genuinely pathological.
    assert!(options.len() >= crate::types::MAX_SUBMODULE_EXPANSION_OPTIONS);
    // ...and the overshoot stays within the options the file declares.
    let declared = content.matches("mkOption").count();
    assert!(options.len() <= crate::types::MAX_SUBMODULE_EXPANSION_OPTIONS + declared);

    Ok(())
}

/// Guards against tempting-but-wrong fixes for nix-options-doc#21, such as
/// lowering `MAX_SUBMODULE_DEPTH` or implementing the expansion budget as a
/// depth/count limit that also truncates legitimate deep-but-narrow module
/// chains. A chain of 21 distinct submodule levels, each declaring exactly
/// one option, is well under `MAX_SUBMODULE_DEPTH` (32) and emits nowhere
/// near `MAX_SUBMODULE_EXPANSION_OPTIONS` (10,000) options, so it must
/// expand in full, all the way to the deepest leaf.
#[test]
fn test_deep_narrow_submodule_chain_still_fully_expands(
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;

    let mut content = String::from("{ lib, ... }:\nlet\n");
    content.push_str(
        "  m0 = lib.types.submodule {\n    options = {\n      leaf = lib.mkOption {\n        type = lib.types.str;\n        description = \"Leaf value.\";\n      };\n    };\n  };\n",
    );
    for i in 1..=20 {
        let prev = i - 1;
        content.push_str(&format!(
            "  m{i} = lib.types.submodule {{\n    options = {{\n      down = lib.mkOption {{ type = m{prev}; description = \"down\"; }};\n    }};\n  }};\n"
        ));
    }
    content.push_str(
        "in\n{\n  options.services.demo.root = lib.mkOption {\n    type = m20;\n    description = \"Root option.\";\n  };\n}\n",
    );

    create_test_file(temp_dir.path(), "deep_narrow.nix", &content)?;

    let options = collect_options(temp_dir.path(), &[], &HashMap::new(), false, false)?;

    let deepest = format!("options.services.demo.root{}.leaf", ".down".repeat(20));
    assert!(options.iter().any(|o| o.name == deepest));

    // Root + 20 `down` options + the final `leaf` - nothing truncated.
    assert_eq!(options.len(), 22);

    Ok(())
}
