//! Name evidence: what of a unit no minifier renames, and whether a source
//! file holds it (exp136).

use super::evidence::{file_holds, name_evidence};

#[test]
fn a_units_evidence_is_its_string_contents_and_property_names() {
    let unit = r#"function nXn(e){return e!==null&&e.type==="Operator"&&(e.value.decodeEscaped(e.value.length-1)==="-"||e.tokenStart==='+')}"#;
    let toks = name_evidence(unit);
    // strings of 4+ characters with a word in them, `.property` names of
    // 4+; names nearly every file holds (type, value, length) say nothing;
    // locals are renamed by every minifier and are not read
    assert_eq!(
        toks.iter().copied().collect::<Vec<_>>(),
        ["Operator", "decodeEscaped", "tokenStart"]
    );
    // quotes pair up: the code between two strings is not a string
    assert!(
        name_evidence(r#"a="Operator"&&b==="-"||c.f==="+""#)
            .iter()
            .all(|t| !t.contains("||"))
    );
    // a comment is not evidence
    assert!(name_evidence("x/* .hiddenName \"quoted words\" */+1").is_empty());
}

#[test]
fn a_file_holds_a_unit_when_it_holds_half_its_evidence() {
    let toks =
        name_evidence(r#"x.decodeEscaped(y),x.consumeEscaped(z),"Hex or identifier is expected""#);
    assert_eq!(toks.len(), 3);
    let own = "export function consumeEscaped(s){}\nexport function decodeEscaped(s){ throw 'Hex or identifier is expected' }";
    assert_eq!(file_holds(own, &toks), Some(true));
    assert_eq!(
        file_holds("module.exports = require('./other.js');", &toks),
        Some(false)
    );
    // under three evidence tokens nothing is said
    let few = name_evidence("a.decodeEscaped(b)");
    assert_eq!(file_holds("", &few), None);
}
