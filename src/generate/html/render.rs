//! Per-option markup: the type-category legend, and rendering a single
//! `OptionDoc` into its `<article class="option">` block plus its entry
//! in the client-side search index.

use crate::OptionDoc;
use comrak::{markdown_to_html, Options as ComrakOptions};

/// Canonical legend order; only categories actually present in a given
/// document get a chip (see [`super::generate_html`]).
pub(super) const CATEGORIES: [(&str, &str); 9] = [
    ("bool", "boolean"),
    ("choice", "choice"),
    ("string", "string"),
    ("number", "number"),
    ("package", "package"),
    ("list", "list"),
    ("set", "set"),
    ("submodule", "submodule"),
    ("any", "other"),
];

/// Categorizes a formatted `nix_type` string (see
/// [`crate::types::format_type`]) into a small, closed set of display
/// categories, used both for the per-option type badge and the
/// click-to-filter legend. This is a presentation-only heuristic over
/// the already human-formatted string, not a structural type analysis.
fn classify_type(nix_type: &str) -> (&'static str, &'static str) {
    let t = nix_type.to_lowercase();
    if t.contains("list of") {
        ("list", "list")
    } else if t.contains("attribute set") {
        ("set", "set")
    } else if t.contains("submodule") {
        ("submodule", "submodule")
    } else if t.contains("one of") {
        ("choice", "choice")
    } else if t.contains("boolean") {
        ("bool", "boolean")
    } else if t.contains("package") {
        ("package", "package")
    } else if ["integer", "number", "port", "floating point"]
        .iter()
        .any(|s| t.contains(s))
    {
        ("number", "number")
    } else if ["string", "path", "raw value"]
        .iter()
        .any(|s| t.contains(s))
    {
        ("string", "string")
    } else {
        ("any", "other")
    }
}

/// Splits a dotted option name into its shared prefix and final (leaf)
/// segment, e.g. `services.nginx.enable` -> (`services.nginx.`, `enable`),
/// so the leaf can be visually emphasized.
fn split_leaf(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(idx) => (&name[..=idx], &name[idx + 1..]),
        None => ("", name),
    }
}

/// Renders a labeled key/value metadata row. Multi-line or long values
/// get their own full-width block with a code panel; short values stay
/// inline next to the label.
fn format_meta_row(label: &str, content: &str) -> String {
    let escaped = html_escape::encode_text(content);
    if content.contains('\n') || content.len() > 60 {
        format!(
            r#"            <div class="meta-row block">
                <span class="meta-label">{label}</span>
                <pre><code>{escaped}</code></pre>
            </div>
"#
        )
    } else {
        format!(
            r#"            <div class="meta-row">
                <span class="meta-label">{label}</span>
                <code>{escaped}</code>
            </div>
"#
        )
    }
}

/// The text an option contributes to the client-side search index: name,
/// description, type, default, and example, space-joined.
pub(super) fn search_index_entry(option: &OptionDoc) -> String {
    [
        Some(option.name.as_str()),
        option.description.as_deref(),
        Some(option.nix_type.as_str()),
        option.default_value.as_deref(),
        option.example.as_deref(),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" ")
}

/// Renders one option as an `<article class="option">` block, along with
/// the type category it was classified into (for the legend's presence
/// set).
pub(super) fn render_option(
    option: &OptionDoc,
    comrak_options: &ComrakOptions,
) -> (String, (&'static str, &'static str)) {
    let slug = option.name.replace(['.', ':'], "-");
    let (category_class, category_label) = classify_type(&option.nix_type);
    let (prefix, leaf) = split_leaf(&option.name);

    let (primary, other_declarations) = option
        .declarations
        .split_first()
        .expect("an option always has at least one declaration");

    let mut article = format!(
        r#"    <article class="option" id="{slug}" data-category="{category_class}">
        <div class="option-head">
            <h2 class="option-path">
                <a href="{href}#L{line}"><span class="path-prefix">{prefix}</span><span class="path-leaf">{leaf}</span></a>
            </h2>
            <span class="type-badge t-{category_class}">{category_label}</span>
        </div>
"#,
        slug = html_escape::encode_text(&slug),
        category_class = category_class,
        href = html_escape::encode_text(&primary.file_path),
        line = primary.line_number,
        prefix = html_escape::encode_text(prefix),
        leaf = html_escape::encode_text(leaf),
        category_label = category_label,
    );

    if let Some(description) = &option.description {
        let html_description = markdown_to_html(description, comrak_options);
        article.push_str(&format!(
            r#"        <div class="option-desc">{html_description}</div>
"#
        ));
    }

    let mut meta_rows = String::new();
    meta_rows.push_str(&format_meta_row("Type", &option.nix_type));
    if let Some(default) = &option.default_value {
        meta_rows.push_str(&format_meta_row("Default", default));
    }
    if let Some(example) = &option.example {
        meta_rows.push_str(&format_meta_row("Example", example));
    }
    article.push_str(&format!(
        "        <div class=\"option-meta\">\n{meta_rows}        </div>\n"
    ));

    article.push_str(&format!(
        r#"        <div class="option-decl"><a href="{0}#L{1}">{0}:{1}</a></div>
"#,
        html_escape::encode_text(&primary.file_path),
        primary.line_number
    ));

    if !other_declarations.is_empty() {
        article.push_str("        <ul class=\"also-declared\">\n");
        for decl in other_declarations {
            article.push_str(&format!(
                r#"            <li><a href="{0}#L{1}">{0}:{1}</a>"#,
                html_escape::encode_text(&decl.file_path),
                decl.line_number
            ));
            if let Some(alt) = &decl.description {
                article.push_str(&format!(
                    r#"<div class="alt-desc">{}</div>"#,
                    markdown_to_html(alt, comrak_options)
                ));
            }
            article.push_str("</li>\n");
        }
        article.push_str("        </ul>\n");
    }

    article.push_str("    </article>\n\n");

    (article, (category_class, category_label))
}
