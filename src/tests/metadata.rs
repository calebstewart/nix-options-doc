use super::*;

/// Regression test for #3: this repo is a hard fork, and the crate metadata
/// used to still name the upstream project. `CARGO_PKG_REPOSITORY` is not
/// merely informational here - `generate_markdown` and `generate_html` bake it
/// into the footer of every document they emit - so an upstream URL in
/// `Cargo.toml` silently misattributes all generated output.
#[test]
fn test_crate_metadata_identifies_this_fork() {
    assert_eq!(
        env!("CARGO_PKG_REPOSITORY"),
        "https://github.com/calebstewart/nix-options-doc"
    );
    assert_eq!(
        env!("CARGO_PKG_HOMEPAGE"),
        "https://github.com/calebstewart/nix-options-doc"
    );

    // MIT obliges us to keep crediting the original author, so this asserts
    // *both* names are present rather than that the upstream one is gone.
    let authors = env!("CARGO_PKG_AUTHORS");
    assert!(
        authors.contains("Chinmay D. Pai"),
        "the original author must stay credited, got: {authors:?}"
    );
    assert!(
        authors.contains("Caleb Stewart"),
        "the fork maintainer must be credited, got: {authors:?}"
    );
}

/// Regression test for #3, and a guard against the wrong fix: the Markdown and
/// HTML footers must keep deriving their link from `CARGO_PKG_REPOSITORY`
/// rather than from a hardcoded string, so the manifest stays the single source
/// of truth for the project URL.
#[test]
fn test_generated_footers_link_this_fork() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let repository = env!("CARGO_PKG_REPOSITORY");

    let markdown = generate_doc(&[], OutputFormat::Markdown, false)?;
    assert!(
        markdown.contains(&format!("*Generated with [nix-options-doc]({repository})*")),
        "markdown footer should link {repository}, got: {markdown:?}"
    );
    assert!(!markdown.contains("Thunderbottom"));

    let html = generate_doc(&[], OutputFormat::Html, false)?;
    assert!(
        html.contains(&format!(r#"<a href="{repository}">nix-options-doc</a>"#)),
        "html footer should link {repository}"
    );
    assert!(!html.contains("Thunderbottom"));

    Ok(())
}

/// Regression test for #3: the README's install instructions used to send
/// users to `github.com/Thunderbottom/nix-options-doc` - a different,
/// diverged codebase. The `Thunderbottom/flakes` live-example link is
/// deliberately *not* covered by this assertion: it is an upstream-generated
/// showcase we still credit, not an install source for this fork.
#[test]
fn test_readme_does_not_link_the_upstream_fork_source() {
    let readme = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("README.md"))
        .expect("README.md should be readable from the crate root");
    assert!(
        !readme.contains("Thunderbottom/nix-options-doc"),
        "README should point installs at this fork, not upstream"
    );
    assert!(
        readme.contains("calebstewart/nix-options-doc"),
        "README should reference this fork's repository"
    );
}
