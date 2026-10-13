use super::*;

#[test]
fn the_lexer_masks_minified_names_and_keeps_strings_regexes_and_numbers() {
    let toks =
        lex("var aB=function(q){return /a\\/b[/]c/g.test(q)?0x1F:1.5e3}// gone\n/* gone */`t${x}`");
    assert_eq!(
        toks,
        vec![
            "var",
            "I",
            "=",
            "function",
            "(",
            "I",
            ")",
            "{",
            "return",
            "/a\\/b[/]c/g",
            ".",
            "I",
            "(",
            "I",
            ")",
            "?",
            "0x1F",
            ":",
            "1.5e3",
            "}",
            "`t${x}`"
        ]
    );
    // lib_<hash8>[_n] is the unpack's own minted name: masked too.
    assert_eq!(
        lex("lib_0123abcd lib_0123abcd_2 library"),
        vec!["I", "I", "library"]
    );
    // After an identifier a `/` divides.
    assert_eq!(lex("a/b/c"), vec!["I", "/", "I", "/", "I"]);
}

#[test]
fn string_tokens_are_read_by_their_quote() {
    assert!(is_string_token("\"ab\""));
    assert!(is_string_token("'ab'"));
    assert!(is_string_token("`a${b}`"));
    assert!(!is_string_token("ab"));
    assert_eq!(unquoted("\"hello\""), "hello");
    assert_eq!(unquoted("`t${x}`"), "t${x}");
    // A token shorter than two characters has no inside.
    assert_eq!(unquoted("\""), "");
}
