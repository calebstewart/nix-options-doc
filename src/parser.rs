//! The parser module contains functions for parsing Nix syntax trees
//! and extracting option documentation.
//!
//! It traverses the abstract syntax tree of Nix files to identify
//! module options and their metadata.

use crate::nix_call::{find_attr, resolve_call};
use crate::types;
use crate::utils::{apply_replacements, clean_description, clean_literal_expr, custom_dedent};
use crate::OptionDoc;
use rnix::ast;
use rnix::{SyntaxKind, SyntaxNode};
use rowan::ast::AstNode;
use std::collections::HashMap;

/// Recursively traverses the syntax tree of a Nix file to extract option definitions.
///
/// # Arguments
/// - `node`: The current syntax node being processed.
/// - `file_path`: The relative file path of the Nix file for documentation reference.
/// - `prefix`: The current option name prefix in the hierarchy.
/// - `replacements`: A map of variable replacements for dynamic segments.
/// - `source_text`: The full text of the source file for line number calculation.
/// - `aliases`: Local function aliases (see [`crate::nix_call::collect_aliases`]).
///
/// # Returns
/// A vector of OptionDoc structs representing the found options or an error.
pub fn visit_node(
    node: &SyntaxNode,
    file_path: &str,
    prefix: &str,
    replacements: &HashMap<String, String>,
    source_text: &str,
    aliases: &HashMap<String, String>,
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
                )?;
                options.append(&mut nested_options);
            }
        }
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
            )?;
            options.append(&mut child_options);
        }
    }

    Ok(options)
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
    let dedented = custom_dedent(&replaced);
    clean_description(&dedented)
}

/// Extracts the unquoted text of a (single-line) string literal node.
fn string_text(node: &SyntaxNode) -> String {
    node.text()
        .to_string()
        .trim_matches(['"', '\''])
        .to_string()
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

/// Parses an attribute set node to extract NixOS module option definitions.
///
/// # Arguments
/// - `node`: The syntax node representing the attribute set.
/// - `file_path`: The file path of the Nix file for reference.
/// - `current_prefix`: The current option name hierarchy as a dot-separated string.
/// - `replacements`: A map of variable replacements for dynamic values.
/// - `source_text`: The source text of the file for line number calculation.
/// - `aliases`: Local function aliases (see [`crate::nix_call::collect_aliases`]).
///
/// # Returns
/// A vector of OptionDoc structs representing the options in the attribute set or an error.
fn parse_attrset(
    node: &SyntaxNode,
    file_path: &str,
    current_prefix: &str,
    replacements: &HashMap<String, String>,
    source_text: &str,
    aliases: &HashMap<String, String>,
) -> Result<Vec<OptionDoc>, Box<dyn std::error::Error + Send + Sync>> {
    let mut options = Vec::new();

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
                "mkEnableOption" => {
                    let description = args
                        .first()
                        .filter(|n| n.kind() == SyntaxKind::NODE_STRING)
                        .map(|n| process_description(&string_text(n), replacements));

                    options.push(OptionDoc {
                        name: current_prefix.to_string(),
                        description: Some(format!(
                            "Whether to enable {}.",
                            description.unwrap_or_default()
                        )),
                        nix_type: "boolean".to_string(),
                        default_value: Some(String::from("false")),
                        example: Some(String::from("true")),
                        file_path: file_path.to_string(),
                        line_number: get_line_number(node, source_text),
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

                    options.push(OptionDoc {
                        name: current_prefix.to_string(),
                        description,
                        nix_type,
                        default_value,
                        example,
                        file_path: file_path.to_string(),
                        line_number: get_line_number(node, source_text),
                    });

                    // Statically-analysable inline submodules: recurse into
                    // `options = { ... };` so nested options show up too,
                    // rather than only showing "submodule" as an opaque type.
                    if let Some(type_node) = type_node {
                        if let Some((body, is_container)) =
                            types::find_submodule_body(&type_node, aliases)
                        {
                            if let Some(options_attrset) = types::submodule_options_attrset(&body) {
                                let nested_prefix = if is_container {
                                    format!("{}.<name>", current_prefix)
                                } else {
                                    current_prefix.to_string()
                                };
                                let mut nested = parse_attrset(
                                    &options_attrset,
                                    file_path,
                                    &nested_prefix,
                                    replacements,
                                    source_text,
                                    aliases,
                                )?;
                                options.append(&mut nested);
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
                        .map(|v| process_description(&string_text(&v), replacements))
                        .unwrap_or_else(|| {
                            format!("The {} package to use.", name_segments.join(" "))
                        });

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

                    options.push(OptionDoc {
                        name: current_prefix.to_string(),
                        description: Some(description),
                        nix_type: "package".to_string(),
                        default_value,
                        example,
                        file_path: file_path.to_string(),
                        line_number: get_line_number(node, source_text),
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
                )?;
                options.append(&mut nested_options);
            }
        }
        _ => {
            log::debug!("Unhandled node kind: {:?}", node.kind());
        }
    }

    Ok(options)
}
