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

// The test vectors' JSON (strings kept as UTF-16 units).
#[derive(Debug)]
enum J {
    Str(Vec<u16>),
    List(Vec<J>),
    Obj(Vec<(String, J)>),
    Other,
}

impl J {
    fn get(&self, k: &str) -> Option<&J> {
        match self {
            J::Obj(m) => m.iter().find(|(x, _)| x == k).map(|(_, v)| v),
            _ => None,
        }
    }
    fn list(&self) -> Option<&[J]> {
        match self {
            J::List(xs) => Some(xs),
            _ => None,
        }
    }
    fn str16(&self) -> Option<&[u16]> {
        match self {
            J::Str(s) => Some(s),
            _ => None,
        }
    }
}

struct JParser<'a> {
    b: &'a [u8],
    k: usize,
}

impl JParser<'_> {
    fn ws(&mut self) {
        while self.k < self.b.len() && self.b[self.k].is_ascii_whitespace() {
            self.k += 1;
        }
    }
    fn value(&mut self) -> Result<J, String> {
        self.ws();
        let c = *self.b.get(self.k).ok_or("unexpected end of JSON")?;
        match c {
            b'{' => {
                self.k += 1;
                let mut m = Vec::new();
                loop {
                    self.ws();
                    if self.b.get(self.k) == Some(&b'}') {
                        self.k += 1;
                        return Ok(J::Obj(m));
                    }
                    let J::Str(key) = self.value()? else {
                        return Err("a JSON key is not a string".into());
                    };
                    self.ws();
                    self.k += 1; // ':'
                    let v = self.value()?;
                    m.push((String::from_utf16_lossy(&key), v));
                    self.ws();
                    if self.b.get(self.k) == Some(&b',') {
                        self.k += 1;
                    }
                }
            }
            b'[' => {
                self.k += 1;
                let mut xs = Vec::new();
                loop {
                    self.ws();
                    if self.b.get(self.k) == Some(&b']') {
                        self.k += 1;
                        return Ok(J::List(xs));
                    }
                    xs.push(self.value()?);
                    self.ws();
                    if self.b.get(self.k) == Some(&b',') {
                        self.k += 1;
                    }
                }
            }
            b'"' => {
                self.k += 1;
                let mut out: Vec<u16> = Vec::new();
                let mut run = self.k;
                loop {
                    let c = *self.b.get(self.k).ok_or("unterminated JSON string")?;
                    if c == b'"' || c == b'\\' {
                        let raw =
                            std::str::from_utf8(&self.b[run..self.k]).map_err(|e| e.to_string())?;
                        out.extend(raw.encode_utf16());
                    }
                    if c == b'"' {
                        self.k += 1;
                        return Ok(J::Str(out));
                    }
                    if c == b'\\' {
                        let e = *self.b.get(self.k + 1).ok_or("bad JSON escape")?;
                        self.k += 2;
                        match e {
                            b'u' => {
                                let h = std::str::from_utf8(&self.b[self.k..self.k + 4])
                                    .map_err(|e| e.to_string())?;
                                out.push(u16::from_str_radix(h, 16).map_err(|e| e.to_string())?);
                                self.k += 4;
                            }
                            b'n' => out.push(10),
                            b'r' => out.push(13),
                            b't' => out.push(9),
                            b'b' => out.push(8),
                            b'f' => out.push(12),
                            other => out.push(other as u16),
                        }
                        run = self.k;
                        continue;
                    }
                    self.k += 1;
                }
            }
            _ => {
                // a number, true, false or null: skipped (the vectors read
                // only strings, lists and objects)
                while self.k < self.b.len()
                    && !b",]}".contains(&self.b[self.k])
                    && !self.b[self.k].is_ascii_whitespace()
                {
                    self.k += 1;
                }
                Ok(J::Other)
            }
        }
    }
}
