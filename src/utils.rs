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

/// How many `html_escape::decode_html_entities` passes `has_dangerous_scheme`
/// will run before it gives up and fails closed. Two passes cover the
/// realistic chain (`CommonMark` decodes the link destination, then a renderer
/// that neglects to re-escape `&` in the `href` it writes hands the browser's
/// HTML parser a second decode); the third is slack. Every pass strictly
/// shortens the string, so this loop always terminates on its own - the bound
/// is a cheap ceiling, not a correctness requirement.
const MAX_ENTITY_DECODE_PASSES: usize = 3;

/// Reports whether `target` is dangerous to use as a link target, testing
/// both its literal text and every HTML-entity-decoded form of it.
///
/// `Declaration::file_path` is spliced by hand into a `CommonMark` link
/// destination (`generate::markdown::link_destination`) and an HTML `href`
/// (`generate::html::render`). `CommonMark` decodes entity references inside a
/// link destination - *including* inside the `<...>` form that
/// `link_destination` emits - so a scheme the literal-text check cannot see
/// gets reassembled downstream: `&#106;avascript:` has no legal scheme
/// character at its start, and `javascript&#58;` has no `:` at all, yet both
/// decode into a live `javascript:` URL (see nix-options-doc#48). Decoding
/// therefore has to happen *before* the scheme test, not after. A renderer
/// that writes the decoded destination into an `href` without re-escaping `&`
/// gives the browser's HTML parser a second decode pass, which is why this
/// iterates to a fixed point rather than decoding once.
///
/// `html_escape::decode_html_entities` only decodes references terminated by
/// `;`, matching `CommonMark`. The browser's HTML tokenizer is looser for
/// *numeric* references: per the WHATWG HTML "numeric character reference
/// end state", a missing `;` is a parse error but the code point is still
/// emitted - only the named-reference path requires the terminator. So a
/// renderer that writes an undecoded destination into an `href` without
/// re-escaping `&` hands the browser a decode this crate's decoder can't
/// reproduce, e.g. `&#106avascript:` -> `javascript:`, with no `;` anywhere
/// for `decode_html_entities` to key on. Rather than reimplementing that
/// tokenizer state, this fails closed on the literal shape instead: `&#`
/// never legitimately appears in a real declaration path, and every
/// numeric-entity scheme-smuggling attempt - semicolon-terminated or not -
/// contains it.
///
/// Every *decoded* candidate is checked with `decoded_form_is_dangerous`
/// rather than `scheme_is_dangerous` directly: `scheme_is_dangerous`'s
/// `http(s)://` allow-list is safe against a literal target only because
/// `Path::join` can never produce the `//` an authority needs, and entity
/// references break that guarantee (`&sol;` decodes to `/`), so a decoded
/// `http(s)://` authority - or a decoded protocol-relative `//` prefix, which
/// has no scheme for `scheme_is_dangerous` to inspect at all - has to be
/// checked against what was already present in the *literal* target, not
/// allowed on its own merits (see nix-options-doc#48 review round 2).
fn has_dangerous_scheme(target: &str) -> bool {
    if scheme_is_dangerous(target) {
        return true;
    }
    // Fast path: no `&` means no entity reference is possible, so the literal
    // check above was already the final word. This keeps every ordinary
    // filesystem path off the allocating path below.
    if !target.contains('&') {
        return false;
    }
    if target.contains("&#") {
        return true;
    }

    let mut candidate = target.to_string();
    for _ in 0..MAX_ENTITY_DECODE_PASSES {
        let decoded = html_escape::decode_html_entities(&candidate).into_owned();
        if decoded == candidate {
            // Fixed point reached with nothing dangerous found.
            return false;
        }
        if decoded_form_is_dangerous(target, &decoded) || decoded.contains("&#") {
            return true;
        }
        candidate = decoded;
    }

    // Still decoding to something new after the bound: fail closed. Reaching
    // here needs three-deep nested entity encoding, which no real path has.
    true
}

