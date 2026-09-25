//! JSON the way JavaScript writes it — the ONE owner of "what bytes would
//! `JSON.stringify` / the TS `canonicalJson` produce" (WP4.1).
//!
//! The LLM disk cache is keyed by sha256 over `canonicalJson(material)`
//! (src/llm/cached-provider.ts) and its entries are written with
//! `JSON.stringify`, so byte parity with the TS is the contract that lets a
//! Rust leg replay a TS-written cache (02 §7). serde_json cannot be used for
//! that directly: its maps sort by BYTES (JS sorts keys by UTF-16 code
//! units), it cannot reproduce JS's object enumeration order (array-index
//! keys first, numerically, then insertion order), and its float formatter
//! is not `Number.prototype.toString`. Every rule here is pinned by
//! test/parity/wp41-js-vectors.json, recorded from the real TS functions
//! (generator: test/parity/wp41-js-probe.ts).
//!
//! What JS does, and so what this does:
//! - an object enumerates its ARRAY-INDEX keys ("0".."4294967294", canonical
//!   decimal, no leading zero) first in ascending numeric order, then every
//!   other key in insertion order — whatever order they were written in;
//! - `canonicalJson` sorts keys with `Array.prototype.sort` (UTF-16 code
//!   units: U+FF5E sorts AFTER U+1F600, the reverse of byte order) and then
//!   re-inserts them, so index keys STILL come first;
//! - a duplicate key keeps its first position and its last value
//!   (`JSON.parse`, and assignment into a record);
//! - strings escape `"`, `\`, `\b \f \n \r \t`, other C0 controls as
//!   `\u00xx` (lowercase hex); DEL, U+2028/2029 and all non-ASCII are
//!   literal UTF-8;
//! - numbers print as `String(x)` (shortest round-trip digits, exponent
//!   form below 1e-6 and from 1e21, `-0` prints `0`); non-finite → `null`.

use std::cmp::Ordering;
use std::fmt::Write as _;

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

/// The numeric value of an array-index key, or None for an ordinary key.
/// An array index is a canonical decimal (no sign, no leading zero unless
/// it is "0") below 2^32 - 1.
pub fn array_index(key: &str) -> Option<u32> {
    let bytes = key.as_bytes();
    if bytes.is_empty() || bytes.len() > 10 || !bytes.iter().all(u8::is_ascii_digit) {
        return None;
    }
    if bytes.len() > 1 && bytes[0] == b'0' {
        return None;
    }
    let value: u64 = key.parse().ok()?;
    (value < u64::from(u32::MAX)).then_some(value as u32)
}

