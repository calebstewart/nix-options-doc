use super::*;

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
    assert!(crate::utils::should_traverse_entry(&root));

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

    assert_eq!(crate::utils::convert_admonitions(input), expected);
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

    assert_eq!(crate::utils::clean_description(input), expected);
}

/// Tests that a blockquote written as `> **Warning:** ...` (a common
/// informal convention outside nixpkgs' own Pandoc-based docs tooling)
/// is recognized as an admonition, not left as a plain blockquote.
#[test]
fn test_blockquote_admonition_conversion() {
    let input = "Expose the service to the internet.\n\n> **Warning:** Do *not* enable this without setting up authentication first!\n";

    let expected = "Expose the service to the internet.\n\n> [!WARNING]\n> Do *not* enable this without setting up authentication first!\n";

    assert_eq!(
        crate::utils::convert_blockquote_admonitions(input),
        expected
    );

    // Descriptions with no such blockquote are returned byte-for-byte
    // unchanged (not just semantically equivalent).
    let plain = "Just a normal description.\n";
    assert_eq!(crate::utils::convert_blockquote_admonitions(plain), plain);
}

/// Tests that `anchor_slug` produces a stable, name-derived slug, and
/// that Markdown output places an explicit anchor ahead of each
/// heading using that same slug - so a link built against the HTML
/// output's `id` and one built against the Markdown output's anchor
/// land on the same option, and both stay stable across regenerations.
#[test]
fn test_anchor_slug_and_markdown_anchor() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    assert_eq!(
        crate::utils::anchor_slug("services.nginx.enable"),
        "services-nginx-enable"
    );
    assert_eq!(crate::utils::anchor_slug("a:b.c"), "a-b-c");

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
    let anchor_pos = markdown
        .find("<a id=\"services-nginx-enable\"></a>")
        .unwrap();
    let heading_pos = markdown
        .find("## [`services.nginx.enable`](<nginx.nix#L1>)")
        .unwrap();
    assert!(anchor_pos < heading_pos);

    Ok(())
}

/// Guards `convert_admonitions`' `clippy::match_same_arms` fix
/// (`src/utils.rs`), which dropped the explicit `"note" => "NOTE"` arm as a
/// duplicate of the `_ => "NOTE"` wildcard. A careless variant of that fix
/// could delete the wildcard instead (leaving unrecognized types
/// unconverted) or repoint the fallback at a different admonition type
/// (e.g. `"IMPORTANT"`); either would silently change rendered output.
/// Exercises both the explicit `note` type and an unrecognized `bogus`
/// type through the full `collect_options` pipeline, and pins a surviving
/// explicit arm (`warning`) alongside them.
#[test]
fn test_admonition_unknown_type_falls_back_to_note(
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;
    let content = r#"
{
  options.test.admonitions = lib.mkOption {
    type = lib.types.bool;
    default = false;
    description = ''
      ::: {.note}
      An explicit note.
      :::

      ::: {.bogus}
      An unrecognized admonition type.
      :::

      ::: {.warning}
      A warning.
      :::
    '';
  };
}
"#;
    create_test_file(temp_dir.path(), "flake.nix", content)?;

    let options = collect_options(temp_dir.path(), &[], &HashMap::new(), false, false)?;
    assert_eq!(options.len(), 1);
    let description = options[0]
        .description
        .as_ref()
        .expect("description should be present");

    assert_eq!(
        description.matches("> [!NOTE]").count(),
        2,
        "both the explicit `note` and the unrecognized `bogus` type should \
         render as [!NOTE]: {description:?}"
    );
    assert!(
        !description.contains("[!BOGUS]"),
        "unrecognized admonition type must not be rendered verbatim: {description:?}"
    );
    assert!(
        description.contains("> [!WARNING]"),
        "the surviving explicit `warning` arm must still render as \
         [!WARNING]: {description:?}"
    );

    Ok(())
}

/// Guards nix-options-doc#48: `has_dangerous_scheme` (`src/utils.rs`) used to
/// inspect only the literal bytes of a link target, so an HTML entity
/// reference could smuggle a dangerous scheme past it - `CommonMark` decodes
/// entity references inside a link destination, including the `<...>` form
/// `link_destination` (`src/generate/markdown.rs`) emits, so the decoded
/// form is what actually reaches a renderer. Covers both smuggling shapes
/// from the issue (an encoded scheme letter and an encoded tab/newline
/// splice), the more dangerous variant the issue did not list (an encoded
/// colon, which skips the literal check's `:`-search early-out entirely),
/// named vs. numeric vs. hex entity forms, and a double-encoded payload that
/// only a fixed-point decode (not a single pass) catches. Also asserts a
/// representative sample of legitimate targets - including one containing a
/// bare `&` and one with a `--out-prefix`-style query string - pass through
/// unchanged, so the fix cannot be satisfied by over-blocking anything with
/// an `&` in it.
#[test]
fn test_sanitize_link_target_decodes_entity_references() {
    let dangerous = [
        "&#106;avascript:alert(1)/x.nix",
        "&#x6a;avascript:alert(1)/x.nix",
        "javascript&#58;alert(1)",
        "javascript&colon;alert(1)",
        "java&Tab;script:alert(1)",
        "java&#9;script:alert(1)",
        "&NewLine;javascript:alert(1)",
        "&amp;#106;avascript:alert(1)",
    ];
    for payload in dangerous {
        assert_eq!(
            crate::utils::sanitize_link_target(payload),
            "#",
            "expected {payload:?} to be neutralized"
        );
    }

    let benign = [
        "modules/services/foo.nix",
        "https://github.com/user/repo/blob/main/modules/foo.nix",
        "https://git.example/plain/x.nix?ref=main&plain=1",
        "&#106avascript:alert(1)",
        "modules/a&b/foo.nix",
    ];
    for payload in benign {
        assert_eq!(
            crate::utils::sanitize_link_target(payload),
            payload,
            "expected {payload:?} to pass through unchanged"
        );
    }

    // The pre-existing single-slash authority rule (§4.2) must survive the
    // refactor into `scheme_is_dangerous`.
    assert_eq!(
        crate::utils::sanitize_link_target("http:/evil.example/x.nix"),
        "#"
    );
}

/// Guards specifically against the plausible wrong fix of a single
/// `decode_html_entities` call: `&amp;#106;avascript:` decodes once to
/// `&#106;avascript:` (still inert), and only a second pass reaches the
/// live `javascript:` scheme. A renderer that writes a `CommonMark`-decoded
/// destination into an `href` without re-escaping `&` hands the browser's
/// HTML parser exactly this second decode pass, so `has_dangerous_scheme`
/// (`src/utils.rs`) must decode to a fixed point rather than once.
#[test]
fn test_sanitize_link_target_decodes_to_a_fixed_point() {
    assert_eq!(
        crate::utils::sanitize_link_target("&amp;#106;avascript:alert(1)"),
        "#"
    );
}
