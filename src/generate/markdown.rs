use crate::utils::{anchor_slug, sanitize_link_target};
use crate::OptionDoc;
use std::fmt::Write;

/// Generates a Markdown formatted string documenting NixOS module options.
///
/// # Arguments
/// - `options`: A slice of option documentation entries to be formatted as markdown.
///
/// # Returns
/// A `Result` containing the formatted Markdown string with headers, descriptions, and code blocks or an error.
pub fn generate_markdown(
    options: &[OptionDoc],
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let mut output = String::with_capacity(options.len() * 500 + 200);
    output.push_str("# NixOS Module Options\n\n");

    for option in options {
        // The heading always links to the first-found declaration; any
        // others are listed separately further down.
        let (primary, other_declarations) = option
            .declarations
            .split_first()
            .expect("an option always has at least one declaration");

        // An explicit anchor ahead of the heading, rather than relying
        // on whatever heading-slug algorithm the renderer displaying
        // this file happens to use - which varies by tool and isn't
        // guaranteed to match the `id` HTML output gives the same
        // option, or to stay the same across regenerations.
        writeln!(output, "\n<a id=\"{}\"></a>", anchor_slug(&option.name))?;
        writeln!(
            output,
            "## [`{}`]({}#L{})",
            option.name,
            sanitize_link_target(&primary.file_path),
            primary.line_number
        )?;

        // Description with preserved formatting
        if let Some(description) = &option.description {
            // Since the description might already contain markdown, we include it directly
            writeln!(output, "\n{}", description)?;
        }

        // Type information - escaped
        if option.nix_type.contains('\n') || option.nix_type.len() > 72 {
            // Multi-line or long type - use code block
            writeln!(output, "\n**Type:**\n\n```nix\n{}\n```", option.nix_type)?;
        } else {
            // Single line type - use inline code
            writeln!(
                output,
                "\n**Type:** `{}`",
                option.nix_type.replace('`', "\\`")
            )?;
        }

        // Default value if available - in code block to preserve formatting
        if let Some(default) = &option.default_value {
            if default.contains('\n') || default.len() > 72 {
                // Multi-line or long default - use code block
                writeln!(output, "\n**Default:**\n\n```nix\n{}\n```", default)?;
            } else {
                // Single line default - use inline code
                writeln!(output, "\n**Default:** `{}`", default)?;
            }
        }

        if let Some(example) = &option.example {
            if example.contains('\n') || example.len() > 72 {
                writeln!(output, "\n**Example:**\n\n```nix\n{}\n```", example)?;
            } else {
                writeln!(output, "\n**Example:** `{}`", example)?;
            }
        }

        if let Some(condition) = &primary.condition {
            writeln!(
                output,
                "\n**Condition:** only declared when `{}`",
                condition
            )?;
        }

        if !other_declarations.is_empty() {
            writeln!(output, "\n**Also declared in:**")?;
            for decl in other_declarations {
                writeln!(
                    output,
                    "- [`{}`]({}#L{})",
                    decl.file_path,
                    sanitize_link_target(&decl.file_path),
                    decl.line_number
                )?;
                if let Some(alt_description) = &decl.description {
                    writeln!(output, "  > {}", alt_description.replace('\n', "\n  > "))?;
                }
                if let Some(condition) = &decl.condition {
                    writeln!(output, "  > Only declared when `{}`", condition)?;
                }
            }
        }
    }

    writeln!(
        output,
        "\n---\n*Generated with [{}]({})*",
        env!("CARGO_PKG_NAME"),
        option_env!("CARGO_PKG_REPOSITORY").unwrap_or(env!("CARGO_PKG_NAME"))
    )?;

    Ok(output)
}
