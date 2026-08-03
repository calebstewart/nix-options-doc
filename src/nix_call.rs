//! Helpers for resolving (possibly curried) Nix function applications
//! using rnix's typed AST layer, and for collecting simple local
//! aliases (`let mkOpt = lib.mkOption; in ...`) so that calls through
//! a renamed binding can still be recognized.

use rnix::ast::{self, Expr, HasEntry};
use rnix::SyntaxNode;
use rowan::ast::AstNode;
use std::collections::HashMap;

/// Resolves the bare identifier a `Expr` refers to, unwrapping
/// parentheses and attribute selection (using the last path segment,
/// e.g. `lib.mkOption` -> `mkOption`).
fn ident_name(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Ident(ident) => ident.ident_token().map(|t| t.text().to_string()),
        Expr::Select(select) => {
            let attr = select.attrpath()?.attrs().last()?;
            match attr {
                ast::Attr::Ident(ident) => ident.ident_token().map(|t| t.text().to_string()),
                ast::Attr::Str(_) | ast::Attr::Dynamic(_) => None,
            }
        }
        Expr::Paren(paren) => ident_name(&paren.expr()?),
        _ => None,
    }
}

/// Resolves a curried function application (`f a b c`, parsed as
/// nested `NODE_APPLY`s) into the canonical function name and the
/// ordered list of argument nodes.
///
/// The function name is canonicalized through `aliases` (see
/// [`collect_aliases`]), so a call through a locally renamed binding
/// still resolves to the well-known name (e.g. `mkOption`).
pub fn resolve_call(
    node: &SyntaxNode,
    aliases: &HashMap<String, String>,
) -> Option<(String, Vec<SyntaxNode>)> {
    let mut current = ast::Apply::cast(node.clone())?;
    let mut args = Vec::new();

    loop {
        args.push(current.argument()?.syntax().clone());
        match current.lambda()? {
            Expr::Apply(inner) => current = inner,
            other => {
                let name = ident_name(&other)?;
                args.reverse();
                let canonical = aliases.get(&name).cloned().unwrap_or(name);
                return Some((canonical, args));
            }
        }
    }
}

/// Finds a top-level, single-segment attrset entry by key (e.g. `default`
/// in `{ default = ...; }`) and returns its value node. Dotted keys
/// (`a.b = ...;`) are intentionally not matched.
pub fn find_attr(attrset: &SyntaxNode, key: &str) -> Option<SyntaxNode> {
    let set = ast::AttrSet::cast(attrset.clone())?;
    set.attrpath_values().find_map(|entry| {
        let path = entry.attrpath()?;
        let mut attrs = path.attrs();
        let first = attrs.next()?;
        if attrs.next().is_some() {
            return None;
        }
        match first {
            ast::Attr::Ident(ident) if ident.ident_token()?.text() == key => {
                entry.value().map(|v| v.syntax().clone())
            }
            _ => None,
        }
    })
}

/// Scans a file for simple, non-shadowing local aliases of the form
/// `<name> = <ident-or-select>;` inside `let ... in ...` blocks, e.g.
/// `let mkOpt = lib.mkOption; in ...`. Returns a map from the local
/// name to the canonical name it refers to.
///
/// This intentionally only follows single, direct bindings - it does
/// not attempt full scope resolution (shadowing, nested nested
/// rebinding, etc.), since that's not needed to recognize the common
/// "rename a lib function for brevity" pattern.
pub fn collect_aliases(root: &SyntaxNode) -> HashMap<String, String> {
    let mut aliases = HashMap::new();
    collect_aliases_rec(root, &mut aliases);
    aliases
}

fn collect_aliases_rec(node: &SyntaxNode, aliases: &mut HashMap<String, String>) {
    if let Some(let_in) = ast::LetIn::cast(node.clone()) {
        for entry in let_in.attrpath_values() {
            let Some(attrpath) = entry.attrpath() else {
                continue;
            };
            let mut attrs = attrpath.attrs();
            let (Some(ast::Attr::Ident(key_ident)), None) = (attrs.next(), attrs.next()) else {
                // Only handle single-segment keys (`name = ...;`), not
                // dotted paths.
                continue;
            };
            let (Some(key), Some(value)) = (
                key_ident.ident_token().map(|t| t.text().to_string()),
                entry.value(),
            ) else {
                continue;
            };
            if let Some(target) = ident_name(&value) {
                aliases.insert(key, target);
            }
        }
    }

    for child in node.children() {
        collect_aliases_rec(&child, aliases);
    }
}
