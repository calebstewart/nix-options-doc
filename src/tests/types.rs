use super::*;

/// Tests that common type combinators are formatted into human-readable
/// descriptions instead of being dumped as raw source text.
#[test]
fn test_type_formatter_combinators() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let temp_dir = TempDir::new()?;
    let content = r#"
{
  options.test = {
    a = lib.mkOption { type = lib.types.nullOr lib.types.bool; };
    b = lib.mkOption { type = lib.types.listOf lib.types.str; };
    c = lib.mkOption { type = lib.types.attrsOf lib.types.int; };
    d = lib.mkOption { type = lib.types.either lib.types.str lib.types.int; };
    e = lib.mkOption { type = lib.types.enum [ "a" "b" "c" ]; };
    f = lib.mkOption { type = lib.types.functionTo lib.types.str; };
  };
}
"#;
    create_test_file(temp_dir.path(), "types.nix", content)?;

    let options = collect_options(temp_dir.path(), &[], &HashMap::new(), false, false)?;
    let find = |name: &str| {
        options
            .iter()
            .find(|o| o.name == format!("options.test.{name}"))
            .unwrap()
            .nix_type
            .clone()
    };

    assert_eq!(find("a"), "null or boolean");
    assert_eq!(find("b"), "list of string");
    assert_eq!(find("c"), "attribute set of signed integer");
    assert_eq!(find("d"), "string or signed integer");
    assert_eq!(find("e"), "one of \"a\", \"b\", \"c\"");
    assert_eq!(find("f"), "function that evaluates to string");

    Ok(())
}
