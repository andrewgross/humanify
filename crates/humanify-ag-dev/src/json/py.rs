//! JSON the way the exp096 library scorer writes it: Python's
//! `json.dumps(x, sort_keys=True, indent=1, ensure_ascii=True)`
//! (lib096.py, "the one JSON spelling").
//!
//! What Python does, and so what this does:
//! - an int and a float are different values: `1` stays `1`, `1.0` stays
//!   `1.0` (an int is kept as its digits; Python ints are unbounded);
//! - a float prints as `repr(float)`: the shortest digits that round-trip,
//!   positional when the decimal exponent is in -5 < e < 16 (always with a
//!   fraction: `100.0`), else `d.ddde±XX` with at least two exponent
//!   digits (`1e-05`, `1.5e+16`); `NaN`, `Infinity`, `-Infinity`;
//! - keys sort by code point (Rust's `str` order is the same);
//! - `ensure_ascii`: `"` `\` and `\n \r \t \b \f` get short escapes, every
//!   other character outside space..`~` (DEL included) is `\uXXXX` in
//!   lowercase hex, a character beyond the BMP as its surrogate pair;
//! - `indent=1`: one space per level, `", "`-free item separator `,` at
//!   line end, `": "` after a key; an empty list or dict is `[]` / `{}`;
//! - a duplicate key keeps the LAST value (a `dict` built in order).

/// A JSON value with Python's int / float distinction.
#[derive(Clone, Debug, PartialEq)]
pub enum PyValue {
    Null,
    Bool(bool),
    /// An int, as its canonical decimal digits.
    Int(String),
    Float(f64),
    Str(String),
    List(Vec<PyValue>),
    /// Keys unique; written sorted.
    Dict(Vec<(String, PyValue)>),
}

impl PyValue {
    /// Parse JSON text as Python's `json.loads` does (it also accepts
    /// `NaN`, `Infinity` and `-Infinity`). A lone surrogate escape is
    /// refused: a Rust string cannot hold one.
    pub fn parse(text: &str) -> Result<PyValue, String> {
        let mut p = Parser {
            b: text.as_bytes(),
            pos: 0,
        };
        p.ws();
        let v = p.value()?;
        p.ws();
        if p.pos != p.b.len() {
            return Err(p.err("trailing text"));
        }
        Ok(v)
    }

    /// The value of `key` in a dict.
    pub fn get(&self, key: &str) -> Option<&PyValue> {
        match self {
            PyValue::Dict(entries) => entries.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
}

struct Parser<'a> {
    b: &'a [u8],
    pos: usize,
}

impl Parser<'_> {
    fn err(&self, what: &str) -> String {
        format!("JSON parse error at byte {}: {what}", self.pos)
    }

    fn ws(&mut self) {
        while self
            .b
            .get(self.pos)
            .is_some_and(|c| matches!(c, b' ' | b'\t' | b'\n' | b'\r'))
        {
            self.pos += 1;
        }
    }

    fn eat(&mut self, lit: &str) -> bool {
        if self.b[self.pos..].starts_with(lit.as_bytes()) {
            self.pos += lit.len();
            true
        } else {
            false
        }
    }

    fn value(&mut self) -> Result<PyValue, String> {
        match self.b.get(self.pos) {
            Some(b'{') => self.dict(),
            Some(b'[') => self.list(),
            Some(b'"') => self.string().map(PyValue::Str),
            Some(b't') if self.eat("true") => Ok(PyValue::Bool(true)),
            Some(b'f') if self.eat("false") => Ok(PyValue::Bool(false)),
            Some(b'n') if self.eat("null") => Ok(PyValue::Null),
            Some(b'N') if self.eat("NaN") => Ok(PyValue::Float(f64::NAN)),
            Some(b'I') if self.eat("Infinity") => Ok(PyValue::Float(f64::INFINITY)),
            Some(b'-') if self.eat("-Infinity") => Ok(PyValue::Float(f64::NEG_INFINITY)),
            Some(b'-' | b'0'..=b'9') => self.number(),
            _ => Err(self.err("unexpected character")),
        }
    }

