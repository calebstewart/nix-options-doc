//! The utils module provides helper functions used throughout the application.
//!
//! It includes functions for file processing, text manipulation, and
//! variable substitution.

use regex::Regex;
use std::collections::HashMap;
use std::fs;
use std::sync::LazyLock;
use textwrap::dedent;

use std::path::{Path, PathBuf};

use crate::nix_call::{collect_aliases, collect_let_bindings};
use crate::parser;
use crate::OptionDoc;

static VAR_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\$\{([^}]+)\}").unwrap());

/// Produces a stable per-option anchor slug from its full dotted name
/// (e.g. `services.nginx.enable` -> `services-nginx-enable`).
///
/// Used as both the `id` HTML output gives each option and the target
/// of `#`-links into Markdown output (see `generate::markdown`), so a
/// link generated against one output format lands on the same option in
/// the other, and so it stays the same across regenerations rather than
/// depending on a renderer's own heading-slug algorithm.
///
/// The output character set is deliberately restricted to
/// `[A-Za-z0-9_-]`: option names can come from quoted Nix attribute keys
/// (`options."evil\"key".enable`), so without a restriction this value
/// could carry `"`, `<`, `>`, or other bytes straight into an HTML `id`
/// attribute or Markdown anchor. Anything outside that set - including
/// `.`, which is otherwise a legal HTML `id` character - collapses to
/// `-`, same as `:` always has.
pub fn anchor_slug(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '_' | '-' => c,
            _ => '-',
        })
        .collect()
}

