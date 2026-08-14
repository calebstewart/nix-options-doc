//! The parser module contains functions for parsing Nix syntax trees
//! and extracting option documentation.
//!
//! It traverses the abstract syntax tree of Nix files to identify
//! module options and their metadata.

use crate::nix_call::{find_attr, resolve_call};
use crate::types::{self, unwrap_paren};
use crate::utils::{apply_replacements, clean_description, clean_literal_expr, custom_dedent};
use crate::{Declaration, OptionDoc};
use rnix::ast;
use rnix::{SyntaxKind, SyntaxNode};
use rowan::ast::AstNode;
use std::collections::HashMap;

/// Per-file accounting for how much submodule expansion parsing has done
/// so far - both the options it has emitted and the body text it has
/// re-walked - used to stop expansion before it fans out combinatorially
/// (nix-options-doc#21) or spends unbounded time re-walking one padded body
/// (nix-options-doc#47).
///
/// This is a single mutable value threaded through the whole traversal of
/// one file, *deliberately* unlike `submodule_stack` (which is copied per
/// branch because it answers a per-path question). The hazard here is the
/// cumulative total across every branch, so sibling branches must share
/// one counter.
///
/// One counter is created per file in [`crate::utils::process_nix_file`],
/// so the cap applies per file. It is never shared across files: rayon
/// parallelises over files (see `crate::collect_options`), and a shared
/// counter would make which file gets truncated depend on thread
/// scheduling, destroying the run-to-run determinism `nix_files.sort()`
/// exists to guarantee.
#[derive(Default)]
pub struct ExpansionBudget {
    emitted: usize,
    expanded_bytes: usize,
    warned: bool,
}

impl ExpansionBudget {
    /// Creates a fresh budget for one file.
    pub fn new() -> Self {
        Self::default()
    }

    /// Records that one more `OptionDoc` has been emitted for this file.
    fn record_option(&mut self) {
        self.emitted += 1;
    }

    /// Records that a submodule body of `len` source bytes is about to be
    /// re-walked, charging that re-walk to this file's work budget.
    ///
    /// The body's source length is used as an O(1) stand-in for the number
    /// of nodes the traversal is about to visit - counting them would cost
    /// exactly the walk being measured.
    fn record_expansion(&mut self, len: usize) {
        self.expanded_bytes = self.expanded_bytes.saturating_add(len);
    }

    /// Reports whether this file has spent enough of either budget that
    /// further submodule expansion should stop.
    fn is_exhausted(&self) -> bool {
        self.options_exhausted() || self.work_exhausted()
    }

    /// Whether the emitted-option cap has been reached.
    fn options_exhausted(&self) -> bool {
        self.emitted >= types::MAX_SUBMODULE_EXPANSION_OPTIONS
    }

    /// Whether the re-walked-bytes cap has been reached.
    fn work_exhausted(&self) -> bool {
        self.expanded_bytes >= types::MAX_SUBMODULE_EXPANSION_BYTES
    }

    /// Warns that expansion was truncated, at most once per file - the
    /// check fires on every subsequent expansion attempt, which on a
    /// pathological file is thousands of times.
    fn warn_once(&mut self, file_path: &str) {
        if self.warned {
            return;
        }
        self.warned = true;
        // Name the budget that actually tripped: the two have very
        // different causes (too many options vs. one oversized body
        // re-walked too often), and a reader chasing truncated output
        // needs to know which.
        if self.options_exhausted() {
            log::warn!(
                "{file_path}: stopped expanding submodule options after {} entries; \
                 output for this file is truncated",
                types::MAX_SUBMODULE_EXPANSION_OPTIONS
            );
        } else {
            log::warn!(
                "{file_path}: stopped expanding submodules after re-walking {} bytes of \
                 submodule bodies; output for this file is truncated",
                types::MAX_SUBMODULE_EXPANSION_BYTES
            );
        }
    }
}