    fn dict(&mut self) -> Result<PyValue, String> {
        self.pos += 1;
        let mut entries: Vec<(String, PyValue)> = Vec::new();
        self.ws();
        if self.eat("}") {
            return Ok(PyValue::Dict(entries));
        }
        loop {
            self.ws();
            if self.b.get(self.pos) != Some(&b'"') {
                return Err(self.err("expected a key"));
            }
            let key = self.string()?;
            self.ws();
            if !self.eat(":") {
                return Err(self.err("expected ':'"));
            }
            self.ws();
            let v = self.value()?;
            match entries.iter_mut().find(|(k, _)| *k == key) {
                Some(slot) => slot.1 = v,
                None => entries.push((key, v)),
            }
            self.ws();
            if self.eat(",") {
                continue;
            }
            if self.eat("}") {
                return Ok(PyValue::Dict(entries));
            }
            return Err(self.err("expected ',' or '}'"));
        }
    }

    fn list(&mut self) -> Result<PyValue, String> {
        self.pos += 1;
        let mut items = Vec::new();
        self.ws();
        if self.eat("]") {
            return Ok(PyValue::List(items));
        }
        loop {
            self.ws();
            items.push(self.value()?);
            self.ws();
            if self.eat(",") {
                continue;
            }
            if self.eat("]") {
                return Ok(PyValue::List(items));
            }
            return Err(self.err("expected ',' or ']'"));
        }
    }

    fn number(&mut self) -> Result<PyValue, String> {
        let start = self.pos;
        let mut float = false;
        while let Some(c) = self.b.get(self.pos) {
            match c {
                b'0'..=b'9' | b'-' | b'+' => {}
                b'.' | b'e' | b'E' => float = true,
                _ => break,
            }
            self.pos += 1;
        }
        let lit = std::str::from_utf8(&self.b[start..self.pos]).expect("ASCII");
        if float {
            lit.parse::<f64>()
                .map(PyValue::Float)
                .map_err(|_| self.err("bad number"))
        } else {
            let digits = lit.strip_prefix('-').unwrap_or(lit);
            if digits.is_empty()
                || !digits.bytes().all(|c| c.is_ascii_digit())
                || (digits.len() > 1 && digits.starts_with('0'))
            {
                return Err(self.err("bad integer"));
            }
            let canonical = if digits == "0" { "0" } else { lit };
            Ok(PyValue::Int(canonical.to_string()))
        }
    }

    fn hex4(&mut self) -> Result<u32, String> {
        let h = self
            .b
            .get(self.pos..self.pos + 4)
            .and_then(|s| std::str::from_utf8(s).ok())
            .and_then(|s| u32::from_str_radix(s, 16).ok())
            .ok_or_else(|| self.err("bad \\u escape"))?;
        self.pos += 4;
        Ok(h)
    }

    fn string(&mut self) -> Result<String, String> {
        self.pos += 1;
        let mut out = String::new();
        loop {
            let start = self.pos;
            while self
                .b
                .get(self.pos)
                .is_some_and(|c| *c != b'"' && *c != b'\\' && *c >= 0x20)
            {
                self.pos += 1;
            }
            out.push_str(
                std::str::from_utf8(&self.b[start..self.pos]).map_err(|_| self.err("not UTF-8"))?,
            );
            match self.b.get(self.pos) {
                Some(b'"') => {
                    self.pos += 1;
                    return Ok(out);
                }
                Some(b'\\') => {
                    self.pos += 1;
                    self.escape(&mut out)?;
                }
                _ => return Err(self.err("unterminated string")),
            }
        }
    }

