use super::*;

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

/// Regression test for #14: raw HTML and dangerous URL schemes embedded in
/// an option's `description` must not survive into the generated HTML
/// document verbatim - the HTML generator must render descriptions with
/// comrak's raw-HTML and dangerous-URL protections switched on (i.e. not
/// `render.unsafe = true`), while still rendering legitimate Markdown
/// (emphasis, code spans, tables, strikethrough, safe links, admonitions).
#[test]
fn test_html_escapes_raw_html_in_description(
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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

/// Extracts the JS array literal from `const <name> = …;` in the
/// generated page. A literal newline can never occur inside the JSON
/// (`serde_json` escapes them), so the first `;\n` after the declaration
/// is its terminator.
fn extract_js_array_literal<'a>(html: &'a str, decl: &str) -> &'a str {
    let start = html.find(decl).expect("declaration should be present") + decl.len();
    let end = start
        + html[start..]
            .find(";\n")
            .expect("declaration should be terminated");
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
