use crate::utils::{anchor_slug, sanitize_link_target};
use crate::OptionDoc;
use std::fmt::Write;

/// Length of the longest run of consecutive backticks in `content`.
///
/// # Arguments
/// - `content`: The text to scan.
///
/// # Returns
/// The number of backticks in the longest consecutive run, or `0` if `content` has none.
/// Used to size code-span/code-block fences long enough that they cannot be closed early
/// by a shorter run of backticks already present in the content.
pub(crate) fn longest_backtick_run(content: &str) -> usize {
    let mut longest = 0usize;
    let mut current = 0usize;
    for c in content.chars() {
        if c == '`' {
            current += 1;
            longest = longest.max(current);
        } else {
            current = 0;
        }
    }
    longest
}

/// Renders `content` as a `CommonMark` inline code span that survives arbitrary input.
///
/// # Arguments
/// - `content`: Third-party-controlled text (a Nix type, default, example, or condition)
///   to render as an inline code span.
///
/// # Returns
/// A backtick-delimited (or longer-delimited) span. Per `CommonMark` §6, backslash escapes do
/// not work inside code spans, so the only correct way to embed a backtick is a delimiter
/// run one backtick longer than the longest run already present in the content, padded
/// with a single space on each side when needed to avoid the content's own backticks (or
/// an all-whitespace body) fusing with the delimiter.
pub(crate) fn inline_code(content: &str) -> String {
    // A code span cannot contain a line break at all; the only caller that can hit this
    // is the condition field, which carries raw Nix source (possibly a multi-line `mkIf`
    // predicate) straight from `parser::format_condition`. Guard behind the `contains`
    // check so every other value passes through byte for byte and existing output is
    // unchanged.
    let normalized;
    let body: &str = if content.contains(['\n', '\r']) {
        normalized = content.split_whitespace().collect::<Vec<_>>().join(" ");
        &normalized
    } else {
        content
    };

    // An empty code span isn't expressible in CommonMark - `` renders as literal
    // backticks, not an empty <code>. Substitute a single space instead.
    if body.is_empty() {
        return "` `".to_string();
    }

    let fence = "`".repeat(longest_backtick_run(body) + 1);

    // Padding stops the content's own leading/trailing backtick from fusing with the
    // delimiter run, and preserves a leading+trailing space that CommonMark's
    // "strip one space from each end" rule would otherwise silently eat. That stripping
    // rule doesn't apply to all-space content, so an all-space body must NOT be padded
    // (padding it would add two spurious spaces).
    let needs_padding = body.starts_with('`')
        || body.ends_with('`')
        || (body.starts_with(' ') && body.ends_with(' ') && body.chars().any(|c| c != ' '));

    if needs_padding {
        format!("{fence} {body} {fence}")
    } else {
        format!("{fence}{body}{fence}")
    }
}

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
