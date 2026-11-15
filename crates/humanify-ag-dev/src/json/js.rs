//! The eval's JS spelling: `JSON.stringify(x, null, 2)` over values with
//! JavaScript object order (array-index keys first, then insertion order),
//! numbers printed as `String(x)`. The eval's cards, summaries, run
//! manifests and launch goldens are written in it, and the harness reads
//! them back through it; the pipeline itself writes plain serde JSON.

use std::fmt::Write as _;

use humanify_ag_model::js::{array_index, number_to_string};

/// A JSON value with JS object semantics (ordered, index keys first).
#[derive(Clone, Debug, PartialEq)]
pub enum JsValue {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<JsValue>),
    Object(JsObject),
}

/// An object in JS enumeration order. Construction goes through
/// [`JsObject::insert`], which applies the JS property rules, so the entry
/// order IS the order `Object.keys`/`JSON.stringify` would see.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct JsObject {
    entries: Vec<(String, JsValue)>,
}

impl JsObject {
    pub fn new() -> Self {
        JsObject::default()
    }

    /// `obj[key] = value`: an existing key keeps its position and takes the
    /// new value; a new index key slots in numerically ahead of every
    /// ordinary key; a new ordinary key appends.
    pub fn insert(&mut self, key: impl Into<String>, value: JsValue) {
        let key = key.into();
        if let Some(slot) = self.entries.iter_mut().find(|(k, _)| *k == key) {
            slot.1 = value;
            return;
        }
        let pos = match array_index(&key) {
            Some(index) => self
                .entries
                .iter()
                .position(|(k, _)| array_index(k).is_none_or(|other| other > index))
                .unwrap_or(self.entries.len()),
            None => self.entries.len(),
        };
        self.entries.insert(pos, (key, value));
    }

    /// Insert only when `value` is Some — JSON.stringify drops a property
    /// whose value is `undefined`.
    pub fn insert_opt(&mut self, key: &str, value: Option<JsValue>) {
        if let Some(value) = value {
            self.insert(key, value);
        }
    }

    pub fn get(&self, key: &str) -> Option<&JsValue> {
        self.entries.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    pub fn get_mut(&mut self, key: &str) -> Option<&mut JsValue> {
        self.entries
            .iter_mut()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v)
    }

    pub fn entries(&self) -> &[(String, JsValue)] {
        &self.entries
    }
}

impl FromIterator<(String, JsValue)> for JsObject {
    fn from_iter<I: IntoIterator<Item = (String, JsValue)>>(iter: I) -> Self {
        let mut obj = JsObject::new();
        for (k, v) in iter {
            obj.insert(k, v);
        }
        obj
    }
}

impl JsValue {
    pub fn str(s: impl Into<String>) -> JsValue {
        JsValue::String(s.into())
    }

    pub fn str_array<S: AsRef<str>>(items: &[S]) -> JsValue {
        JsValue::Array(items.iter().map(|s| JsValue::str(s.as_ref())).collect())
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            JsValue::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_object(&self) -> Option<&JsObject> {
        match self {
            JsValue::Object(o) => Some(o),
            _ => None,
        }
    }

    /// Parse JSON text with `JSON.parse` semantics (object order and
    /// duplicate-key rules above). Numbers are parsed from their exact
    /// literal with Rust's correctly-rounded `str::parse::<f64>` — NOT
    /// serde_json's default float parser, which is up to 1 ulp off on long
    /// literals (15-porting-lessons §8; the probe's 123456789012345680000
    /// came back as ...670000 through it).
    pub fn parse(text: &str) -> Result<JsValue, String> {
        let mut parser = Parser {
            bytes: text.as_bytes(),
            text,
            pos: 0,
        };
        parser.skip_ws();
        let value = parser.value()?;
        parser.skip_ws();
        if parser.pos != parser.bytes.len() {
            return Err(parser.error("unexpected trailing text"));
        }
        Ok(value)
    }
}

/// A strict JSON (RFC 8259) parser — JSON.parse accepts exactly this
/// grammar (whitespace = space, tab, LF, CR).
struct Parser<'a> {
    bytes: &'a [u8],
    text: &'a str,
    pos: usize,
}

