//! The ported tokenizer against tokens14.mjs's own output (acorn 8.19.0,
//! recorded from Node on hand-written inputs: regex-vs-division guesses,
//! templates with bad escapes, escaped keywords, numeric separators,
//! strict-mode octals, HTML comments, a hashbang, private names, a
//! TypeScript file's regex fallback, astral names, a lone surrogate).

use super::*;

/// (input, its fileTokens, its unitEvidence).
type Case = (Vec<u16>, Vec<Vec<u16>>, Vec<Vec<u16>>);

fn vectors() -> Vec<Case> {
    let text = include_str!("testdata/tokens14-vectors.json");
    let v = JParser {
        b: text.as_bytes(),
        k: 0,
    }
    .value()
    .expect("the vectors parse");
    let strs = |j: &J| -> Vec<Vec<u16>> {
        j.list()
            .expect("a list")
            .iter()
            .map(|x| x.str16().expect("a string").to_vec())
            .collect()
    };
    v.get("cases")
        .and_then(J::list)
        .expect("cases")
        .iter()
        .map(|c| {
            (
                c.get("input").and_then(J::str16).expect("input").to_vec(),
                strs(c.get("fileTokens").expect("fileTokens")),
                strs(c.get("unitEvidence").expect("unitEvidence")),
            )
        })
        .collect()
}

#[test]
fn file_tokens_are_node_s() {
    for (input, want, _) in vectors() {
        let text = utf16_lossy(String::from_utf16_lossy(&input).as_bytes());
        assert_eq!(
            file_tokens(&text),
            want,
            "{}",
            String::from_utf16_lossy(&input)
        );
    }
}

#[test]
fn unit_evidence_is_node_s() {
    for (input, _, want) in vectors() {
        let text = utf16_lossy(String::from_utf16_lossy(&input).as_bytes());
        assert_eq!(
            unit_evidence(&text),
            want,
            "{}",
            String::from_utf16_lossy(&input)
        );
    }
}

#[test]
fn wtf8_keeps_a_lone_surrogate_apart() {
    assert_eq!(wtf8(&[0x61]), b"a");
    assert_eq!(wtf8(&[0xd83d, 0xde00]), "\u{1F600}".as_bytes());
    assert_eq!(wtf8(&[0xd800]), vec![0xed, 0xa0, 0x80]);
    // code-point order: U+D800 sorts before U+E000 and U+1F600 after both
    assert!(wtf8(&[0xd800]) < wtf8(&[0xe000]));
    assert!(wtf8(&[0xe000]) < wtf8(&[0xd83d, 0xde00]));
}

#[test]
fn the_fallback_strings_stop_at_400_units() {
    let long: String = std::iter::repeat_n('x', 401).collect();
    let text: Vec<u16> = format!("\"{long}\" 'ok'").encode_utf16().collect();
    let mut out = OrderedSet::default();
    fallback_matches(&text, &mut out);
    // the 401-unit string is no match: its identifier run is, then 'ok'
    assert_eq!(
        out.items,
        vec![
            long.encode_utf16().collect::<Vec<u16>>(),
            "ok".encode_utf16().collect()
        ]
    );
}