/// Recursively traverses the syntax tree of a Nix file to extract option definitions.
///
/// # Arguments
/// - `node`: The current syntax node being processed.
/// - `file_path`: The relative file path of the Nix file for documentation reference.
/// - `prefix`: The current option name prefix in the hierarchy.
/// - `replacements`: A map of variable replacements for dynamic segments.
/// - `source_text`: The full text of the source file for line number calculation.
/// - `aliases`: Local function aliases (see [`crate::nix_call::collect_aliases`]).
/// - `let_bindings`: Local `let`-bound expressions (see
///   [`crate::nix_call::collect_let_bindings`]).
/// - `condition`: The `mkIf` condition(s) currently in scope, if any (see
///   [`as_mkif`]), joined with `&&` when nested.
/// - `submodule_stack`: The text ranges of submodule bodies currently
///   being expanded on this path, innermost last (see [`parse_attrset`]).
///   Forwarded unchanged; only the `mkOption` submodule-expansion arm in
///   `parse_attrset` grows it.
/// - `budget`: Per-file cap on emitted options, shared by every branch of
///   this file's traversal (unlike `submodule_stack`, which is per-path).
///   Submodule expansion stops once it is exhausted; see
///   [`ExpansionBudget`].
///
/// # Returns
/// A vector of `OptionDoc` structs representing the found options or an error.
#[allow(clippy::too_many_arguments)]
pub fn visit_node(
    node: &SyntaxNode,
    file_path: &str,
    prefix: &str,
    replacements: &HashMap<String, String>,
    source_text: &str,
    aliases: &HashMap<String, String>,
    let_bindings: &HashMap<String, SyntaxNode>,
    condition: Option<&str>,
    submodule_stack: &[rnix::TextRange],
    budget: &mut ExpansionBudget,
) -> Result<Vec<OptionDoc>, Box<dyn std::error::Error + Send + Sync>> {
    let mut options = Vec::new();

    if node.kind() == SyntaxKind::NODE_ATTRPATH_VALUE {
        let key = node
            .children()
            .find(|n| n.kind() == SyntaxKind::NODE_ATTRPATH)
            .as_ref()
            .map(|n| parse_attrpath(n, replacements));

        if let Some(value_node) = node.children().nth(1) {
            if let Some(key) = key {
                let new_prefix = if prefix.is_empty() {
                    key
                } else {
                    format!("{}.{}", prefix, key)
                };
                let mut nested_options = parse_attrset(
                    &value_node,
                    file_path,
                    &new_prefix,
                    replacements,
                    source_text,
                    aliases,
                    let_bindings,
                    condition,
                    submodule_stack,
                    budget,
                )?;
                options.append(&mut nested_options);
            }
        }
    } else if let Some((cond_node, value_node)) = as_mkif(node, aliases) {
        // `mkIf cond value` - recurse into the guarded value only, with
        // `cond` folded into the condition tracked for any option found
        // underneath it (rather than blindly visiting all children,
        // which would needlessly walk into the condition expression
        // itself too).
        let new_condition = combine_condition(condition, &format_condition(&cond_node));
        let mut child_options = visit_node(
            &value_node,
            file_path,
            prefix,
            replacements,
            source_text,
            aliases,
            let_bindings,
            Some(&new_condition),
            submodule_stack,
            budget,
        )?;
        options.append(&mut child_options);
    } else {
        // Visit all children for other node types
        for child in node.children() {
            let mut child_options = visit_node(
                &child,
                file_path,
                prefix,
                replacements,
                source_text,
                aliases,
                let_bindings,
                condition,
                submodule_stack,
                budget,
            )?;
            options.append(&mut child_options);
        }
    }

    Ok(options)
}

/// If `node` is a resolved `mkIf cond value` call, returns its condition
/// and guarded value nodes.
fn as_mkif(
    node: &SyntaxNode,
    aliases: &HashMap<String, String>,
) -> Option<(SyntaxNode, SyntaxNode)> {
    let (name, args) = resolve_call(node, aliases)?;
    if name != "mkIf" {
        return None;
    }
    Some((args.first()?.clone(), args.get(1)?.clone()))
}

/// Formats an `mkIf` condition expression's source text for display.
fn format_condition(node: &SyntaxNode) -> String {
    node.text().to_string().trim().to_string()
}

/// Folds a newly-encountered `mkIf` condition into whatever condition (if
/// any) is already in scope from an enclosing `mkIf`.
fn combine_condition(existing: Option<&str>, new: &str) -> String {
    match existing {
        Some(existing) => format!("{existing} && {new}"),
        None => new.to_string(),
    }
}

/// Parses an attribute path node and returns a dot-separated string representing the option name.
fn parse_attrpath(node: &SyntaxNode, replacements: &HashMap<String, String>) -> String {
    node.children()
        .map(|child| apply_replacements(&child.text().to_string(), replacements))
        .collect::<Vec<_>>()
        .join(".")
}

/// Determines the 1-based line number where a syntax node starts in the source file.
fn get_line_number(node: &SyntaxNode, source_text: &str) -> usize {
    let text_range = node.text_range();
    let start_offset: usize = text_range.start().into();

    let line_count = source_text[..start_offset]
        .chars()
        .filter(|&c| c == '\n')
        .count();

    line_count + 1
}

