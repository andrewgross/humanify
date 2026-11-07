//! The line tokenizer `eval diff`'s real-column ledger pairs lines with
//! (TS: `experiments/lib/js/line-diff.ts` `tokenizeLine`): one line of
//! formatter output into identifier and opaque text tokens; None when the
//! line is not self-contained (an open string, template or comment) —
//! callers treat such a line as genuinely edited.

use humanify_ag_model::js::is_js_whitespace;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineToken {
    pub ident: bool,
    pub text: String,
}

/// Words kept verbatim, so a keyword change reads as a genuine change.
const RESERVED: &[&str] = &[
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "debugger",
    "default",
    "delete",
    "do",
    "else",
    "enum",
    "export",
    "extends",
    "false",
    "finally",
    "for",
    "function",
    "if",
    "import",
    "in",
    "instanceof",
    "new",
    "null",
    "return",
    "super",
    "switch",
    "this",
    "throw",
    "true",
    "try",
    "typeof",
    "var",
    "void",
    "while",
    "with",
    "yield",
    "let",
    "static",
    "async",
    "await",
    "of",
    "get",
    "set",
];

/// Tokens after which `/` is division.
const DIVISION_PRECEDERS: &[&str] = &[")", "]", "this", "true", "false", "null", "super"];

fn ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_' || c == '$'
}
fn ident_cont(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '$'
}
fn number_cont(c: char) -> bool {
    ident_cont(c) || c == '.'
}

#[derive(Clone, Copy)]
enum Frame {
    Quasi,
    Expr(u32),
}

struct St {
    line: Vec<char>,
    i: usize,
    tokens: Vec<LineToken>,
    frames: Vec<Frame>,
    prev: Option<usize>,
    failed: bool,
}

impl St {
    fn at(&self, i: usize) -> Option<char> {
        self.line.get(i).copied()
    }

    fn emit(&mut self, ident: bool, start: usize) {
        let text: String = self.line[start..self.i.min(self.line.len())]
            .iter()
            .collect();
        let blank = text.chars().all(is_js_whitespace);
        self.tokens.push(LineToken { ident, text });
        if !blank {
            self.prev = Some(self.tokens.len() - 1);
        }
    }

    fn division_context(&self) -> bool {
        let Some(p) = self.prev.map(|i| &self.tokens[i]) else {
            return false;
        };
        p.ident
            || DIVISION_PRECEDERS.contains(&p.text.as_str())
            || p.text.starts_with(|c: char| c.is_ascii_digit())
    }

    fn simple_string(&mut self, quote: char) {
        let start = self.i;
        self.i += 1;
        while let Some(c) = self.at(self.i) {
            if c == '\\' {
                self.i += 2;
                continue;
            }
            self.i += 1;
            if c == quote {
                self.emit(false, start);
                return;
            }
        }
        self.failed = true;
    }

    fn quasi(&mut self) {
        let start = self.i;
        while let Some(c) = self.at(self.i) {
            if c == '\\' {
                self.i += 2;
                continue;
            }
            if c == '`' {
                self.i += 1;
                self.emit(false, start);
                self.frames.pop();
                return;
            }
            if c == '$' && self.at(self.i + 1) == Some('{') {
                self.i += 2;
                self.emit(false, start);
                self.frames.push(Frame::Expr(0));
                return;
            }
            self.i += 1;
        }
        self.failed = true;
    }

    fn slash(&mut self) {
        let start = self.i;
        match self.at(self.i + 1) {
            Some('/') => {
                self.i = self.line.len();
                self.emit(false, start);
            }
            Some('*') => {
                let mut j = self.i + 2;
                while j + 1 < self.line.len() && !(self.line[j] == '*' && self.line[j + 1] == '/') {
                    j += 1;
                }
                if j + 1 >= self.line.len() {
                    self.failed = true;
                    return;
                }
                self.i = j + 2;
                self.emit(false, start);
            }
            _ if self.division_context() => {
                self.i += 1;
                self.emit(false, start);
            }
            _ => self.regex(),
        }
    }

    fn regex(&mut self) {
        let start = self.i;
        self.i += 1;
        let mut in_class = false;
        while let Some(c) = self.at(self.i) {
            match c {
                '\\' => {
                    self.i += 2;
                    continue;
                }
                '[' => in_class = true,
                ']' => in_class = false,
                '/' if !in_class => {
                    self.i += 1;
                    while self.at(self.i).is_some_and(ident_cont) {
                        self.i += 1;
                    }
                    self.emit(false, start);
                    return;
                }
                _ => {}
            }
            self.i += 1;
        }
        self.failed = true;
    }

    fn run_of(&mut self, ident: Option<bool>, cont: fn(char) -> bool) {
        let start = self.i;
        self.i += 1;
        while self.at(self.i).is_some_and(cont) {
            self.i += 1;
        }
        let is_ident = match ident {
            Some(v) => v,
            None => {
                let word: String = self.line[start..self.i].iter().collect();
                !RESERVED.contains(&word.as_str())
            }
        };
        self.emit(is_ident, start);
    }

    fn brace(&mut self, c: char) {
        if let Some(Frame::Expr(depth)) = self.frames.last().copied() {
            let n = self.frames.len() - 1;
            if c == '{' {
                self.frames[n] = Frame::Expr(depth + 1);
            } else if depth == 0 {
                self.frames.pop();
            } else {
                self.frames[n] = Frame::Expr(depth - 1);
            }
        }
        let start = self.i;
        self.i += 1;
        self.emit(false, start);
    }

    fn step(&mut self) {
        let c = self.line[self.i];
        match c {
            '"' | '\'' => self.simple_string(c),
            '`' => {
                let start = self.i;
                self.i += 1;
                self.emit(false, start);
                self.frames.push(Frame::Quasi);
            }
            '/' => self.slash(),
            '{' | '}' => self.brace(c),
            c if ident_start(c) => self.run_of(None, ident_cont),
            c if c.is_ascii_digit() => self.run_of(Some(false), number_cont),
            c if is_js_whitespace(c) => {
                let start = self.i;
                while self.at(self.i).is_some_and(is_js_whitespace) {
                    self.i += 1;
                }
                self.emit(false, start);
            }
            _ => {
                let start = self.i;
                self.i += 1;
                self.emit(false, start);
            }
        }
    }
}

/// Tokenize one line; None when it is not self-contained.
pub fn tokenize_line(line: &str) -> Option<Vec<LineToken>> {
    let mut st = St {
        line: line.chars().collect(),
        i: 0,
        tokens: Vec::new(),
        frames: Vec::new(),
        prev: None,
        failed: false,
    };
    while st.i < st.line.len() && !st.failed {
        if matches!(st.frames.last(), Some(Frame::Quasi)) {
            st.quasi();
        } else {
            st.step();
        }
    }
    (!st.failed && st.frames.is_empty()).then_some(st.tokens)
}

/// The two lines differ ONLY in identifier tokens (and do differ).
pub fn name_only(a: &str, b: &str) -> bool {
    let (Some(ta), Some(tb)) = (tokenize_line(a), tokenize_line(b)) else {
        return false;
    };
    if ta.len() != tb.len() {
        return false;
    }
    let mut saw = false;
    for (x, y) in ta.iter().zip(&tb) {
        if x.ident != y.ident {
            return false;
        }
        if x.text == y.text {
            continue;
        }
        if !x.ident {
            return false;
        }
        saw = true;
    }
    saw
}

#[cfg(test)]
#[path = "line_tokens_test.rs"]
mod line_tokens_test;