impl Parser<'_> {
    fn error(&self, what: &str) -> String {
        format!("JSON parse error at byte {}: {what}", self.pos)
    }

    fn skip_ws(&mut self) {
        while let Some(b' ' | b'\t' | b'\n' | b'\r') = self.bytes.get(self.pos) {
            self.pos += 1;
        }
    }

    fn eat(&mut self, byte: u8) -> Result<(), String> {
        if self.bytes.get(self.pos) == Some(&byte) {
            self.pos += 1;
            Ok(())
        } else {
            Err(self.error(&format!("expected '{}'", byte as char)))
        }
    }

    fn literal(&mut self, word: &str, value: JsValue) -> Result<JsValue, String> {
        if self.text[self.pos..].starts_with(word) {
            self.pos += word.len();
            Ok(value)
        } else {
            Err(self.error("invalid literal"))
        }
    }

    fn value(&mut self) -> Result<JsValue, String> {
        match self.bytes.get(self.pos) {
            Some(b'{') => self.object(),
            Some(b'[') => self.array(),
            Some(b'"') => self.string().map(JsValue::String),
            Some(b't') => self.literal("true", JsValue::Bool(true)),
            Some(b'f') => self.literal("false", JsValue::Bool(false)),
            Some(b'n') => self.literal("null", JsValue::Null),
            Some(b'-' | b'0'..=b'9') => self.number(),
            _ => Err(self.error("expected a value")),
        }
    }

    fn object(&mut self) -> Result<JsValue, String> {
        self.eat(b'{')?;
        let mut obj = JsObject::new();
        self.skip_ws();
        if self.bytes.get(self.pos) == Some(&b'}') {
            self.pos += 1;
            return Ok(JsValue::Object(obj));
        }
        loop {
            self.skip_ws();
            let key = self.string()?;
            self.skip_ws();
            self.eat(b':')?;
            self.skip_ws();
            let value = self.value()?;
            obj.insert(key, value);
            self.skip_ws();
            match self.bytes.get(self.pos) {
                Some(b',') => self.pos += 1,
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(JsValue::Object(obj));
                }
                _ => return Err(self.error("expected ',' or '}'")),
            }
        }
    }

    fn array(&mut self) -> Result<JsValue, String> {
        self.eat(b'[')?;
        let mut items = Vec::new();
        self.skip_ws();
        if self.bytes.get(self.pos) == Some(&b']') {
            self.pos += 1;
            return Ok(JsValue::Array(items));
        }
        loop {
            self.skip_ws();
            items.push(self.value()?);
            self.skip_ws();
            match self.bytes.get(self.pos) {
                Some(b',') => self.pos += 1,
                Some(b']') => {
                    self.pos += 1;
                    return Ok(JsValue::Array(items));
                }
                _ => return Err(self.error("expected ',' or ']'")),
            }
        }
    }

    fn number(&mut self) -> Result<JsValue, String> {
        let start = self.pos;
        if self.bytes.get(self.pos) == Some(&b'-') {
            self.pos += 1;
        }
        match self.bytes.get(self.pos) {
            Some(b'0') => self.pos += 1,
            Some(b'1'..=b'9') => self.digits(),
            _ => return Err(self.error("invalid number")),
        }
        if self.bytes.get(self.pos) == Some(&b'.') {
            self.pos += 1;
            if !self.bytes.get(self.pos).is_some_and(u8::is_ascii_digit) {
                return Err(self.error("invalid fraction"));
            }
            self.digits();
        }
        if let Some(b'e' | b'E') = self.bytes.get(self.pos) {
            self.pos += 1;
            if let Some(b'+' | b'-') = self.bytes.get(self.pos) {
                self.pos += 1;
            }
            if !self.bytes.get(self.pos).is_some_and(u8::is_ascii_digit) {
                return Err(self.error("invalid exponent"));
            }
            self.digits();
        }
        let literal = &self.text[start..self.pos];
        literal
            .parse::<f64>()
            .map(JsValue::Number)
            .map_err(|_| self.error("invalid number"))
    }

    fn digits(&mut self) {
        while self.bytes.get(self.pos).is_some_and(u8::is_ascii_digit) {
            self.pos += 1;
        }
    }

    fn hex4(&mut self) -> Result<u32, String> {
        let hex = self
            .text
            .get(self.pos..self.pos + 4)
            .ok_or_else(|| self.error("short \\u escape"))?;
        let unit = u32::from_str_radix(hex, 16).map_err(|_| self.error("bad \\u escape"))?;
        self.pos += 4;
        Ok(unit)
    }

    fn string(&mut self) -> Result<String, String> {
        self.eat(b'"')?;
        let mut out = String::new();
        loop {
            let run_start = self.pos;
            while let Some(&b) = self.bytes.get(self.pos) {
                if b == b'"' || b == b'\\' || b < 0x20 {
                    break;
                }
                self.pos += 1;
            }
            out.push_str(&self.text[run_start..self.pos]);
            match self.bytes.get(self.pos) {
                Some(b'"') => {
                    self.pos += 1;
                    return Ok(out);
                }
                Some(b'\\') => {
                    self.pos += 1;
                    self.escape(&mut out)?;
                }
                _ => return Err(self.error("unterminated string or raw control character")),
            }
        }
    }

    fn escape(&mut self, out: &mut String) -> Result<(), String> {
        let byte = *self
            .bytes
            .get(self.pos)
            .ok_or_else(|| self.error("bad escape"))?;
        self.pos += 1;
        let c = match byte {
            b'"' => '"',
            b'\\' => '\\',
            b'/' => '/',
            b'b' => '\u{8}',
            b'f' => '\u{c}',
            b'n' => '\n',
            b'r' => '\r',
            b't' => '\t',
            b'u' => return self.unicode_escape(out),
            _ => return Err(self.error("bad escape")),
        };
        out.push(c);
        Ok(())
    }

    /// `\uXXXX`, pairing surrogates. A LONE surrogate is legal in a JS
    /// string but not in a Rust one: it becomes U+FFFD.
    fn unicode_escape(&mut self, out: &mut String) -> Result<(), String> {
        let unit = self.hex4()?;
        if (0xD800..0xDC00).contains(&unit) && self.text[self.pos..].starts_with("\\u") {
            let save = self.pos;
            self.pos += 2;
            let low = self.hex4()?;
            if (0xDC00..0xE000).contains(&low) {
                let code = 0x10000 + ((unit - 0xD800) << 10) + (low - 0xDC00);
                out.push(char::from_u32(code).unwrap_or('\u{FFFD}'));
                return Ok(());
            }
            self.pos = save;
        }
        out.push(char::from_u32(unit).unwrap_or('\u{FFFD}'));
        Ok(())
    }
}