    fn escape(&mut self, out: &mut String) -> Result<(), String> {
        let c = *self.b.get(self.pos).ok_or_else(|| self.err("bad escape"))?;
        self.pos += 1;
        let ch = match c {
            b'"' => '"',
            b'\\' => '\\',
            b'/' => '/',
            b'b' => '\u{8}',
            b'f' => '\u{c}',
            b'n' => '\n',
            b'r' => '\r',
            b't' => '\t',
            b'u' => {
                let hi = self.hex4()?;
                let code = if (0xD800..0xDC00).contains(&hi) && self.eat("\\u") {
                    let lo = self.hex4()?;
                    if !(0xDC00..0xE000).contains(&lo) {
                        return Err(self.err("lone surrogate"));
                    }
                    0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00)
                } else {
                    hi
                };
                char::from_u32(code).ok_or_else(|| self.err("lone surrogate"))?
            }
            _ => return Err(self.err("bad escape")),
        };
        out.push(ch);
        Ok(())
    }
}

/// `repr(x)` for a Python float.
pub fn float_repr(x: f64) -> String {
    if x.is_nan() {
        return "NaN".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "Infinity" } else { "-Infinity" }.into();
    }
    let sign = if x.is_sign_negative() { "-" } else { "" };
    if x == 0.0 {
        return format!("{sign}0.0");
    }
    // Rust's `{:e}` with no precision is the shortest round-trip digits.
    let sci = format!("{:e}", x.abs());
    let (mantissa, exp) = sci.split_once('e').expect("{:e} has an exponent");
    let exp: i32 = exp.parse().expect("an integer exponent");
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    if !(-5 < exp && exp < 16) {
        let m = if digits.len() > 1 {
            format!("{}.{}", &digits[..1], &digits[1..])
        } else {
            digits
        };
        let es = if exp < 0 { '-' } else { '+' };
        return format!("{sign}{m}e{es}{:02}", exp.abs());
    }
    if exp >= 0 {
        let point = exp as usize + 1;
        if digits.len() <= point {
            format!("{sign}{digits}{}.0", "0".repeat(point - digits.len()))
        } else {
            format!("{sign}{}.{}", &digits[..point], &digits[point..])
        }
    } else {
        format!("{sign}0.{}{digits}", "0".repeat((-exp - 1) as usize))
    }
}

/// A string as `ensure_ascii` writes it, quotes included.
pub fn write_str(out: &mut String, s: &str) {
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            ' '..='~' => out.push(ch),
            _ => {
                let mut buf = [0u16; 2];
                for unit in ch.encode_utf16(&mut buf) {
                    out.push_str(&format!("\\u{unit:04x}"));
                }
            }
        }
    }
    out.push('"');
}

/// `json.dumps(value, sort_keys=True, indent=1, ensure_ascii=True)`.
pub fn dumps(value: &PyValue) -> String {
    let mut out = String::new();
    write(&mut out, value, 0);
    out
}

fn newline(out: &mut String, depth: usize) {
    out.push('\n');
    out.extend(std::iter::repeat_n(' ', depth));
}

fn write(out: &mut String, value: &PyValue, depth: usize) {
    match value {
        PyValue::Null => out.push_str("null"),
        PyValue::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        PyValue::Int(digits) => out.push_str(digits),
        PyValue::Float(x) => out.push_str(&float_repr(*x)),
        PyValue::Str(s) => write_str(out, s),
        PyValue::List(items) if items.is_empty() => out.push_str("[]"),
        PyValue::List(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                newline(out, depth + 1);
                write(out, item, depth + 1);
            }
            newline(out, depth);
            out.push(']');
        }
        PyValue::Dict(entries) if entries.is_empty() => out.push_str("{}"),
        PyValue::Dict(entries) => {
            let mut sorted: Vec<&(String, PyValue)> = entries.iter().collect();
            sorted.sort_by(|a, b| a.0.cmp(&b.0));
            out.push('{');
            for (i, (k, v)) in sorted.into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                newline(out, depth + 1);
                write_str(out, k);
                out.push_str(": ");
                write(out, v, depth + 1);
            }
            newline(out, depth);
            out.push('}');
        }
    }
}