/// Reports whether `target` begins with a URI scheme other than a real
/// `http://`/`https://` authority - i.e. whether it is dangerous to use
/// as a link target rather than an ordinary relative/absolute path.
///
/// A URI scheme is `ALPHA *( ALPHA / DIGIT / "+" / "-" / "." )` (RFC 3986
/// §3.1) followed by `:`. Browsers strip ASCII tab, newline, and carriage
/// return from anywhere in a URL before parsing its scheme (WHATWG URL
/// Standard, "basic URL parser"), so a check against the literal prefix
/// alone can be defeated by splicing one of those bytes into the scheme
/// name (e.g. `java\tscript:`) - this strips them first, the same way,
/// before looking for the scheme. A bare `http`/`https` scheme name is
/// not enough to be considered safe; see the `://` check below.
fn has_dangerous_scheme(target: &str) -> bool {
    let cleaned: String = target
        .chars()
        .filter(|c| !matches!(c, '\t' | '\n' | '\r'))
        .collect();
    let trimmed = cleaned.trim_start_matches(|c: char| c.is_ascii_whitespace() || c.is_control());

    let Some(colon) = trimmed.find(':') else {
        return false;
    };
    let scheme = &trimmed[..colon];
    let looks_like_scheme = scheme
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    if !looks_like_scheme {
        return false;
    }

    // Require the scheme to actually be followed by `//` - a real
    // `http://`/`https://` authority, as `--out-prefix` legitimately
    // produces (see README) - not just a bare `http`/`https` scheme
    // name. A directory literally named `http:` joined with the rest of
    // a path produces exactly one `/` after the colon
    // (`http:/evil.example/x.nix`), which browsers still resolve as an
    // authority for "special" schemes like http(s) even with only one
    // slash present (WHATWG URL Standard's leniency for special-scheme
    // authorities) - off-site navigation from what looks like a
    // same-repo source link. Anything short of the literal `://` is
    // therefore treated as dangerous, the same as any other scheme.
    let rest = &trimmed[colon + 1..];
    let is_real_http_url = rest.starts_with("//")
        && (scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https"));

    !is_real_http_url
}

/// Sanitizes a value before it is used as a link `href`/target: ordinary
/// relative/absolute filesystem paths and `http(s)://` URLs pass through
/// unchanged, and anything else - most importantly a `javascript:`,
/// `data:`, `vbscript:`, or other URI scheme - is neutralized to an inert
/// in-page anchor (`#`) instead.
///
/// Declaration file paths (`Declaration::file_path`) are formatted by
/// hand into `href`/link-target values in both the HTML and Markdown
/// generators, rather than going through comrak's Markdown link parser -
/// so they never see comrak's own `javascript:`/`data:`/`vbscript:`
/// filter, which only runs on links written *inside* a description. A
/// `.nix` file living in a directory whose name is itself a URI scheme
/// (every byte involved is legal in a Unix path) would otherwise become a
/// live, clickable link that executes as that scheme (see
/// nix-options-doc#14, nix-options-doc#15). This closes that gap; the
/// existing double-quoted-attribute escaping in the HTML generator (see
/// `anchor_slug` above) is unrelated and still required alongside it -
/// one stops a quote breaking out of the attribute, the other stops the
/// scheme itself from being dangerous.
pub fn sanitize_link_target(target: &str) -> String {
    if has_dangerous_scheme(target) {
        "#".to_string()
    } else {
        target.to_string()
    }
}

/// Replaces dynamic variables in the given text using the provided replacements.
///
/// # Arguments
/// - `text`: The text containing variables in the format ${variable}.
/// - `replacements`: A map of variable names to their replacement values.
///
/// # Returns
/// A string with all variables replaced by their corresponding values.
pub fn apply_replacements(text: &str, replacements: &HashMap<String, String>) -> String {
    if replacements.is_empty() {
        return text.to_string();
    }

    // Use regex replacement rather than iterating through each replacement
    VAR_REGEX
        .replace_all(text, |caps: &regex::Captures| {
            let var_name = &caps[1];
            replacements
                .get(var_name)
                .map_or_else(|| caps[0].to_string(), |value| value.clone())
        })
        .to_string()
}

/// Converts Pandoc-style admonition blocks to GitHub-compatible markdown admonitions.
///
/// # Arguments
/// - `text`: The text potentially containing Pandoc-style admonitions.
///
/// # Returns
/// A string with all admonition blocks converted to GitHub format.
pub fn convert_admonitions(text: &str) -> String {
    // Matches Pandoc-style admonition blocks like ::: {.note} content :::
    static ADMONITION_REGEX: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r":::\s*\{\.([a-z]+)\}([\s\S]*?):::").unwrap());

    // Replace each admonition block with its GitHub compatible version
    let result = ADMONITION_REGEX.replace_all(text, |caps: &regex::Captures| {
        let admonition_type = &caps[1];
        let content = caps[2].trim();

        // Map Pandoc admonition types to GitHub admonition types
        let github_type = match admonition_type {
            "note" => "NOTE",
            "warning" | "caution" => "WARNING",
            "important" => "IMPORTANT",
            "tip" => "TIP",
            _ => "NOTE", // Default fallback
        };

        // Format as GitHub admonition
        format!(
            "> [!{}]  \n> {}",
            github_type,
            content.replace('\n', "\n> ")
        )
    });

    result.to_string()
}

/// Converts a blockquote whose first line starts with a bold admonition
/// keyword (`> **Warning:** ...`) into a GitHub-style admonition.
///
/// This is a common informal convention in hand-written Nix option
/// descriptions (outside nixpkgs' own Pandoc-based documentation
/// tooling) that neither `convert_admonitions` (which looks for
/// `::: {.type}` fences) nor comrak's GFM alerts extension (which looks
/// for `> [!TYPE]`) recognizes on its own, so it would otherwise render
/// as a plain, unstyled blockquote instead of a proper admonition box.
///
/// # Arguments
/// - `text`: The text potentially containing such blockquotes.
///
/// # Returns
/// A string with recognized blockquotes rewritten to `> [!TYPE]` form.
pub fn convert_blockquote_admonitions(text: &str) -> String {
    static PREFIX_REGEX: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?im)^>\s*\*\*(note|warning|important|tip|caution)\.?:?\*\*\s*").unwrap()
    });

    // Leave the text completely untouched when there's nothing to
    // convert, rather than rebuilding it line-by-line regardless: that
    // rebuild loses the distinction between a trailing newline and none
    // (`str::lines` doesn't yield a final empty element for one), which
    // would otherwise shift every description by a stray newline.
    if !PREFIX_REGEX.is_match(text) {
        return text.to_string();
    }

    let mut rebuilt = text
        .lines()
        .map(|line| match PREFIX_REGEX.captures(line) {
            Some(caps) => {
                let kind = caps[1].to_uppercase();
                let rest = &line[caps[0].len()..];
                if rest.trim().is_empty() {
                    format!("> [!{kind}]")
                } else {
                    format!("> [!{kind}]\n> {rest}")
                }
            }
            None => line.to_string(),
        })
        .collect::<Vec<_>>()
        .join("\n");

    if text.ends_with('\n') {
        rebuilt.push('\n');
    }
    rebuilt
}