/// Reports whether `decoded` - the result of one or more entity-decode
/// passes over `literal` - is dangerous to use as a link target. Stricter
/// than `scheme_is_dangerous` in exactly the two ways a decoded (rather than
/// literal) string needs:
///
/// - **Protocol-relative URLs.** `scheme_is_dangerous` looks for a `:` and
///   has nothing to say about a target with no scheme at all. But entity
///   references can decode straight to a leading `//` (`&sol;&sol;` ->
///   `//`), and a `//host/path` target navigates off-site exactly like a
///   full `http://host/path` one does. A literal target can never start
///   with `//` - `Path::join` cannot produce that empty leading component -
///   so rejecting a decoded form that does costs nothing on real input.
/// - **The `http(s)://` allow-list, re-scoped to what decoding actually
///   produced.** `scheme_is_dangerous` trusts any real `http(s)://`
///   authority, which is safe for a *literal* target (again, `Path::join`
///   cannot manufacture `://`) but not for a decoded one - `&colon;&sol;
///   &sol;` decodes to `://` just as readily as `&sol;&sol;` decodes to
///   `//`. So a decoded `http(s)://` authority is only trusted here if the
///   same authority substring was already present, literally, in `literal`;
///   otherwise decoding is what manufactured it. A genuine `--out-prefix`
///   URL with an entity only in its query string (e.g. `&amp;` inside
///   `?a=1&amp;b=2`) keeps its `http(s)://` prefix untouched by decoding, so
///   it still passes.
fn decoded_form_is_dangerous(literal: &str, decoded: &str) -> bool {
    let cleaned: String = decoded
        .chars()
        .filter(|c| !matches!(c, '\t' | '\n' | '\r'))
        .collect();
    let trimmed = cleaned.trim_start_matches(|c: char| c.is_ascii_whitespace() || c.is_control());
    if trimmed.starts_with("//") {
        return true;
    }

    if scheme_is_dangerous(decoded) {
        return true;
    }

    // `scheme_is_dangerous(decoded)` returned false, so `decoded` has either
    // no scheme at all (safe - nothing more to check) or a real `http(s)://`
    // authority (needs the literal-origin check below).
    let decoded_lower = decoded.to_ascii_lowercase();
    let has_http_authority =
        decoded_lower.contains("http://") || decoded_lower.contains("https://");
    if !has_http_authority {
        return false;
    }
    let literal_lower = literal.to_ascii_lowercase();
    let already_present_literally =
        literal_lower.contains("http://") || literal_lower.contains("https://");
    !already_present_literally
}

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
///
/// Lives here rather than in `generate::markdown` because it is shared by the Markdown
/// generator, `parser::find_deprecations` (which writes a rename shim's description) and
/// `filter_options` (which rewrites that same span into a link) - the latter two must produce
/// byte-identical spans for the rewrite to match, which is only guaranteed if both call this
/// one function. Same reasoning as `anchor_slug` above.
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
fn scheme_is_dangerous(target: &str) -> bool {
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
/// scheme itself from being dangerous. The scheme test also runs against
/// every HTML-entity-decoded form of `target`, not just its literal text,
/// so an encoded scheme or colon cannot smuggle a dangerous URI through
/// (see nix-options-doc#48).
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
        .replace_all(text, |caps: &regex::Captures<'_>| {
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
    let result = ADMONITION_REGEX.replace_all(text, |caps: &regex::Captures<'_>| {
        let admonition_type = &caps[1];
        let content = caps[2].trim();

        // Map Pandoc admonition types to GitHub admonition types
        let github_type = match admonition_type {
            "warning" | "caution" => "WARNING",
            "important" => "IMPORTANT",
            "tip" => "TIP",
            // "note", plus any unrecognized admonition type
            _ => "NOTE",
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
/// The check is name-based on purpose, and only ever sees the entry's own
/// name: a `DirEntry` for a symlink reports the *link's* name, never the
/// target's. So under `--follow-symlinks` a non-hidden link into a hidden
/// directory is traversed, and the same goes for a non-hidden link to a
/// single `.nix` file inside one in `should_process_file`. That is the
/// specified behavior of the flag (nix-options-doc#42) - passing it is
/// consent to leave the visible tree, and pruning by *resolved* path would
/// silently drop options for trees that symlink out to a hidden source
/// directory. Do not "fix" this without changing the documented contract in
/// the `--follow-symlinks` help text and the README first.
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
/// An unreadable or unparseable file degrades to zero options rather than
/// failing the run. The one exception is outside this crate's control: a
/// file holding an extremely long operator or application chain
/// (`a // b // c ...`, `1 + 1 + 1 ...`, `f x x x ...`) overflows the stack
/// inside the `rnix::Root::parse` call below, or when the tree it returns
/// is dropped, and aborts the process. See nix-options-doc#67 and the
/// README's "Known Limitation: Very Deep Expressions".
///
/// # Arguments
/// - `file_path`: Path to the Nix file to process.
/// - `dir`: The base directory for calculating relative paths.
/// - `replacements`: Variable replacements to apply during parsing.
///
/// # Returns
/// A vector of `OptionDoc` structs representing the options found in the file.
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
            let line_index = parser::LineIndex::new(&content);
            let aliases = collect_aliases(&parse.syntax());
            let let_bindings = collect_let_bindings(&parse.syntax());
            let mut file_options = match parser::visit_node(
                0,
                &parse.syntax(),
                &relative_path,
                "",
                replacements,
                &line_index,
                &aliases,
                &let_bindings,
                None,
                &[],
                &mut parser::ExpansionBudget::new(),
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
                &line_index,
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