/// Clean and format a description string for documentation.
fn process_description(description: &str, replacements: &HashMap<String, String>) -> String {
    let replaced = apply_replacements(description, replacements);
    clean_description(&replaced)
}

/// Extracts the text of a string literal node with Nix's own string
/// semantics applied: exactly one delimiter pair removed, escape
/// sequences interpreted, and (for `''` strings) the common indentation
/// stripped the way Nix itself strips it.
///
/// Interpolations are re-emitted as their `${...}` source text rather
/// than dropped, because they are statically unresolvable here and
/// `utils::apply_replacements` (the `--replace` flag) still has to be
/// able to see and substitute them downstream. This doesn't change
/// *what* `--replace` can substitute - the raw source already contained
/// `${name}` and `VAR_REGEX` (`utils.rs`) already matched it there too.
/// The only difference is that a `''`-string's own `''$` escape (a
/// literal `$` that is *not* the start of an interpolation) is now
/// correctly unescaped instead of leaking its stray `''` into the
/// output.
///
/// Falls back to the raw (dedented, trimmed) source text for anything
/// that is not a well-formed string node - both because a description
/// can legitimately be some other expression (`lib.mdDoc "..."`), and
/// because rnix's `normalized_parts` *asserts* on the error-recovery
/// nodes a malformed file produces, which would turn this crate's
/// deliberate graceful degradation into a process-wide panic.
fn string_text(node: &SyntaxNode) -> String {
    let is_well_formed = node.kind() == SyntaxKind::NODE_STRING
        && node.children_with_tokens().all(|child| {
            matches!(
                child.kind(),
                SyntaxKind::TOKEN_STRING_START
                    | SyntaxKind::TOKEN_STRING_END
                    | SyntaxKind::TOKEN_STRING_CONTENT
                    | SyntaxKind::NODE_INTERPOL
            )
        });
    let string = match ast::Str::cast(node.clone()).filter(|_| is_well_formed) {
        Some(string) => string,
        None => return custom_dedent(node.text().to_string().trim()),
    };
    string
        .normalized_parts()
        .into_iter()
        .map(|part| match part {
            ast::InterpolPart::Literal(text) => text,
            ast::InterpolPart::Interpolation(interpol) => interpol.syntax().text().to_string(),
        })
        .collect()
}

/// Extracts the unquoted text of each string item in a `NODE_LIST` node.
fn list_of_strings(node: &SyntaxNode) -> Option<Vec<String>> {
    let list = ast::List::cast(node.clone())?;
    Some(
        list.items()
            .map(|item| string_text(item.syntax()))
            .collect(),
    )
}

/// Unwraps `mkDefault`/`mkForce`/`mkOverride` and `mkIf` wrappers around a
/// value expression, returning the inner value node they guard.
///
/// This is purely syntactic, not evaluation: `mkIf cond value` is always
/// unwrapped to `value` since we have no way to resolve `cond` without
/// evaluating the whole module set, and showing the guarded value is more
/// useful to a reader than showing the wrapper call verbatim.
fn unwrap_value_wrapper(node: &SyntaxNode, aliases: &HashMap<String, String>) -> SyntaxNode {
    if let Some((name, args)) = resolve_call(node, aliases) {
        match name.as_str() {
            "mkDefault" | "mkForce" | "mkOverride" => {
                if let Some(value) = args.last() {
                    return value.clone();
                }
            }
            "mkIf" => {
                if let Some(value) = args.get(1) {
                    return value.clone();
                }
            }
            _ => {}
        }
    }
    node.clone()
}

/// Cleans and dedents a value expression node's source text, unwrapping
/// known override/conditional wrappers first.
fn render_value(node: &SyntaxNode, aliases: &HashMap<String, String>) -> String {
    let effective = unwrap_value_wrapper(node, aliases);
    custom_dedent(&clean_literal_expr(&effective.text().to_string()))
}

/// The subset of an option's fields that an `{ default = ...; }`-style
/// override attrset (as in `<expr> // { default = ...; }`) can set.
#[derive(Default)]
struct OptionOverrides {
    nix_type: Option<String>,
    description: Option<String>,
    default_value: Option<String>,
    example: Option<String>,
}

