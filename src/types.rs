//! Formats Nix type expressions (as used in `mkOption { type = ...; }`)
//! into human-readable strings, the same way nixpkgs' `optionType.description`
//! composes them for the manual - without evaluating anything.
//!
//! Type combinators (`nullOr`, `listOf`, `attrsOf`, ...) are almost always
//! written directly as syntax rather than computed, so this is a purely
//! syntactic, best-effort formatter: anything it doesn't recognize falls
//! back to the original (dedented) source text rather than guessing wrong.

use crate::nix_call::{find_attr, resolve_call};
use crate::utils::custom_dedent;
use rnix::ast::{self, Expr};
use rnix::{SyntaxKind, SyntaxNode};
use rowan::ast::AstNode;
use std::collections::HashMap;

/// Formats a type expression node into a human-readable description.
pub fn format_type(node: &SyntaxNode, aliases: &HashMap<String, String>) -> String {
    format_node(node, aliases).unwrap_or_else(|| custom_dedent(node.text().to_string().trim()))
}

fn format_node(node: &SyntaxNode, aliases: &HashMap<String, String>) -> Option<String> {
    match node.kind() {
        SyntaxKind::NODE_WITH => {
            let with = ast::With::cast(node.clone())?;
            format_node(with.body()?.syntax(), aliases)
        }
        SyntaxKind::NODE_PAREN => {
            let paren = ast::Paren::cast(node.clone())?;
            format_node(paren.expr()?.syntax(), aliases)
        }
        SyntaxKind::NODE_APPLY => format_call(node, aliases),
        SyntaxKind::NODE_IDENT => format_ident(&ident_text(node)?),
        SyntaxKind::NODE_SELECT => format_ident(&select_last_segment(node)?),
        _ => None,
    }
}

fn format_call(node: &SyntaxNode, aliases: &HashMap<String, String>) -> Option<String> {
    let (name, args) = resolve_call(node, aliases)?;
    let arg = |i: usize| args.get(i).map(|n| format_type_or_fallback(n, aliases));

    match name.as_str() {
        "nullOr" => Some(format!("null or {}", arg(0)?)),
        "listOf" => Some(format!("list of {}", arg(0)?)),
        "nonEmptyListOf" => Some(format!("non-empty list of {}", arg(0)?)),
        "attrsOf" => Some(format!("attribute set of {}", arg(0)?)),
        "lazyAttrsOf" => Some(format!("lazy attribute set of {}", arg(0)?)),
        "uniq" | "coercedTo" | "addCheck" => arg(0),
        "functionTo" => Some(format!("function that evaluates to {}", arg(0)?)),
        "either" => Some(format!("{} or {}", arg(0)?, arg(1)?)),
        "oneOf" => format_list_arg(args.first()?, aliases, " or ", None),
        "enum" => format_list_arg(args.first()?, aliases, ", ", Some("one of ")),
        "submodule" | "submoduleWith" => Some("submodule".to_string()),
        _ => None,
    }
}

/// Formats a node that is itself a nested type expression, falling back to
/// its raw source text if unrecognized (rather than propagating `None` and
/// losing the whole enclosing combinator's formatting).
fn format_type_or_fallback(node: &SyntaxNode, aliases: &HashMap<String, String>) -> String {
    format_node(node, aliases).unwrap_or_else(|| custom_dedent(node.text().to_string().trim()))
}

/// Formats a `NODE_LIST` argument's items (string or bare literals),
/// joining them with `sep` and optionally prefixing the whole result.
fn format_list_arg(
    node: &SyntaxNode,
    aliases: &HashMap<String, String>,
    sep: &str,
    prefix: Option<&str>,
) -> Option<String> {
    let list = ast::List::cast(node.clone())?;
    let items: Vec<String> = list
        .items()
        .map(|item| format_type_or_fallback(item.syntax(), aliases))
        .collect();
    if items.is_empty() {
        return None;
    }
    let joined = items.join(sep);
    Some(match prefix {
        Some(p) => format!("{p}{joined}"),
        None => joined,
    })
}

fn ident_text(node: &SyntaxNode) -> Option<String> {
    let ident = ast::Ident::cast(node.clone())?;
    ident.ident_token().map(|t| t.text().to_string())
}

fn select_last_segment(node: &SyntaxNode) -> Option<String> {
    let select = ast::Select::cast(node.clone())?;
    let attr = select.attrpath()?.attrs().last()?;
    match attr {
        ast::Attr::Ident(ident) => ident.ident_token().map(|t| t.text().to_string()),
        ast::Attr::Str(_) | ast::Attr::Dynamic(_) => None,
    }
}

/// Maps a bare type identifier (e.g. `str`, `bool`, the last segment of
/// `types.package`) to the human-readable description nixpkgs uses.
/// Unknown identifiers are returned as-is - still an improvement over
/// dumping the whole surrounding expression.
fn format_ident(name: &str) -> Option<String> {
    let mapped = match name {
        "bool" => "boolean",
        "str" | "string" => "string",
        "nonEmptyStr" => "non-empty string",
        "int" => "signed integer",
        "float" => "floating point number",
        "number" => "number",
        "path" => "path",
        "package" => "package",
        "attrs" => "attribute set",
        "lines" => "strings concatenated with \"\\n\"",
        "anything" | "any" | "unspecified" => "anything",
        "raw" => "raw value",
        _ => name,
    };
    Some(mapped.to_string())
}

/// Detects whether a type expression node resolves to `submodule ...`,
/// `attrsOf (submodule ...)`, or `listOf (submodule ...)`, returning the
/// submodule's body expression node (an attrset or a lambda) if so, along
/// with whether it's wrapped in a container that should get a `<name>`
/// placeholder segment when its nested options are expanded.
pub fn find_submodule_body(
    node: &SyntaxNode,
    aliases: &HashMap<String, String>,
) -> Option<(SyntaxNode, bool)> {
    let node = unwrap_paren(node);
    let (name, args) = resolve_call(&node, aliases)?;
    match name.as_str() {
        "submodule" | "submoduleWith" => Some((unwrap_paren(args.first()?), false)),
        "attrsOf" | "listOf" | "nonEmptyListOf" | "lazyAttrsOf" | "nullOr" | "uniq" => {
            let inner = args.first()?;
            let (body, _) = find_submodule_body(inner, aliases)?;
            let is_container = !matches!(name.as_str(), "nullOr" | "uniq");
            Some((body, is_container))
        }
        _ => None,
    }
}

/// Unwraps any number of enclosing `NODE_PAREN` nodes, e.g. the
/// `(types.submodule { ... })` in `attrsOf (types.submodule { ... })`.
fn unwrap_paren(node: &SyntaxNode) -> SyntaxNode {
    let mut current = node.clone();
    while let Some(paren) = ast::Paren::cast(current.clone()) {
        match paren.expr() {
            Some(expr) => current = expr.syntax().clone(),
            None => break,
        }
    }
    current
}

/// Resolves a submodule body expression (an inline attrset, or a lambda
/// whose body is an attrset, e.g. `{ config, ... }: { options = ...; }`)
/// down to the attrset node that may contain an `options = { ... };`
/// binding.
pub fn submodule_options_attrset(body: &SyntaxNode) -> Option<SyntaxNode> {
    let attrset = match body.kind() {
        SyntaxKind::NODE_ATTR_SET => body.clone(),
        SyntaxKind::NODE_LAMBDA => {
            let lambda = ast::Lambda::cast(body.clone())?;
            match lambda.body()? {
                Expr::AttrSet(set) => set.syntax().clone(),
                _ => return None,
            }
        }
        _ => return None,
    };

    find_attr(&attrset, "options")
}
