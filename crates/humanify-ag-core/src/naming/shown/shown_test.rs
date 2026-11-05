use super::{shows, unshown};

#[test]
fn an_identifier_is_shown_only_as_a_whole_token() {
    let code = "var Kq_ = two();\nreturn Kq_ + abc;";
    assert!(shows(code, "Kq_"));
    assert!(shows(code, "abc"));
    // A substring is not the identifier: `a` is inside `abc`, `$a` is not
    // anywhere.
    assert!(!shows(code, "a"));
    assert!(!shows(code, "Kq"));
    assert!(!shows(code, "$a"));
    assert!(!shows(code, ""), "an empty name is never shown");
}

#[test]
fn unshown_lists_the_asked_identifiers_the_code_lacks_in_ask_order() {
    let code = "function f(a) {\n  return a + b;\n}";
    let asked = ["zz".to_string(), "a".to_string(), "Q".to_string()];
    assert_eq!(unshown(code, &asked), vec!["zz", "Q"]);
}