/// Scans an attrset for `type`/`description`/`default`/`example` entries,
/// the same fields `mkOption { ... }` itself accepts, for use as an
/// override attrset in `<option-constructor> // { ... }`.
fn scan_option_overrides(
    attr_set: &SyntaxNode,
    aliases: &HashMap<String, String>,
    replacements: &HashMap<String, String>,
) -> OptionOverrides {
    let mut overrides = OptionOverrides::default();

    for attr in attr_set.children() {
        if attr.kind() != SyntaxKind::NODE_ATTRPATH_VALUE {
            continue;
        }
        let attr_key = attr
            .children()
            .find(|n| n.kind() == SyntaxKind::NODE_ATTRPATH)
            .and_then(|n| n.children().next())
            .map(|n| n.text().to_string());
        let attr_value = attr.children().nth(1);

        match (attr_key.as_deref(), attr_value) {
            (Some("type"), Some(v)) => overrides.nix_type = Some(types::format_type(&v, aliases)),
            (Some("description"), Some(v)) => {
                overrides.description = Some(process_description(&string_text(&v), replacements));
            }
            (Some("default"), Some(v)) => overrides.default_value = Some(render_value(&v, aliases)),
            (Some("example"), Some(v)) => overrides.example = Some(render_value(&v, aliases)),
            _ => {}
        }
    }

    overrides
}