/// JS `Array.prototype.sort` default order on strings: UTF-16 code units.
pub fn cmp_utf16(a: &str, b: &str) -> Ordering {
    a.encode_utf16().cmp(b.encode_utf16())
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

    /// `Object.fromEntries(entries)` in one pass (a large Map's keys are
    /// unique by construction): index keys ahead, ascending; ordinary keys
    /// in insertion order. A repeated key falls back to [`JsObject::insert`]
    /// (the later value wins, the first position stays).
    pub fn from_entries(entries: Vec<(String, JsValue)>) -> Self {
        let mut seen = std::collections::HashSet::with_capacity(entries.len());
        if !entries.iter().all(|(k, _)| seen.insert(k.as_str())) {
            return entries.into_iter().collect();
        }
        let (mut index, ordinary): (Vec<_>, Vec<_>) = entries
            .into_iter()
            .partition(|(k, _)| array_index(k).is_some());
        index.sort_by_key(|(k, _)| array_index(k));
        index.extend(ordinary);
        JsObject { entries: index }
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

    /// `delete obj[key]`: the other entries keep their order (a later
    /// re-insert of the key appends, as in JS).
    pub fn remove(&mut self, key: &str) -> Option<JsValue> {
        let pos = self.entries.iter().position(|(k, _)| k == key)?;
        Some(self.entries.remove(pos).1)
    }

    pub fn entries(&self) -> &[(String, JsValue)] {
        &self.entries
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
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
    /// string but not in a Rust one: it becomes U+FFFD (never seen in the
    /// cache — every key and name there is ASCII, 2026-09-24).
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
    write_value(&mut out, value, false);
    out
}

/// The ASCII order of `String.prototype.localeCompare` (ICU root/en-US
/// collation, tertiary strength, non-ignorable punctuation), recorded from
/// Node 24 / ICU 78 (test/parity/wpb4-vectors.json `collation`). Letters
/// share a primary weight with their other case (`aA`); everything else is
/// its own primary.
const COLLATION_ASCII: &str = "\t\n\u{b}\u{c}\r _-,;:!?.'\"()[]{}@*/\\&#%`^+<=>|~$0123456789aAbBcCdDeEfFgGhHiIjJkKlLmMnNoOpPqQrRsStTuUvVwWxXyYzZ";

/// A character's collation element: `None` when completely ignorable
/// (the C0 controls outside TAB..CR, and DEL), else (primary, tertiary).
fn collation_element(c: char) -> Option<(u32, u32)> {
    if (c as u32) < 0x20 && !('\t'..='\r').contains(&c) || c == '\u{7f}' {
        return None;
    }
    if c.is_ascii() {
        let pos = COLLATION_ASCII
            .find(c)
            .expect("every printable ASCII char is ranked") as u32;
        return Some(if c.is_ascii_alphabetic() {
            // pairs share the lower-case slot; upper case is tertiary 1
            let base = COLLATION_ASCII.find(c.to_ascii_lowercase()).unwrap() as u32;
            (base, u32::from(c.is_ascii_uppercase()))
        } else {
            (pos, 0)
        });
    }
    // Outside ASCII: after every ASCII primary, by code point. NOT the ICU
    // order (accents are secondary differences there) — the gated corpus
    // is ASCII; a non-ASCII env-var name or path is a declared divergence.
    Some((0x1000 + c as u32, 0))
}

/// `a.localeCompare(b)` for the default locale (see [`COLLATION_ASCII`]):
/// all primaries first, then all tertiaries (lower case before upper),
/// then — only when the strings differ by ignorables alone — equal.
pub fn locale_compare(a: &str, b: &str) -> Ordering {
    let keys = |s: &str| -> Vec<(u32, u32)> { s.chars().filter_map(collation_element).collect() };
    let (ka, kb) = (keys(a), keys(b));
    let primary = |k: &[(u32, u32)]| k.iter().map(|e| e.0).collect::<Vec<_>>();
    let tertiary = |k: &[(u32, u32)]| k.iter().map(|e| e.1).collect::<Vec<_>>();
    primary(&ka)
        .cmp(&primary(&kb))
        .then_with(|| tertiary(&ka).cmp(&tertiary(&kb)))
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
        other => write_value(out, other, false),
    }
}

/// The TS `canonicalJson(value)`: recursively sorted keys (UTF-16 order),
/// re-inserted into an object — so array-index keys still lead.
pub fn canonical(value: &JsValue) -> String {
    let mut out = String::new();
    write_value(&mut out, value, true);
    out
}

fn write_value(out: &mut String, value: &JsValue, sorted: bool) {
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
                write_value(out, item, sorted);
            }
            out.push(']');
        }
        JsValue::Object(obj) => {
            let mut entries: Vec<&(String, JsValue)> = obj.entries.iter().collect();
            if sorted {
                // Sort, then re-insert: index keys come back to the front in
                // numeric order, ordinary keys keep the sort order.
                entries.sort_by(|a, b| cmp_utf16(&a.0, &b.0));
                let (mut index, ordinary): (Vec<_>, Vec<_>) = entries
                    .into_iter()
                    .partition(|(k, _)| array_index(k).is_some());
                index.sort_by_key(|(k, _)| array_index(k));
                entries = index.into_iter().chain(ordinary).collect();
            }
            out.push('{');
            for (i, (k, v)) in entries.into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_string(out, k);
                out.push(':');
                write_value(out, v, sorted);
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

/// The shortest round-trip decimal digits of a finite, nonzero |x| and its
/// decimal exponent `n` such that x = 0.d1d2... * 10^n (ECMA-262's k/n).
fn shortest_digits(x: f64) -> (String, i32) {
    // Rust's `{:e}` is the shortest round-trip representation: "d.ddde±x".
    let formatted = format!("{:e}", x.abs());
    let (mantissa, exp) = formatted.split_once('e').expect("LowerExp has an exponent");
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    let exp: i32 = exp.parse().expect("LowerExp exponent is an integer");
    (digits, exp + 1)
}

/// `Number.prototype.toString()` (ECMA-262 Number::toString, radix 10),
/// NaN and the infinities included.
pub fn number_to_string(x: f64) -> String {
    if x.is_nan() {
        return "NaN".to_string();
    }
    if x.is_infinite() {
        return if x > 0.0 { "Infinity" } else { "-Infinity" }.to_string();
    }
    if x == 0.0 {
        return "0".to_string(); // +0 and -0 alike
    }
    let sign = if x < 0.0 { "-" } else { "" };
    let (digits, n) = shortest_digits(x);
    let k = digits.len() as i32;
    let body = if k <= n && n <= 21 {
        format!("{digits}{}", "0".repeat((n - k) as usize))
    } else if 0 < n && n <= 21 {
        format!("{}.{}", &digits[..n as usize], &digits[n as usize..])
    } else if -6 < n && n <= 0 {
        format!("0.{}{digits}", "0".repeat((-n) as usize))
    } else {
        let e = n - 1;
        let exp = if e >= 0 {
            format!("+{e}")
        } else {
            e.to_string()
        };
        if k == 1 {
            format!("{digits}e{exp}")
        } else {
            format!("{}.{}e{exp}", &digits[..1], &digits[1..])
        }
    };
    format!("{sign}{body}")
}

/// `Math.log(x)` exactly as V8 computes it: fdlibm's `__ieee754_log`
/// (V8 `base::ieee754::log`), which is NOT always correctly rounded — so
/// Rust's `f64::ln` (the platform libm) can differ by an ulp, and a
/// float that feeds a sort or a tie (cluster-assign's IDF weights) must
/// come out bit-identical.
pub fn math_log(x: f64) -> f64 {
    // fdlibm's constants, by their documented bit patterns (e_log.c).
    const LN2_HI: f64 = f64::from_bits(0x3fe6_2e42_fee0_0000);
    const LN2_LO: f64 = f64::from_bits(0x3dea_39ef_3579_3c76);
    const TWO54: f64 = f64::from_bits(0x4350_0000_0000_0000);
    const LG1: f64 = f64::from_bits(0x3fe5_5555_5555_5593);
    const LG2: f64 = f64::from_bits(0x3fd9_9999_9997_fa04);
    const LG3: f64 = f64::from_bits(0x3fd2_4924_9422_9359);
    const LG4: f64 = f64::from_bits(0x3fcc_71c5_1d8e_78af);
    const LG5: f64 = f64::from_bits(0x3fc7_4664_96cb_03de);
    const LG6: f64 = f64::from_bits(0x3fc3_9a09_d078_c69f);
    const LG7: f64 = f64::from_bits(0x3fc2_f112_df3e_5244);
    // `0.33333333333333333` in the C source — the double nearest 1/3.
    const THIRD: f64 = f64::from_bits(0x3fd5_5555_5555_5555);
    let hi = |v: f64| (v.to_bits() >> 32) as i32;
    let lo = |v: f64| v.to_bits() as u32;
    let with_hi = |v: f64, h: i32| f64::from_bits((u64::from(h as u32) << 32) | u64::from(lo(v)));

    let mut x = x;
    let mut hx = hi(x);
    let lx = lo(x);
    let mut k: i32 = 0;
    if hx < 0x0010_0000 {
        if ((hx & 0x7fff_ffff) as u32 | lx) == 0 {
            return f64::NEG_INFINITY;
        }
        if hx < 0 {
            return f64::NAN;
        }
        k -= 54;
        x *= TWO54;
        hx = hi(x);
    }
    if hx >= 0x7ff0_0000 {
        return x + x;
    }
    k += (hx >> 20) - 1023;
    hx &= 0x000f_ffff;
    let i = (hx + 0x95f64) & 0x10_0000;
    x = with_hi(x, hx | (i ^ 0x3ff0_0000));
    k += i >> 20;
    let f = x - 1.0;
    let dk = f64::from(k);
    if (0x000f_ffff & (2 + hx)) < 3 {
        if f == 0.0 {
            return if k == 0 {
                0.0
            } else {
                dk * LN2_HI + dk * LN2_LO
            };
        }
        let r = f * f * (0.5 - THIRD * f);
        return if k == 0 {
            f - r
        } else {
            dk * LN2_HI - ((r - dk * LN2_LO) - f)
        };
    }
    let s = f / (2.0 + f);
    let z = s * s;
    let mut i = hx - 0x6147a;
    let w = z * z;
    let j = 0x6b851 - hx;
    let t1 = w * (LG2 + w * (LG4 + w * LG6));
    let t2 = z * (LG1 + w * (LG3 + w * (LG5 + w * LG7)));
    i |= j;
    let r = t2 + t1;
    if i > 0 {
        let hfsq = 0.5 * f * f;
        if k == 0 {
            f - (hfsq - s * (hfsq + r))
        } else {
            dk * LN2_HI - ((hfsq - (s * (hfsq + r) + dk * LN2_LO)) - f)
        }
    } else if k == 0 {
        f - s * (f - r)
    } else {
        dk * LN2_HI - ((s * (f - r) - dk * LN2_LO) - f)
    }
}

/// The value of a JS numeric literal from its SOURCE spelling (`raw`) —
/// what Babel's `NumericLiteral.value` holds. Read the raw text, never a
/// JSON-parsed `value`: serde_json's default float parser is up to 1 ulp
/// off (lesson 8; the 2.1.216 FLT_MAX literal `340282346638528860000…`
/// parsed one ulp high). Handles `_` separators, hex/octal/binary radix
/// prefixes, and sloppy-mode legacy octal (`017` = 15, `019` = 19).
/// `None` for a BigInt (`…n`) or anything that is not a numeric literal.
pub fn numeric_literal_value(raw: &str) -> Option<f64> {
    let s: String = raw.chars().filter(|c| *c != '_').collect();
    if s.ends_with('n') {
        return None;
    }
    let radix = |digits: &str, r: u32| u128::from_str_radix(digits, r).ok().map(|v| v as f64);
    let lower = s.to_ascii_lowercase();
    if let Some(d) = lower.strip_prefix("0x") {
        return radix(d, 16);
    }
    if let Some(d) = lower.strip_prefix("0o") {
        return radix(d, 8);
    }
    if let Some(d) = lower.strip_prefix("0b") {
        return radix(d, 2);
    }
    let legacy = s.len() > 1 && s.starts_with('0') && s.bytes().all(|b| b.is_ascii_digit());
    if legacy && s.bytes().all(|b| (b'0'..=b'7').contains(&b)) {
        return radix(&s, 8);
    }
    s.parse::<f64>().ok()
}

/// `Number.prototype.toFixed(digits)` for finite |x| < 1e21: the integer n
/// minimizing |n / 10^f - x| over the EXACT binary value of x, the larger n
/// on a tie (half-up for positives) — Rust's `{:.N}` ties to even, which
/// is why this exists (1.25.toFixed(1) is "1.3" in JS, "1.2" in Rust).
pub fn to_fixed(x: f64, digits: usize) -> String {
    if x < 0.0 {
        let inner = to_fixed(-x, digits);
        return if inner.chars().all(|c| c == '0' || c == '.') {
            inner // (-0.001).toFixed(1) is "-0.0" in JS; not reachable here
        } else {
            format!("-{inner}")
        };
    }
    // The exact decimal expansion of a double terminates within 1074
    // fractional digits; this prints it exactly (no rounding happens).
    let exact = format!("{x:.1100}");
    let (int_part, frac) = exact.split_once('.').expect("fixed has a point");
    let kept = &frac[..digits];
    let rest = &frac[digits..];
    let round_up = rest.as_bytes().first().is_some_and(|d| *d >= b'5');
    let mut number: Vec<u8> = format!("{int_part}{kept}").into_bytes();
    if round_up {
        let mut i = number.len();
        loop {
            if i == 0 {
                number.insert(0, b'1');
                break;
            }
            i -= 1;
            if number[i] == b'9' {
                number[i] = b'0';
            } else {
                number[i] += 1;
                break;
            }
        }
    }
    let text = String::from_utf8(number).expect("ascii digits");
    if digits == 0 {
        return text;
    }
    let split = text.len() - digits;
    format!("{}.{}", &text[..split], &text[split..])
}

/// TS `formatDuration` (src/llm/metrics.ts): 500ms / 5.0s / 2m 5s / 2h 5m,
/// with the TS's accidents — seconds are ROUNDED under FLOORED minutes, so
/// 59,999 ms prints "60.0s" and 3,599,999 ms prints "59m 60s". The one
/// owner: the LLM metrics and the profile summary both print through it.
pub fn format_duration(ms: f64) -> String {
    if ms < 1000.0 {
        return format!("{}ms", number_to_string(ms));
    }
    if ms < 60_000.0 {
        return format!("{}s", to_fixed(ms / 1000.0, 1));
    }
    let mins = (ms / 60_000.0).floor();
    let secs = math_round((ms % 60_000.0) / 1000.0);
    if mins < 60.0 {
        return format!("{}m {}s", number_to_string(mins), number_to_string(secs));
    }
    let hours = (mins / 60.0).floor();
    format!(
        "{}h {}m",
        number_to_string(hours),
        number_to_string(mins % 60.0)
    )
}

/// JS WhiteSpace + LineTerminator — the set `String.prototype.trim`
/// strips (ECMA-262 §12.2/§12.3: TAB VT FF SP NBSP ZWNBSP, category Zs,
/// LF CR LS PS). NOT `char::is_whitespace`: Unicode White_Space includes
/// U+0085 (JS keeps it) and excludes U+FEFF (JS strips it). Pinned against
/// every code point by wp42-vectors.json (WP4.2).
pub fn is_js_whitespace(c: char) -> bool {
    matches!(
        c,
        '\u{9}' | '\u{a}' | '\u{b}' | '\u{c}' | '\u{d}' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
    )
}

/// Node's `path.resolve(p)` (POSIX): absolute against the working
/// directory, then `.` and `..` normalized LEXICALLY — never through the
/// filesystem, and before any parent walk (`std::path::absolute` keeps
/// `..`, so a raw `parent()` walk from `a/x/../b` visits `a/x`).
pub fn node_path_resolve(p: &std::path::Path) -> std::path::PathBuf {
    use std::path::{Component, PathBuf};
    // `absolute` joins the working directory WITHOUT normalizing.
    let abs = std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf());
    let mut out = PathBuf::from("/");
    for c in abs.components() {
        match c {
            Component::Normal(s) => out.push(s),
            Component::ParentDir => {
                out.pop();
            }
            _ => {}
        }
    }
    out
}

/// `String.prototype.trim()`.
pub fn trim(s: &str) -> &str {
    s.trim_matches(is_js_whitespace)
}

/// A string's JS `.length`: UTF-16 code units.
pub fn utf16_len(s: &str) -> usize {
    s.chars().map(char::len_utf16).sum()
}

/// `s.slice(0, n)`: the first `n` UTF-16 code units. A cut inside a
/// surrogate pair would leave JS a lone high surrogate, which a Rust
/// `String` cannot hold — the prefix then stops before the pair (never
/// observed in the oracle texts; 07 OQ3).
pub fn utf16_prefix(s: &str, n: usize) -> &str {
    let mut units = 0usize;
    for (byte, ch) in s.char_indices() {
        let next = units + ch.len_utf16();
        if next > n {
            return &s[..byte];
        }
        units = next;
    }
    s
}

/// `Math.round(x)`: the closest integer, ties toward +infinity.
pub fn math_round(x: f64) -> f64 {
    let floor = x.floor();
    if x - floor >= 0.5 { floor + 1.0 } else { floor }
}