impl<'de> serde::Deserialize<'de> for JsValue {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(JsValueVisitor)
    }
}

struct JsValueVisitor;

impl<'de> serde::de::Visitor<'de> for JsValueVisitor {
    type Value = JsValue;

    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("any JSON value")
    }
    fn visit_unit<E>(self) -> Result<JsValue, E> {
        Ok(JsValue::Null)
    }
    fn visit_none<E>(self) -> Result<JsValue, E> {
        Ok(JsValue::Null)
    }
    fn visit_bool<E>(self, v: bool) -> Result<JsValue, E> {
        Ok(JsValue::Bool(v))
    }
    fn visit_i64<E>(self, v: i64) -> Result<JsValue, E> {
        Ok(JsValue::Number(v as f64))
    }
    fn visit_u64<E>(self, v: u64) -> Result<JsValue, E> {
        Ok(JsValue::Number(v as f64))
    }
    fn visit_f64<E>(self, v: f64) -> Result<JsValue, E> {
        Ok(JsValue::Number(v))
    }
    fn visit_str<E>(self, v: &str) -> Result<JsValue, E> {
        Ok(JsValue::String(v.to_string()))
    }
    fn visit_string<E>(self, v: String) -> Result<JsValue, E> {
        Ok(JsValue::String(v))
    }
    fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<JsValue, A::Error> {
        let mut out = Vec::new();
        while let Some(item) = seq.next_element::<JsValue>()? {
            out.push(item);
        }
        Ok(JsValue::Array(out))
    }
    fn visit_map<A: serde::de::MapAccess<'de>>(self, mut map: A) -> Result<JsValue, A::Error> {
        let mut obj = JsObject::new();
        while let Some((k, v)) = map.next_entry::<String, JsValue>()? {
            obj.insert(k, v);
        }
        Ok(JsValue::Object(obj))
    }
}

/// `JSON.stringify(value)` (compact).
pub fn stringify(value: &JsValue) -> String {
    let mut out = String::new();
    write_value(&mut out, value);
    out
}

/// `JSON.stringify(value, null, indent)` — the pretty form every TS
/// `--stats-json` / stage-hashes / placement-stats writer uses (indent 2).
/// Empty arrays and objects stay `[]` / `{}`; entries are
/// `"key": value` on their own lines, like V8's JSON.stringify.
pub fn stringify_pretty(value: &JsValue, indent: usize) -> String {
    let mut out = String::new();
    write_pretty(&mut out, value, indent, 0);
    out
}

fn write_pretty(out: &mut String, value: &JsValue, indent: usize, depth: usize) {
    let pad = |out: &mut String, d: usize| {
        out.push('\n');
        out.extend(std::iter::repeat_n(' ', indent * d));
    };
    match value {
        JsValue::Array(items) if !items.is_empty() => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                pad(out, depth + 1);
                write_pretty(out, item, indent, depth + 1);
            }
            pad(out, depth);
            out.push(']');
        }
        JsValue::Object(obj) if !obj.entries.is_empty() => {
            out.push('{');
            for (i, (k, v)) in obj.entries.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                pad(out, depth + 1);
                write_string(out, k);
                out.push_str(": ");
                write_pretty(out, v, indent, depth + 1);
            }
            pad(out, depth);
            out.push('}');
        }
        other => write_value(out, other),
    }
}

fn write_value(out: &mut String, value: &JsValue) {
    match value {
        JsValue::Null => out.push_str("null"),
        JsValue::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        JsValue::Number(n) => {
            if n.is_finite() {
                out.push_str(&number_to_string(*n));
            } else {
                out.push_str("null");
            }
        }
        JsValue::String(s) => write_string(out, s),
        JsValue::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_value(out, item);
            }
            out.push(']');
        }
        JsValue::Object(obj) => {
            out.push('{');
            for (i, (k, v)) in obj.entries.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_string(out, k);
                out.push(':');
                write_value(out, v);
            }
            out.push('}');
        }
    }
}

fn write_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
#[path = "js_test.rs"]
mod js_test;
