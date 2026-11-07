use super::*;

#[test]
fn identifiers_and_opaque_text() {
    let t = tokenize_line("const a = b.c(\"s\", /re/g, 1.5);").unwrap();
    let idents: Vec<&str> = t
        .iter()
        .filter(|x| x.ident)
        .map(|x| x.text.as_str())
        .collect();
    assert_eq!(idents, ["a", "b", "c"]);
    assert!(t.iter().any(|x| x.text == "/re/g"));
    assert!(tokenize_line("const s = \"open").is_none());
    assert!(tokenize_line("x = `a ${b} c`;").is_some());
    assert!(tokenize_line("x = `open").is_none());
    let div = tokenize_line("x = a / b / c;").unwrap();
    assert_eq!(div.iter().filter(|t| t.text == "/").count(), 2);
}

#[test]
fn name_only_lines() {
    assert!(name_only("  return foo(bar);", "  return baz(qux);"));
    assert!(!name_only("  return foo(1);", "  return foo(2);"));
    assert!(!name_only("  return foo;", "  throw foo;"));
    assert!(!name_only("same", "same"));
}