/// Cleans up Nix-specific formatting directives from description text
/// and converts admonition blocks to GitHub-compatible format.
///
/// # Arguments
/// - `text`: The raw description text to clean.
///
/// # Returns
/// A cleaned string with formatting directives transformed and admonitions converted.
pub fn clean_description(text: &str) -> String {
    // Matches patterns like {var}`content` and replaces with just `content`
    static DIRECTIVE_REGEX: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\{[a-z]+\}(`[^`]+`)").unwrap());

    // Apply both transformations
    let cleaned = DIRECTIVE_REGEX.replace_all(text, "$1").to_string();
    let cleaned = convert_blockquote_admonitions(&cleaned);
    convert_admonitions(&cleaned)
}

/// Extracts the actual content from Nix literalExpression wrappers.
///
/// # Arguments
/// - `value`: The raw value string potentially containing literalExpression wrappers.
///
/// # Returns
/// A string with the literalExpression wrapper removed, exposing just the content.
pub fn clean_literal_expr(value: &str) -> String {
    // Remove common wrappers
    let value = value.trim();

    // Handle lib.literalExpression patterns
    if value.starts_with("lib.literalExpression") || value.starts_with("literalExpression") {
        // Simple approach: extract the content between the string delimiters

        // For indented string literals: ''...''
        if let Some(start_pos) = value.find("''") {
            let start = start_pos + 2; // Skip past the opening ''

            // Find the closing '' - we assume the last '' in the string
            // This works because literalExpression takes a single string argument
            if let Some(end_pos) = value.rfind("''") {
                if end_pos > start {
                    return value[start..end_pos].trim().to_string();
                }
            }
        }
        // For regular quoted strings: "..."
        else if let Some(start_pos) = value.find('"') {
            let start = start_pos + 1; // Skip past the opening "

            // Find the closing " - we assume the last " in the string
            // This is a simplification but works for most common cases
            if let Some(end_pos) = value.rfind('"') {
                if end_pos > start {
                    return value[start..end_pos].trim().to_string();
                }
            }
        }
    }

    // If we couldn't extract inner content, return the original
    value.to_string()
}

/// Custom dedent function that preserves the first line and only dedents subsequent lines.
///
/// # Arguments
/// - `text`: The text to dedent, potentially with inconsistent indentation.
///
/// # Returns
/// A string with consistent indentation where the first line is preserved as-is.
pub fn custom_dedent(text: &str) -> String {
    // Split by first line break
    if let Some(pos) = text.find('\n') {
        let first_line = &text[..pos];
        let rest = &text[pos..];

        // Dedent only the remaining text
        format!("{}{}", first_line, dedent(rest))
    } else {
        // No line breaks, return as is
        text.to_string()
    }
}

/// Determines if a directory entry (file or directory) is hidden, i.e. its
/// own name starts with a dot.
///
/// # Arguments
/// - `entry`: The directory entry to check.
///
/// # Returns
/// True if the directory is hidden (starts with a dot), false otherwise.
fn is_hidden(entry: &walkdir::DirEntry) -> bool {
    entry.file_name().to_string_lossy().starts_with('.')
}

/// Determines whether the directory walker should yield and descend into
/// an entry.
///
/// Hidden entries (name starting with `.`) are rejected so that
/// `WalkDir::filter_entry` prunes the whole subtree rather than merely
/// skipping the directory node itself: without this, a non-hidden `.nix`
/// file inside `.direnv`, `.git`, `.cache`, ... still reaches
/// `should_process_file` and gets documented, and `.git` is walked in full
/// on every run (see nix-options-doc#8).
///
/// The entry at depth 0 — the root the user actually pointed at — is
/// always accepted. `walkdir::DirEntry::file_name` falls back to the whole
/// path when the path has no final component, so the default `--path .`
/// yields the literal file name `"."` for the root, and an explicit
/// `--path ./.config/nixos` or `--path ..` is likewise "hidden" by this
/// test. Rejecting the root would make those invocations silently produce
/// zero options.
///
/// # Arguments
/// - `entry`: The directory entry to test.
///
/// # Returns
/// True if the walker should yield and descend into the entry, false if
/// the entry (and its subtree, if any) should be pruned.
pub fn should_traverse_entry(entry: &walkdir::DirEntry) -> bool {
    entry.depth() == 0 || !is_hidden(entry)
}

/// Determines if a file should be processed based on extension and exclusion criteria.
///
/// # Arguments
/// - `entry`: The directory entry representing the file to check.
/// - `exclude_paths`: A list of paths to exclude from processing.
///
/// # Returns
/// True if the file should be processed, false if it should be skipped.
pub fn should_process_file(entry: &walkdir::DirEntry, exclude_paths: &[PathBuf]) -> bool {
    // Skip excluded paths
    if exclude_paths
        .iter()
        .any(|excl| entry.path().starts_with(excl))
    {
        log::debug!("Skipping excluded path: {}", entry.path().display());
        return false;
    }

    // Skip hidden files, non-files, and non-nix files
    if is_hidden(entry)
        || !entry.file_type().is_file()
        || entry.path().extension().is_none_or(|ext| ext != "nix")
    {
        return false;
    }

    true
}

/// Process a single Nix file to extract option documentation.
///
/// # Arguments
/// - `file_path`: Path to the Nix file to process.
/// - `dir`: The base directory for calculating relative paths.
/// - `replacements`: Variable replacements to apply during parsing.
///
/// # Returns
/// A vector of OptionDoc structs representing the options found in the file.
pub fn process_nix_file(
    file_path: &Path,
    dir: &Path,
    replacements: &HashMap<String, String>,
) -> Vec<OptionDoc> {
    match fs::read_to_string(file_path) {
        Ok(content) => {
            let parse = rnix::Root::parse(&content);
            let relative_path = match file_path.strip_prefix(dir) {
                Ok(rel_path) => rel_path.to_string_lossy().into_owned(),
                Err(e) => {
                    log::warn!(
                        "Error getting relative path for {}: {}",
                        file_path.display(),
                        e
                    );
                    file_path.to_string_lossy().into_owned()
                }
            };

            // Parse the file and get options
            let aliases = collect_aliases(&parse.syntax());
            let let_bindings = collect_let_bindings(&parse.syntax());
            let mut file_options = match parser::visit_node(
                &parse.syntax(),
                &relative_path,
                "",
                replacements,
                &content,
                &aliases,
                &let_bindings,
                None,
                &[],
            ) {
                Ok(file_options) => file_options,
                Err(e) => {
                    log::error!("Error parsing file {}: {}", file_path.display(), e);
                    Vec::new()
                }
            };

            file_options.extend(parser::find_deprecations(
                &parse.syntax(),
                &relative_path,
                &content,
                &aliases,
            ));

            file_options
        }
        Err(e) => {
            log::error!("Error reading file {}: {}", file_path.display(), e);
            Vec::new()
        }
    }
}

/// Parses a string in the format key=value and returns the separate components.
///
/// # Arguments
/// - `s`: A string in the format "key=value".
///
/// # Returns
/// A Result containing a tuple of (key, value) strings or an error if the format is invalid.
pub fn parse_key_value(s: &str) -> Result<(String, String), String> {
    let parts: Vec<&str> = s.splitn(2, '=').collect();
    if parts.len() != 2 || parts[0].is_empty() {
        return Err(format!("Invalid key=value format: {}", s));
    }
    Ok((parts[0].to_string(), parts[1].to_string()))
}
