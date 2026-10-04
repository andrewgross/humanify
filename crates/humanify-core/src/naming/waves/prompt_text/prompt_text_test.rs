use super::{Cap, excerpt};

fn long_call(mention_at: usize, lines: usize) -> (String, usize) {
    let mut code = String::from("register({\n");
    let mut at = 0;
    for i in 0..lines {
        if i == mention_at {
            at = code.len() + "  GOOGLE: () => ".len();
            code += "  GOOGLE: () => x0u,\n";
        } else {
            code += &format!("  KEY_{i}: () => {i},\n");
        }
    }
    code += "});";
    (code, at)
}

/// A mention inside the positional cut keeps the cut, byte for byte.
#[test]
fn a_mention_inside_the_cut_keeps_the_positional_cut() {
    let (code, at) = long_call(3, 30);
    let cut = excerpt(&code, Some((at, 3)), Cap::Snippet);
    assert_eq!(cut, excerpt(&code, None, Cap::Snippet));
    assert!(cut.contains("GOOGLE: () => x0u"), "{cut}");
}

/// Past the cut, the excerpt is the lines around the mention under the
/// statement's first line.
#[test]
fn a_mention_past_the_cut_windows_the_lines_around_it() {
    let (code, at) = long_call(20, 30);
    let cut = excerpt(&code, Some((at, 3)), Cap::Snippet);
    assert_eq!(
        cut,
        "register({\n  // ...\n  KEY_18: () => 18,\n  KEY_19: () => 19,\n  GOOGLE: () => x0u,\n  KEY_21: () => 21,\n  KEY_22: () => 22,\n  // ..."
    );
    let decl = excerpt(&code, Some((at, 3)), Cap::Declaration);
    assert!(decl.contains("GOOGLE: () => x0u"), "{decl}");
}

/// One line too long for the char cap: the chars around the mention.
#[test]
fn a_mention_on_an_overlong_line_windows_its_chars() {
    let names: Vec<String> = (0..400).map(|i| format!("v{i}")).collect();
    let code = format!("var {};", names.join(", "));
    let at = code.find("v200,").expect("present");
    let cut = excerpt(&code, Some((at, 4)), Cap::Declaration);
    assert!(cut.contains("v200,"), "{cut}");
    assert!(humanify_model::js::utf16_len(&cut) <= 1000, "{}", cut.len());
    assert!(cut.starts_with('…') && cut.ends_with('…'), "{cut}");
    // Near the line's end the unused right side widens the left.
    let at = code.find("v398,").expect("present");
    let cut = excerpt(&code, Some((at, 4)), Cap::Declaration);
    assert!(cut.starts_with('…') && cut.ends_with("v399;"), "{cut}");
    assert_eq!(humanify_model::js::utf16_len(&cut), 999);
}

/// Uncapped stays whole.
#[test]
fn an_uncapped_excerpt_is_the_whole_text() {
    let (code, at) = long_call(20, 30);
    assert_eq!(excerpt(&code, Some((at, 3)), Cap::Whole), code);
}