/// Parses an attribute set node to extract NixOS module option definitions.
///
/// # Arguments
/// - `node`: The syntax node representing the attribute set.
/// - `file_path`: The file path of the Nix file for reference.
/// - `current_prefix`: The current option name hierarchy as a dot-separated string.
/// - `replacements`: A map of variable replacements for dynamic values.
/// - `source_text`: The source text of the file for line number calculation.
/// - `aliases`: Local function aliases (see [`crate::nix_call::collect_aliases`]).
/// - `let_bindings`: Local `let`-bound expressions (see
///   [`crate::nix_call::collect_let_bindings`]).
/// - `condition`: The `mkIf` condition(s) currently in scope, if any (see
///   [`visit_node`]).
/// - `submodule_stack`: Text ranges of submodule bodies currently being
///   expanded on this path, innermost last. Forwarded unchanged by every
///   recursive call here *except* the `mkOption` submodule-expansion arm,
///   which pushes the resolved body's range before recursing into it -
///   and refuses to expand a body whose range is already on the stack
///   (a recursive/tree-shaped submodule type) or once the stack reaches
///   [`types::MAX_SUBMODULE_DEPTH`], so self-referential `let`-bound
///   submodule types (nix-options-doc#6) terminate instead of recursing
///   forever.
/// - `budget`: Per-file cap on emitted options, shared by every branch of
///   this file's traversal (unlike `submodule_stack`, which is per-path).
///   Submodule expansion stops once it is exhausted; see
///   [`ExpansionBudget`].
///
/// # Returns
/// A vector of `OptionDoc` structs representing the options in the attribute set or an error.
#[allow(clippy::too_many_arguments)]
fn parse_attrset(
    node: &SyntaxNode,
    file_path: &str,
    current_prefix: &str,
    replacements: &HashMap<String, String>,
    source_text: &str,
    aliases: &HashMap<String, String>,
    let_bindings: &HashMap<String, SyntaxNode>,
    condition: Option<&str>,
    submodule_stack: &[rnix::TextRange],
    budget: &mut ExpansionBudget,
) -> Result<Vec<OptionDoc>, Box<dyn std::error::Error + Send + Sync>> {
    let mut options = Vec::new();
    let node = &unwrap_paren(node);

    match node.kind() {
        // Nested attributes
        SyntaxKind::NODE_ATTR_SET => {
            for child in node.children() {
                let mut child_options = visit_node(
                    &child,
                    file_path,
                    current_prefix,
                    replacements,
                    source_text,
                    aliases,
                    let_bindings,
                    condition,
                    submodule_stack,
                    budget,
                )?;
                options.append(&mut child_options);
            }
        }
        // Child node, parse for mkOption, mkEnableOption, or mkPackageOption
        SyntaxKind::NODE_APPLY => {
            let Some((fn_name, args)) = resolve_call(node, aliases) else {
                log::debug!("Could not resolve a function call for option node");
                return Ok(options);
            };

            match fn_name.as_str() {
                "mkMerge" => {
                    // `mkMerge [ a b c ]` merges its list items at the same
                    // level as the attribute it's assigned to, so each item
                    // is parsed at the same current_prefix/condition rather
                    // than as a nested level.
                    if let Some(list) = args.first().and_then(|n| ast::List::cast(n.clone())) {
                        for item in list.items() {
                            let mut nested = parse_attrset(
                                item.syntax(),
                                file_path,
                                current_prefix,
                                replacements,
                                source_text,
                                aliases,
                                let_bindings,
                                condition,
                                submodule_stack,
                                budget,
                            )?;
                            options.append(&mut nested);
                        }
                    }
                }
                "mkIf" => {
                    if let Some(value) = args.get(1) {
                        let new_condition =
                            combine_condition(condition, &format_condition(&args[0]));
                        let mut nested = parse_attrset(
                            value,
                            file_path,
                            current_prefix,
                            replacements,
                            source_text,
                            aliases,
                            let_bindings,
                            Some(&new_condition),
                            submodule_stack,
                            budget,
                        )?;
                        options.append(&mut nested);
                    }
                }
                "mkEnableOption" => {
                    let description = args
                        .first()
                        .filter(|n| n.kind() == SyntaxKind::NODE_STRING)
                        .map(|n| process_description(&string_text(n), replacements))
                        .filter(|d| !d.trim().is_empty());

                    // `mkEnableOption ""` is a common idiom (usually
                    // paired with `// { default = true; }`) for options
                    // whose purpose is already clear from their own
                    // name, but nixpkgs' own default text then reads as
                    // "Whether to enable ." with nothing to enable. Fall
                    // back to the option's own leaf name so it stays
                    // informative instead of just looking broken.
                    //
                    // The span is built with `inline_code` rather than a hard-coded pair of
                    // backticks because `leaf` is a slice of an attribute key taken verbatim
                    // from the scanned Nix source and may contain a backtick, which would
                    // close the span early and leak the rest of the name into the prose
                    // (issue #60; same defect class as #12 and #49). The empty-leaf filter
                    // matters because `rsplit` always yields `Some` - `Some("")` for an
                    // empty prefix, reachable via an interpolated key with an empty
                    // `--replace` value - and both `format!("`{leaf}`")` and
                    // `inline_code("")` would then emit a visible-but-empty code span
                    // instead of degrading to nixpkgs' plain "Whether to enable .".
                    let subject = description.unwrap_or_else(|| {
                        current_prefix
                            .rsplit('.')
                            .next()
                            .filter(|leaf| !leaf.is_empty())
                            .map(crate::utils::inline_code)
                            .unwrap_or_default()
                    });

                    budget.record_option();
                    options.push(OptionDoc {
                        name: current_prefix.to_string(),
                        description: Some(format!("Whether to enable {subject}.")),
                        nix_type: "boolean".to_string(),
                        default_value: Some(String::from("false")),
                        example: Some(String::from("true")),
                        renamed_to: None,
                        declarations: vec![Declaration {
                            file_path: file_path.to_string(),
                            line_number: get_line_number(node, source_text),
                            description: None,
                            condition: condition.map(str::to_string),
                        }],
                    });
                }
                "mkOption" => {
                    let mut nix_type = "any".to_string();
                    let mut type_node: Option<SyntaxNode> = None;
                    let mut description = None;
                    let mut default_value = None;
                    let mut example = None;

                    if let Some(attr_set) = args
                        .first()
                        .filter(|n| n.kind() == SyntaxKind::NODE_ATTR_SET)
                    {
                        for attr in attr_set.children() {
                            if attr.kind() == SyntaxKind::NODE_ATTRPATH_VALUE {
                                let attr_key = attr
                                    .children()
                                    .find(|n| n.kind() == SyntaxKind::NODE_ATTRPATH)
                                    .and_then(|n| n.children().next())
                                    .map(|n| n.text().to_string());

                                let attr_value = attr.children().nth(1);

                                match (attr_key.as_deref(), attr_value) {
                                    (Some("type"), Some(v)) => {
                                        nix_type = types::format_type(&v, aliases);
                                        type_node = Some(v);
                                    }
                                    (Some("description"), Some(v)) => {
                                        description = Some(process_description(
                                            &string_text(&v),
                                            replacements,
                                        ));
                                    }
                                    (Some("default"), Some(v)) => {
                                        default_value = Some(render_value(&v, aliases));
                                    }
                                    (Some("example"), Some(v)) => {
                                        example = Some(render_value(&v, aliases));
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }

                    budget.record_option();
                    options.push(OptionDoc {
                        name: current_prefix.to_string(),
                        description,
                        nix_type,
                        default_value,
                        example,
                        renamed_to: None,
                        declarations: vec![Declaration {
                            file_path: file_path.to_string(),
                            line_number: get_line_number(node, source_text),
                            description: None,
                            condition: condition.map(str::to_string),
                        }],
                    });

                    // Statically-analysable inline submodules: recurse into
                    // `options = { ... };` so nested options show up too,
                    // rather than only showing "submodule" as an opaque type.
                    if let Some(type_node) = type_node {
                        if let Some((body, is_container)) =
                            types::find_submodule_body(&type_node, aliases, let_bindings)
                        {
                            let body_range = body.text_range();
                            // A submodule type that resolves back to a body
                            // already being expanded is a recursive
                            // (tree-shaped) type - Nix `let` bindings are
                            // recursive, so following it would never
                            // terminate. Document the option itself (done
                            // above) and stop nesting here. The depth cap
                            // is a backstop for acyclic-but-pathological
                            // chains of distinct submodule bodies.
                            if submodule_stack.contains(&body_range)
                                || submodule_stack.len() >= types::MAX_SUBMODULE_DEPTH
                            {
                                log::debug!(
                                    "Not expanding submodule for {current_prefix}: recursive or too deeply nested submodule type"
                                );
                            } else if budget.is_exhausted() {
                                // Depth is bounded above, but breadth is
                                // independent of depth: a wide, acyclic,
                                // comfortably-shallow tree of distinct
                                // submodule bodies still expands to ~b^d
                                // options (nix-options-doc#21). Stop
                                // expanding once this file has produced
                                // implausibly many options, keeping what
                                // was found rather than failing the run.
                                //
                                // Note this bounds *expansion*, not emission:
                                // frames already in flight still finish
                                // emitting their own bodies' remaining direct
                                // options as the recursion unwinds. Each is a
                                // distinct body (see the cycle guard above),
                                // so that overshoot is bounded by the options
                                // declared in the file - linear in file size,
                                // not a constant (nix-options-doc#46).
                                budget.warn_once(file_path);
                            } else {
                                // Charge the re-walk this expansion is about
                                // to do, not just the options it produces:
                                // the same body is re-traversed on every
                                // expansion, so a body padded with
                                // non-option attributes would otherwise be
                                // re-walked for free (nix-options-doc#47).
                                // Charging on entry (rather than on exit)
                                // keeps the overshoot to one body.
                                budget.record_expansion(usize::from(body_range.len()));
                                let mut nested_stack = submodule_stack.to_vec();
                                nested_stack.push(body_range);

                                let nested_prefix = if is_container {
                                    format!("{}.<name>", current_prefix)
                                } else {
                                    current_prefix.to_string()
                                };

                                if let Some(options_attrset) =
                                    types::submodule_options_attrset(&body)
                                {
                                    let mut nested = parse_attrset(
                                        &options_attrset,
                                        file_path,
                                        &nested_prefix,
                                        replacements,
                                        source_text,
                                        aliases,
                                        let_bindings,
                                        condition,
                                        &nested_stack,
                                        budget,
                                    )?;
                                    options.append(&mut nested);
                                }

                                // `freeformType` marks a submodule as also
                                // accepting undeclared options validated
                                // against that type, alongside whatever's
                                // explicitly listed in `options` - surface it
                                // as a placeholder entry rather than silently
                                // dropping it.
                                if let Some(freeform_type_node) = types::find_freeform_type(&body) {
                                    budget.record_option();
                                    options.push(OptionDoc {
                                        name: format!("{}.<freeform>", nested_prefix),
                                        description: Some(
                                            "Any additional option accepted by this module's freeform type."
                                                .to_string(),
                                        ),
                                        nix_type: types::format_type(&freeform_type_node, aliases),
                                        default_value: None,
                                        example: None,
                                        renamed_to: None,
                                        declarations: vec![Declaration {
                                            file_path: file_path.to_string(),
                                            line_number: get_line_number(node, source_text),
                                            description: None,
                                            condition: condition.map(str::to_string),
                                        }],
                                    });
                                }
                            }
                        }
                    }
                }
                "mkPackageOption" | "mkPackageOptionMD" => {
                    // `mkPackageOption pkgs name { default?; example?;
                    // description?; extraDescription?; }` - `name` is
                    // either a string or a pkgs attribute path (list of
                    // strings), and everything else is optional.
                    let name_segments = args
                        .get(1)
                        .and_then(|n| match n.kind() {
                            SyntaxKind::NODE_STRING => Some(vec![string_text(n)]),
                            SyntaxKind::NODE_LIST => list_of_strings(n),
                            _ => None,
                        })
                        .unwrap_or_default();

                    let overrides = args
                        .get(2)
                        .filter(|n| n.kind() == SyntaxKind::NODE_ATTR_SET);

                    let default_segments = overrides
                        .and_then(|attr_set| find_attr(attr_set, "default"))
                        .and_then(|v| list_of_strings(&v))
                        .unwrap_or_else(|| name_segments.clone());

                    let default_value = if default_segments.is_empty() {
                        None
                    } else {
                        Some(format!("pkgs.{}", default_segments.join(".")))
                    };

                    let mut description = overrides
                        .and_then(|attr_set| find_attr(attr_set, "description"))
                        .filter(|n| n.kind() == SyntaxKind::NODE_STRING)
                        .map_or_else(
                            || format!("The {} package to use.", name_segments.join(" ")),
                            |v| process_description(&string_text(&v), replacements),
                        );

                    if let Some(extra) = overrides
                        .and_then(|attr_set| find_attr(attr_set, "extraDescription"))
                        .filter(|n| n.kind() == SyntaxKind::NODE_STRING)
                        .map(|v| process_description(&string_text(&v), replacements))
                    {
                        description = format!("{} {}", description, extra);
                    }

                    let example = overrides
                        .and_then(|attr_set| find_attr(attr_set, "example"))
                        .map(|v| render_value(&v, aliases));

                    budget.record_option();
                    options.push(OptionDoc {
                        name: current_prefix.to_string(),
                        description: Some(description),
                        nix_type: "package".to_string(),
                        default_value,
                        example,
                        renamed_to: None,
                        declarations: vec![Declaration {
                            file_path: file_path.to_string(),
                            line_number: get_line_number(node, source_text),
                            description: None,
                            condition: condition.map(str::to_string),
                        }],
                    });
                }
                _ => {
                    log::debug!("Not a recognized option function: {}", fn_name);
                }
            }
        }
        // Handle `with <expr>;`
        SyntaxKind::NODE_WITH => {
            if let Some(body) = node.children().nth(1) {
                let mut nested_options = visit_node(
                    &body,
                    file_path,
                    current_prefix,
                    replacements,
                    source_text,
                    aliases,
                    let_bindings,
                    condition,
                    submodule_stack,
                    budget,
                )?;
                options.append(&mut nested_options);
            }
        }
        // Handle `let <bindings> in <attrset>` as an attribute value
        // (e.g. `options.foo = let settingsFormat = ...; in { ... };`,
        // used to define a local helper before the options attrset).
        SyntaxKind::NODE_LET_IN => {
            if let Some(body) = ast::LetIn::cast(node.clone()).and_then(|let_in| let_in.body()) {
                let mut nested_options = parse_attrset(
                    body.syntax(),
                    file_path,
                    current_prefix,
                    replacements,
                    source_text,
                    aliases,
                    let_bindings,
                    condition,
                    submodule_stack,
                    budget,
                )?;
                options.append(&mut nested_options);
            }
        }
        // Handle `<expr> // { field = value; ... }` (attrset update),
        // e.g. `mkEnableOption "" // { default = true; }` - the
        // nixpkgs-recommended way to write an enable option that
        // defaults to true, since mkEnableOption itself always
        // defaults to false. Parse the left side normally, then apply
        // any default/description/example/type fields from the right
        // side as overrides on top of it.
        SyntaxKind::NODE_BIN_OP => {
            if let Some(bin_op) = ast::BinOp::cast(node.clone()) {
                if bin_op.operator() == Some(ast::BinOpKind::Update) {
                    if let (Some(lhs), Some(rhs)) = (bin_op.lhs(), bin_op.rhs()) {
                        let mut base = parse_attrset(
                            lhs.syntax(),
                            file_path,
                            current_prefix,
                            replacements,
                            source_text,
                            aliases,
                            let_bindings,
                            condition,
                            submodule_stack,
                            budget,
                        )?;

                        let rhs_node = unwrap_paren(rhs.syntax());
                        if rhs_node.kind() == SyntaxKind::NODE_ATTR_SET {
                            let overrides = scan_option_overrides(&rhs_node, aliases, replacements);
                            for opt in &mut base {
                                if let Some(nix_type) = &overrides.nix_type {
                                    opt.nix_type = nix_type.clone();
                                }
                                if overrides.description.is_some() {
                                    opt.description = overrides.description.clone();
                                }
                                if overrides.default_value.is_some() {
                                    opt.default_value = overrides.default_value.clone();
                                }
                                if overrides.example.is_some() {
                                    opt.example = overrides.example.clone();
                                }
                            }
                        }

                        options.append(&mut base);
                    }
                }
            }
        }
        _ => {
            log::debug!(
                "Unhandled node kind: {:?} at prefix {:?}",
                node.kind(),
                current_prefix
            );
        }
    }

    Ok(options)
}

/// Scans an entire file (regardless of where in the tree they appear -
/// typically inside `imports = [ ... ];`, not under `options`) for
/// `mkRenamedOptionModule`/`mkRemovedOptionModule` shims, and represents
/// each as a synthetic `OptionDoc` at the old option's name so it shows
/// up alongside real options in search, filtering, and every output
/// format without any special-casing there.
///
/// The description is written as a GitHub-style admonition so Markdown
/// renders it as a callout directly, and HTML gets the same styling for
/// free since descriptions already go through the same markdown
/// pipeline there.
pub fn find_deprecations(
    node: &SyntaxNode,
    file_path: &str,
    source_text: &str,
    aliases: &HashMap<String, String>,
) -> Vec<OptionDoc> {
    let mut found = Vec::new();

    if node.kind() == SyntaxKind::NODE_APPLY {
        if let Some((fn_name, args)) = resolve_call(node, aliases) {
            match fn_name.as_str() {
                "mkRenamedOptionModule" => {
                    if let (Some(old_path), Some(new_path)) = (
                        args.first().and_then(list_of_strings).map(|p| p.join(".")),
                        args.get(1).and_then(list_of_strings).map(|p| p.join(".")),
                    ) {
                        // `mkRenamedOptionModule`'s arguments are bare
                        // config paths (the same path used at both
                        // `options.<path>` and `config.<path>`), but real
                        // options in the rest of the document are named
                        // from the literal attrset structure in the file,
                        // which by convention starts with a literal
                        // `options` key. Prefix the *entry's own name*
                        // with `options.` to match that (and to keep
                        // --strip-prefix's options. default working the
                        // same way on both) - but not the "use X instead"
                        // text, since that's meant to be typed into a
                        // user's config as-is, where `options.` would be
                        // wrong.
                        //
                        // The link to the new option isn't built here:
                        // --strip-prefix runs later (in filter_options)
                        // and changes what the target's actual anchor
                        // ends up being, so linking it here would go
                        // stale the moment --strip-prefix is used.
                        // renamed_to carries the bare target path for
                        // filter_options to resolve once that's known.
                        // The span is built with `inline_code` rather than a hard-coded
                        // pair of backticks because `new_path` is arbitrary text from the
                        // module author's Nix source and may contain a backtick (issue
                        // #49). `filter_options` re-finds this exact span to turn it into
                        // a link, so it must call `inline_code` on the same string - see
                        // `crate::filter_options`.
                        let mut option = deprecation_option_doc(
                            &format!("options.{old_path}"),
                            format!(
                                "> [!WARNING]\n> This option was renamed. Use {} instead.",
                                crate::utils::inline_code(&new_path)
                            ),
                            "renamed option",
                            file_path,
                            get_line_number(node, source_text),
                        );
                        option.renamed_to = Some(new_path);
                        found.push(option);
                    }
                    // `resolve_call` already unwinds the whole curried
                    // application chain, so the nested Apply nodes making
                    // up that chain resolve to this same call at
                    // progressively smaller (incomplete) arg lists -
                    // don't recurse into them and double-count it.
                    return found;
                }
                "mkRemovedOptionModule" => {
                    if let Some(old_path) =
                        args.first().and_then(list_of_strings).map(|p| p.join("."))
                    {
                        let message = args
                            .get(1)
                            .filter(|n| n.kind() == SyntaxKind::NODE_STRING)
                            .map(string_text)
                            .filter(|m| !m.trim().is_empty());
                        let detail = match message {
                            Some(message) => format!(" {message}"),
                            None => String::new(),
                        };
                        found.push(deprecation_option_doc(
                            &format!("options.{old_path}"),
                            format!("> [!WARNING]\n> This option has been removed.{detail}"),
                            "removed option",
                            file_path,
                            get_line_number(node, source_text),
                        ));
                    }
                    return found;
                }
                _ => {}
            }
        }
    }

    for child in node.children() {
        found.extend(find_deprecations(&child, file_path, source_text, aliases));
    }

    found
}

/// Builds a synthetic `OptionDoc` representing a rename/removal shim.
fn deprecation_option_doc(
    name: &str,
    description: String,
    nix_type: &str,
    file_path: &str,
    line_number: usize,
) -> OptionDoc {
    OptionDoc {
        name: name.to_string(),
        description: Some(description),
        nix_type: nix_type.to_string(),
        default_value: None,
        example: None,
        renamed_to: None,
        declarations: vec![Declaration {
            file_path: file_path.to_string(),
            line_number,
            description: None,
            condition: None,
        }],
    }
}
