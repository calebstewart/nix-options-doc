use crate::utils::{anchor_slug, inline_code, longest_backtick_run, sanitize_link_target};
use crate::OptionDoc;
use std::fmt::Write;

/// Renders `content` as a fenced ```` ```nix ```` code block that survives arbitrary input.
///
/// # Arguments
/// - `content`: Third-party-controlled text to render as a fenced block.
///
/// # Returns
/// A fenced block whose fence is at least three backticks long, and longer still when
/// `content` itself contains a run of three or more backticks (which would otherwise
/// close the fence early).
pub(crate) fn nix_code_block(content: &str) -> String {
    let fence = "`".repeat(longest_backtick_run(content).max(2) + 1);
    format!("{fence}nix\n{content}\n{fence}")
}

/// Wraps a sanitized link target in a `CommonMark` angle-bracket link destination.
///
/// # Arguments
/// - `target`: The raw declaration file path to link to.
///
/// # Returns
/// A `<...>`-delimited link destination. `sanitize_link_target` is applied first and is
/// unchanged by this helper - it neutralizes dangerous URI schemes (see #14/#15); this
/// helper is a separate *syntax* layer on top of it, needed because a bare (non-angle-
/// bracket) destination cannot contain spaces or unbalanced parentheses, both of which are
/// plausible in a filesystem path. Escaping `\` is load-bearing on Windows, where
/// `Declaration::file_path` uses `\` separators; without it, a literal backslash would be
/// silently interpreted as an escape by the renderer.
pub(crate) fn link_destination(target: &str) -> String {
    let sanitized = sanitize_link_target(target);
    let mut out = String::with_capacity(sanitized.len() + 2);
    out.push('<');
    for c in sanitized.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '<' => out.push_str("\\<"),
            '>' => out.push_str("\\>"),
            // An angle-bracket destination may not contain a line ending
            // at all, so these cannot be backslash-escaped away.
            '\n' => out.push_str("%0A"),
            '\r' => out.push_str("%0D"),
            _ => out.push(c),
        }
    }
    out.push('>');
    out
}

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
            "## [{}]({})",
            inline_code(&option.name),
            link_destination(&format!("{}#L{}", primary.file_path, primary.line_number))
        )?;

        // Description with preserved formatting
        if let Some(description) = &option.description {
            // Since the description might already contain markdown, we include it directly
            writeln!(output, "\n{}", description)?;
        }

        // Type information - escaped
        if option.nix_type.contains('\n') || option.nix_type.len() > 72 {
            // Multi-line or long type - use code block
            writeln!(
                output,
                "\n**Type:**\n\n{}",
                nix_code_block(&option.nix_type)
            )?;
        } else {
            // Single line type - use inline code
            writeln!(output, "\n**Type:** {}", inline_code(&option.nix_type))?;
        }

        // Default value if available - in code block to preserve formatting
        if let Some(default) = &option.default_value {
            if default.contains('\n') || default.len() > 72 {
                // Multi-line or long default - use code block
                writeln!(output, "\n**Default:**\n\n{}", nix_code_block(default))?;
            } else {
                // Single line default - use inline code
                writeln!(output, "\n**Default:** {}", inline_code(default))?;
            }
        }

        if let Some(example) = &option.example {
            if example.contains('\n') || example.len() > 72 {
                writeln!(output, "\n**Example:**\n\n{}", nix_code_block(example))?;
            } else {
                writeln!(output, "\n**Example:** {}", inline_code(example))?;
            }
        }

        if let Some(condition) = &primary.condition {
            writeln!(
                output,
                "\n**Condition:** only declared when {}",
                inline_code(condition)
            )?;
        }

        if !other_declarations.is_empty() {
            writeln!(output, "\n**Also declared in:**")?;
            for decl in other_declarations {
                writeln!(
                    output,
                    "- [{}]({})",
                    inline_code(&decl.file_path),
                    link_destination(&format!("{}#L{}", decl.file_path, decl.line_number))
                )?;
                if let Some(alt_description) = &decl.description {
                    writeln!(output, "  > {}", alt_description.replace('\n', "\n  > "))?;
                }
                if let Some(condition) = &decl.condition {
                    writeln!(output, "  > Only declared when {}", inline_code(condition))?;
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
