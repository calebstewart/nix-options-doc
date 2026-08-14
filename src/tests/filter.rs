use super::*;

/// Tests that `filter_options` resolves a renamed-option shim's mention
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

/// `--strip-prefix` used `String::replace`, which removes the pattern
/// *everywhere* in the name, not just a leading match. A name that merely
/// contains the pattern mid-string (e.g. a nested submodule path that
/// happens to repeat `options.services.`) must be left with only its
/// leading occurrence removed, and a name that never starts with the
/// pattern must be left untouched entirely (see nix-options-doc#2).
#[test]
fn test_strip_prefix_matches_only_at_start() -> Result<(), Box<dyn std::error::Error + Send + Sync>>
{
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
    assert!(
        names.contains(&"plain"),
        "control option missing, got: {names:?}"
    );

    assert!(
        !names.contains(&"foo.bar"),
        "interior occurrence was wrongly stripped too"
    );
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

    let expected = crate::utils::anchor_slug(&target.name);

    let shim = filtered
        .iter()
        .find(|o| o.name == "old")
        .expect("renamed option shim should be found, with the leading prefix stripped");
    assert!(
        shim.description.as_deref().unwrap().contains(&format!(
            "[`services.foo.options.services.new`](#{expected})"
        )),
        "shim description did not link to the target's actual anchor `{expected}`: {:?}",
        shim.description
    );

    Ok(())
}

/// The `--strip-prefix` help text is user-facing documentation of
/// `filter_options`' prefix normalization, and it drifted from the code:
/// it claimed the value "must start with 'options.'" (no such validation
/// exists) and that the no-value default was `option.` (a typo for
/// `options.`) - see nix-options-doc#13. Guards the wording against
/// regressing to either claim, and pins the no-value default to the
/// value the help text promises.
#[test]
fn test_strip_prefix_help_text_matches_behavior(
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use clap::CommandFactory;

    let cmd = Cli::command();
    let arg = cmd
        .get_arguments()
        .find(|a| a.get_id() == "strip_prefix")
        .expect("--strip-prefix argument should exist");
    let help = arg
        .get_long_help()
        .or_else(|| arg.get_help())
        .expect("--strip-prefix should have help text")
        .to_string();

    // `options.` legitimately appears; a bare `option.` never should.
    assert!(
        !help.replace("options.", "").contains("option."),
        "help text still contains the `option.` typo: {help}"
    );
    assert!(
        !help.to_lowercase().contains("must start with"),
        "help text still claims a constraint that is not enforced: {help}"
    );

    let cli = Cli::parse_from(["program", "--strip-prefix"]);
    assert_eq!(
        cli.filter.strip_prefix.as_deref(),
        Some("options."),
        "the no-value default must stay the `options.` the help text documents"
    );

    Ok(())
}

/// A bare prefix (one that does not start with `options.`) is valid and
/// is normalized to `options.<PREFIX>.`; an explicit prefix without a
/// trailing dot gets one appended; an empty value means `options.`.
/// Nothing rejects a bare prefix, so a "fix" for nix-options-doc#13 that
/// enforced the help text's old claim instead of correcting it would be
/// a breaking behavior change - this pins the behavior the corrected
/// wording describes.
#[test]
fn test_strip_prefix_accepts_bare_prefix() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;
    create_test_file(
        temp_dir.path(),
        "bare.nix",
        r#"
{ lib, ... }:
{
  options.services.foo.enable = lib.mkOption {
    type = lib.types.bool;
    default = false;
    description = "Enable foo.";
  };

  options.services.bar.enable = lib.mkOption {
    type = lib.types.bool;
    default = false;
    description = "Enable bar.";
  };
}
"#,
    )?;

    let options = collect_options(temp_dir.path(), &[], &HashMap::new(), false, false)?;

    // Bare prefix: normalized to `options.services.foo.`.
    let cli = Cli::parse_from(["program", "--strip-prefix", "services.foo"]);
    let names: Vec<String> = filter_options(&options, &cli)
        .into_iter()
        .map(|o| o.name)
        .collect();
    assert!(
        names.iter().any(|n| n == "enable"),
        "a bare prefix should be treated as `options.<PREFIX>`, got: {names:?}"
    );
    assert!(
        names.iter().any(|n| n == "options.services.bar.enable"),
        "non-matching names must be left untouched, got: {names:?}"
    );

    // Explicit prefix, no trailing dot: same result.
    let cli = Cli::parse_from(["program", "--strip-prefix", "options.services.foo"]);
    let explicit: Vec<String> = filter_options(&options, &cli)
        .into_iter()
        .map(|o| o.name)
        .collect();
    assert_eq!(
        explicit, names,
        "an explicit `options.`-prefixed value should behave like the bare one"
    );

    // Empty value: same as passing the flag with no value at all.
    let cli = Cli::parse_from(["program", "--strip-prefix", ""]);
    let empty: Vec<String> = filter_options(&options, &cli)
        .into_iter()
        .map(|o| o.name)
        .collect();
    assert!(
        empty.iter().any(|n| n == "services.foo.enable"),
        "an empty value should strip `options.`, got: {empty:?}"
    );

    Ok(())
}
