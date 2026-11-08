//! acorn 8.19.0's TOKENIZER (`acorn.tokenizer(text, {ecmaVersion:
//! "latest", sourceType: "script", allowHashBang, ...})`), the one
//! `tokens14.mjs` runs for scorer 14's map-defect layer: the same token
//! stream and the same error position, over the same UTF-16 code units.
//! Tokenizer-only mode guesses regex-vs-division from acorn's token
//! context stack (`tokencontext.js`), not from a parse; that guess is
//! ported as it is, mistakes included, because the frozen layer was built
//! from its output.
//!
//! Only what the layer reads is kept per token: its type and, for strings,
//! templates, names and keywords, its value (cooked, as UTF-16).
//!
//! Regular-expression PATTERN validation (acorn's `regexp.js`) is
//! delegated to `oxc_regular_expression` (the same ECMAScript grammar,
//! Annex B included); flags are validated here exactly as acorn does.
//! Proof (b') and the token-stream differential (`tokens14` verb) are
//! what show the two accept the same patterns on the corpora's inputs.

use super::acorn_tables::{ASTRAL_CHAR, ASTRAL_START, BMP_CHAR, BMP_START};

/// A token type (`tokentype.js`); keywords by name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tt {
    Num,
    Regexp,
    String,
    Name,
    PrivateId,
    Eof,
    BracketL,
    BracketR,
    BraceL,
    BraceR,
    ParenL,
    ParenR,
    Comma,
    Semi,
    Colon,
    Dot,
    Question,
    QuestionDot,
    Arrow,
    Template,
    InvalidTemplate,
    Ellipsis,
    BackQuote,
    DollarBraceL,
    Eq,
    Assign,
    IncDec,
    Prefix,
    Binop,
    PlusMin,
    Star,
    Slash,
    Starstar,
    Kw(Kw),
}

/// The ES2015+ keywords (`keywords$1[6]`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kw {
    Break,
    Case,
    Catch,
    Continue,
    Debugger,
    Default,
    Do,
    Else,
    Finally,
    For,
    Function,
    If,
    Return,
    Switch,
    Throw,
    Try,
    Var,
    Const,
    While,
    With,
    New,
    This,
    Super,
    Class,
    Extends,
    Export,
    Import,
    Null,
    True,
    False,
    In,
    Instanceof,
    Typeof,
    Void,
    Delete,
}

fn keyword(word: &[u16]) -> Option<Kw> {
    let w = String::from_utf16(word).ok()?;
    Some(match w.as_str() {
        "break" => Kw::Break,
        "case" => Kw::Case,
        "catch" => Kw::Catch,
        "continue" => Kw::Continue,
        "debugger" => Kw::Debugger,
        "default" => Kw::Default,
        "do" => Kw::Do,
        "else" => Kw::Else,
        "finally" => Kw::Finally,
        "for" => Kw::For,
        "function" => Kw::Function,
        "if" => Kw::If,
        "return" => Kw::Return,
        "switch" => Kw::Switch,
        "throw" => Kw::Throw,
        "try" => Kw::Try,
        "var" => Kw::Var,
        "const" => Kw::Const,
        "while" => Kw::While,
        "with" => Kw::With,
        "new" => Kw::New,
        "this" => Kw::This,
        "super" => Kw::Super,
        "class" => Kw::Class,
        "extends" => Kw::Extends,
        "export" => Kw::Export,
        "import" => Kw::Import,
        "null" => Kw::Null,
        "true" => Kw::True,
        "false" => Kw::False,
        "in" => Kw::In,
        "instanceof" => Kw::Instanceof,
        "typeof" => Kw::Typeof,
        "void" => Kw::Void,
        "delete" => Kw::Delete,
        _ => return None,
    })
}

impl Tt {
    fn before_expr(self) -> bool {
        match self {
            Tt::BracketL
            | Tt::BraceL
            | Tt::ParenL
            | Tt::Comma
            | Tt::Semi
            | Tt::Colon
            | Tt::Question
            | Tt::Arrow
            | Tt::Ellipsis
            | Tt::DollarBraceL
            | Tt::Eq
            | Tt::Assign
            | Tt::Prefix
            | Tt::Binop
            | Tt::PlusMin
            | Tt::Star
            | Tt::Slash
            | Tt::Starstar => true,
            Tt::Kw(k) => matches!(
                k,
                Kw::Case
                    | Kw::Default
                    | Kw::Do
                    | Kw::Else
                    | Kw::Return
                    | Kw::Throw
                    | Kw::New
                    | Kw::Extends
                    | Kw::In
                    | Kw::Instanceof
                    | Kw::Typeof
                    | Kw::Void
                    | Kw::Delete
            ),
            _ => false,
        }
    }
}

/// A token context (`tokencontext.js` `types`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ctx {
    BStat,
    BExpr,
    BTmpl,
    PStat,
    PExpr,
    QTmpl,
    FStat,
    FExpr,
    FExprGen,
    FGen,
}

impl Ctx {
    fn is_expr(self) -> bool {
        matches!(
            self,
            Ctx::BExpr | Ctx::PExpr | Ctx::QTmpl | Ctx::FExpr | Ctx::FExprGen
        )
    }
    fn is_function(self) -> bool {
        matches!(self, Ctx::FStat | Ctx::FExpr | Ctx::FExprGen | Ctx::FGen)
    }
    fn generator(self) -> bool {
        matches!(self, Ctx::FExprGen | Ctx::FGen)
    }
}

/// One token: its type and (strings, templates, names, keywords) value.
#[derive(Clone, Debug)]
pub struct Tok {
    pub tt: Tt,
    pub value: Option<Vec<u16>>,
}

enum Fail {
    /// An acorn `SyntaxError` at this position.
    At(usize),
    /// `INVALID_TEMPLATE_ESCAPE_ERROR`.
    BadTemplateEscape,
}

type R<T> = Result<T, Fail>;

fn in_ranges(t: &[(u32, u32)], c: u32) -> bool {
    let i = t.partition_point(|&(_, hi)| hi < c);
    i < t.len() && t[i].0 <= c
}

fn in_astral(code: u32, set: &[u32]) -> bool {
    let mut pos = 0x10000;
    let mut i = 0;
    while i < set.len() {
        pos += set[i];
        if pos > code {
            return false;
        }
        pos += set[i + 1];
        if pos >= code {
            return true;
        }
        i += 2;
    }
    false
}

/// `isIdentifierStart(code, true)`.
pub fn is_id_start(code: u32) -> bool {
    match code {
        _ if code < 65 => code == 36,
        _ if code < 91 => true,
        _ if code < 97 => code == 95,
        _ if code < 123 => true,
        _ if code <= 0xffff => code >= 0xaa && in_ranges(BMP_START, code),
        _ => in_astral(code, ASTRAL_START),
    }
}

/// `isIdentifierChar(code, true)`.
pub fn is_id_char(code: u32) -> bool {
    match code {
        _ if code < 48 => code == 36,
        _ if code < 58 => true,
        _ if code < 65 => false,
        _ if code < 91 => true,
        _ if code < 97 => code == 95,
        _ if code < 123 => true,
        _ if code <= 0xffff => code >= 0xaa && in_ranges(BMP_CHAR, code),
        _ => in_astral(code, ASTRAL_START) || in_astral(code, ASTRAL_CHAR),
    }
}

fn is_newline(c: u32) -> bool {
    c == 10 || c == 13 || c == 0x2028 || c == 0x2029
}

/// JavaScript's `\s`.
pub fn is_js_space(c: u32) -> bool {
    matches!(
        c,
        9..=13 | 32 | 0xa0 | 0x1680 | 0x2000..=0x200a | 0x2028 | 0x2029 | 0x202f | 0x205f | 0x3000 | 0xfeff
    )
}

/// `codePointToString`.
fn push_code_point(out: &mut Vec<u16>, code: u32) {
    if code <= 0xffff {
        out.push(code as u16);
    } else {
        let c = code - 0x10000;
        out.push(((c >> 10) + 0xd800) as u16);
        out.push(((c & 1023) + 0xdc00) as u16);
    }
}

struct Lexer<'a> {
    input: &'a [u16],
    pos: usize,
    start: usize,
    end: usize,
    last_tok_end: usize,
    tt: Tt,
    value: Option<Vec<u16>>,
    context: Vec<Ctx>,
    expr_allowed: bool,
    strict: bool,
    contains_esc: bool,
    in_template_element: bool,
}

impl<'a> Lexer<'a> {
    /// The code unit at `i`, or NaN (here: None) past the end.
    fn at(&self, i: usize) -> Option<u32> {
        self.input.get(i).map(|&c| c as u32)
    }

    fn code(&self, i: usize) -> u32 {
        // JS `charCodeAt` past the end is NaN: no comparison holds.
        self.at(i).unwrap_or(u32::MAX)
    }

    fn full_char_code_at(&self, pos: usize) -> u32 {
        let code = self.code(pos);
        if code <= 0xd7ff || code >= 0xdc00 {
            return code;
        }
        let next = self.code(pos + 1);
        if next <= 0xdbff || next >= 0xe000 {
            code
        } else {
            (code << 10) + next - 0x35fdc00
        }
    }

    fn line_break_in(&self, a: usize, b: usize) -> bool {
        self.input[a.min(b)..b]
            .iter()
            .any(|&c| is_newline(c as u32))
    }

    fn cur_context(&self) -> Ctx {
        *self
            .context
            .last()
            .expect("the context stack is never empty")
    }

    // ------------------------------------------------------------ space

    fn skip_block_comment(&mut self) -> R<()> {
        let from = self.pos + 2;
        let end = (from..self.input.len().saturating_sub(1))
            .find(|&i| self.input[i] == b'*' as u16 && self.input[i + 1] == b'/' as u16);
        self.pos = from;
        match end {
            None => Err(Fail::At(self.pos - 2)),
            Some(e) => {
                self.pos = e + 2;
                Ok(())
            }
        }
    }

    fn skip_line_comment(&mut self, start_skip: usize) {
        self.pos += start_skip;
        let mut ch = self.code(self.pos);
        while self.pos < self.input.len() && !is_newline(ch) {
            self.pos += 1;
            ch = self.code(self.pos);
        }
    }

    fn skip_space(&mut self) -> R<()> {
        while self.pos < self.input.len() {
            let ch = self.code(self.pos);
            match ch {
                32 | 160 => self.pos += 1,
                13 => {
                    if self.code(self.pos + 1) == 10 {
                        self.pos += 1;
                    }
                    self.pos += 1;
                }
                10 | 8232 | 8233 => self.pos += 1,
                47 => match self.code(self.pos + 1) {
                    42 => self.skip_block_comment()?,
                    47 => self.skip_line_comment(2),
                    _ => break,
                },
                _ => {
                    let ws = matches!(
                        ch,
                        0x1680 | 0x2000..=0x200a | 0x202f | 0x205f | 0x3000 | 0xfeff
                    );
                    if (ch > 8 && ch < 14) || (ch >= 5760 && ws) {
                        self.pos += 1;
                    } else {
                        break;
                    }
                }
            }
        }
        Ok(())
    }

    // ------------------------------------------------------------ context

    fn brace_is_block(&self, prev: Tt) -> bool {
        let parent = self.cur_context();
        if parent == Ctx::FExpr || parent == Ctx::FStat {
            return true;
        }
        if prev == Tt::Colon && (parent == Ctx::BStat || parent == Ctx::BExpr) {
            return !parent.is_expr();
        }
        if prev == Tt::Kw(Kw::Return) || (prev == Tt::Name && self.expr_allowed) {
            return self.line_break_in(self.last_tok_end, self.start);
        }
        if matches!(
            prev,
            Tt::Kw(Kw::Else) | Tt::Semi | Tt::Eof | Tt::ParenR | Tt::Arrow
        ) {
            return true;
        }
        if prev == Tt::BraceL {
            return parent == Ctx::BStat;
        }
        if matches!(prev, Tt::Kw(Kw::Var) | Tt::Kw(Kw::Const) | Tt::Name) {
            return false;
        }
        !self.expr_allowed
    }

    fn in_generator_context(&self) -> bool {
        for i in (1..self.context.len()).rev() {
            let c = self.context[i];
            if c.is_function() {
                return c.generator();
            }
        }
        false
    }

    fn update_context(&mut self, prev: Tt) {
        let tt = self.tt;
        if matches!(tt, Tt::Kw(_)) && prev == Tt::Dot {
            self.expr_allowed = false;
            return;
        }
        match tt {
            Tt::ParenR | Tt::BraceR => {
                if self.context.len() == 1 {
                    self.expr_allowed = true;
                    return;
                }
                let mut out = self.context.pop().expect("non-empty");
                if out == Ctx::BStat && self.cur_context().is_function() {
                    out = self.context.pop().expect("non-empty");
                }
                self.expr_allowed = !out.is_expr();
            }
            Tt::BraceL => {
                let c = if self.brace_is_block(prev) {
                    Ctx::BStat
                } else {
                    Ctx::BExpr
                };
                self.context.push(c);
                self.expr_allowed = true;
            }
            Tt::DollarBraceL => {
                self.context.push(Ctx::BTmpl);
                self.expr_allowed = true;
            }
            Tt::ParenL => {
                let stat = matches!(
                    prev,
                    Tt::Kw(Kw::If) | Tt::Kw(Kw::For) | Tt::Kw(Kw::With) | Tt::Kw(Kw::While)
                );
                self.context
                    .push(if stat { Ctx::PStat } else { Ctx::PExpr });
                self.expr_allowed = true;
            }
            Tt::IncDec => {}
            Tt::Kw(Kw::Function) | Tt::Kw(Kw::Class) => {
                let cur = self.cur_context();
                if prev.before_expr()
                    && prev != Tt::Kw(Kw::Else)
                    && !(prev == Tt::Semi && cur != Ctx::PStat)
                    && !(prev == Tt::Kw(Kw::Return)
                        && self.line_break_in(self.last_tok_end, self.start))
                    && !((prev == Tt::Colon || prev == Tt::BraceL) && cur == Ctx::BStat)
                {
                    self.context.push(Ctx::FExpr);
                } else {
                    self.context.push(Ctx::FStat);
                }
                self.expr_allowed = false;
            }
            Tt::Colon => {
                if self.cur_context().is_function() {
                    self.context.pop();
                }
                self.expr_allowed = true;
            }
            Tt::BackQuote => {
                if self.cur_context() == Ctx::QTmpl {
                    self.context.pop();
                } else {
                    self.context.push(Ctx::QTmpl);
                }
                self.expr_allowed = false;
            }
            Tt::Star => {
                if prev == Tt::Kw(Kw::Function) {
                    let i = self.context.len() - 1;
                    self.context[i] = if self.context[i] == Ctx::FExpr {
                        Ctx::FExprGen
                    } else {
                        Ctx::FGen
                    };
                }
                self.expr_allowed = true;
            }
            Tt::Name => {
                let mut allowed = false;
                if prev != Tt::Dot {
                    let v = self.value.as_deref().unwrap_or(&[]);
                    let is = |w: &str| v.iter().copied().eq(w.encode_utf16());
                    if (is("of") && !self.expr_allowed)
                        || (is("yield") && self.in_generator_context())
                    {
                        allowed = true;
                    }
                }
                self.expr_allowed = allowed;
            }
            other => self.expr_allowed = other.before_expr(),
        }
    }

    fn finish(&mut self, tt: Tt, value: Option<Vec<u16>>) -> R<()> {
        self.end = self.pos;
        let prev = self.tt;
        self.tt = tt;
        self.value = value;
        self.update_context(prev);
        Ok(())
    }

    fn finish_op(&mut self, tt: Tt, size: usize) -> R<()> {
        self.pos += size;
        self.finish(tt, None)
    }

    // ------------------------------------------------------------ tokens

    /// `next()` + `getToken()`.
    fn next(&mut self) -> R<()> {
        if matches!(self.tt, Tt::Kw(_)) && self.contains_esc {
            return Err(Fail::At(self.start));
        }
        self.last_tok_end = self.end;
        self.next_token()
    }

    fn next_token(&mut self) -> R<()> {
        let cur = self.cur_context();
        if cur != Ctx::QTmpl {
            self.skip_space()?;
        }
        self.start = self.pos;
        if self.pos >= self.input.len() {
            return self.finish(Tt::Eof, None);
        }
        if cur == Ctx::QTmpl {
            return self.try_read_template_token();
        }
        let code = self.full_char_code_at(self.pos);
        if is_id_start(code) || code == 92 {
            return self.read_word();
        }
        self.get_token_from_code(code)
    }

    fn get_token_from_code(&mut self, code: u32) -> R<()> {
        match code {
            46 => self.read_token_dot(),
            40 => self.finish_op(Tt::ParenL, 1),
            41 => self.finish_op(Tt::ParenR, 1),
            59 => self.finish_op(Tt::Semi, 1),
            44 => self.finish_op(Tt::Comma, 1),
            91 => self.finish_op(Tt::BracketL, 1),
            93 => self.finish_op(Tt::BracketR, 1),
            123 => self.finish_op(Tt::BraceL, 1),
            125 => self.finish_op(Tt::BraceR, 1),
            58 => self.finish_op(Tt::Colon, 1),
            96 => self.finish_op(Tt::BackQuote, 1),
            48 => {
                let next = self.code(self.pos + 1);
                match next {
                    120 | 88 => self.read_radix_number(16),
                    111 | 79 => self.read_radix_number(8),
                    98 | 66 => self.read_radix_number(2),
                    _ => self.read_number(false),
                }
            }
            49..=57 => self.read_number(false),
            34 | 39 => self.read_string(code as u16),
            47 => {
                let next = self.code(self.pos + 1);
                if self.expr_allowed {
                    self.pos += 1;
                    return self.read_regexp();
                }
                if next == 61 {
                    return self.finish_op(Tt::Assign, 2);
                }
                self.finish_op(Tt::Slash, 1)
            }
            37 | 42 => {
                let mut next = self.code(self.pos + 1);
                let mut size = 1;
                let mut tt = if code == 42 { Tt::Star } else { Tt::Binop };
                if code == 42 && next == 42 {
                    size += 1;
                    tt = Tt::Starstar;
                    next = self.code(self.pos + 2);
                }
                if next == 61 {
                    return self.finish_op(Tt::Assign, size + 1);
                }
                self.finish_op(tt, size)
            }
            124 | 38 => {
                let next = self.code(self.pos + 1);
                if next == code {
                    if self.code(self.pos + 2) == 61 {
                        return self.finish_op(Tt::Assign, 3);
                    }
                    return self.finish_op(Tt::Binop, 2);
                }
                if next == 61 {
                    return self.finish_op(Tt::Assign, 2);
                }
                self.finish_op(Tt::Binop, 1)
            }
            94 => {
                if self.code(self.pos + 1) == 61 {
                    return self.finish_op(Tt::Assign, 2);
                }
                self.finish_op(Tt::Binop, 1)
            }
            43 | 45 => self.read_token_plus_min(code),
            60 | 62 => self.read_token_lt_gt(code),
            61 | 33 => {
                let next = self.code(self.pos + 1);
                if next == 61 {
                    let size = if self.code(self.pos + 2) == 61 { 3 } else { 2 };
                    return self.finish_op(Tt::Binop, size);
                }
                if code == 61 && next == 62 {
                    return self.finish_op(Tt::Arrow, 2);
                }
                self.finish_op(if code == 61 { Tt::Eq } else { Tt::Prefix }, 1)
            }
            63 => {
                let next = self.code(self.pos + 1);
                if next == 46 {
                    // `next2 < 48 || next2 > 57`: false for NaN (the end)
                    if self
                        .at(self.pos + 2)
                        .is_some_and(|n| !(48..=57).contains(&n))
                    {
                        return self.finish_op(Tt::QuestionDot, 2);
                    }
                }
                if next == 63 {
                    if self.code(self.pos + 2) == 61 {
                        return self.finish_op(Tt::Assign, 3);
                    }
                    return self.finish_op(Tt::Binop, 2);
                }
                self.finish_op(Tt::Question, 1)
            }
            126 => self.finish_op(Tt::Prefix, 1),
            35 => {
                self.pos += 1;
                let c = self.full_char_code_at(self.pos);
                if is_id_start(c) || c == 92 {
                    let w = self.read_word1()?;
                    return self.finish(Tt::PrivateId, Some(w));
                }
                Err(Fail::At(self.pos))
            }
            _ => Err(Fail::At(self.pos)),
        }
    }

    fn read_token_dot(&mut self) -> R<()> {
        let next = self.code(self.pos + 1);
        if (48..=57).contains(&next) {
            return self.read_number(true);
        }
        if next == 46 && self.code(self.pos + 2) == 46 {
            return self.finish_op(Tt::Ellipsis, 3);
        }
        self.finish_op(Tt::Dot, 1)
    }

    fn read_token_plus_min(&mut self, code: u32) -> R<()> {
        let next = self.code(self.pos + 1);
        if next == code {
            if next == 45
                && self.code(self.pos + 2) == 62
                && (self.last_tok_end == 0 || self.line_break_in(self.last_tok_end, self.pos))
            {
                self.skip_line_comment(3);
                self.skip_space()?;
                return self.next_token();
            }
            return self.finish_op(Tt::IncDec, 2);
        }
        if next == 61 {
            return self.finish_op(Tt::Assign, 2);
        }
        self.finish_op(Tt::PlusMin, 1)
    }

    fn read_token_lt_gt(&mut self, code: u32) -> R<()> {
        let next = self.code(self.pos + 1);
        let mut size = 1;
        if next == code {
            size = if code == 62 && self.code(self.pos + 2) == 62 {
                3
            } else {
                2
            };
            if self.code(self.pos + size) == 61 {
                return self.finish_op(Tt::Assign, size + 1);
            }
            return self.finish_op(Tt::Binop, size);
        }
        if next == 33
            && code == 60
            && self.code(self.pos + 2) == 45
            && self.code(self.pos + 3) == 45
        {
            self.skip_line_comment(4);
            self.skip_space()?;
            return self.next_token();
        }
        if next == 61 {
            size = 2;
        }
        self.finish_op(Tt::Binop, size)
    }

    fn read_regexp(&mut self) -> R<()> {
        let start = self.pos;
        let mut escaped = false;
        let mut in_class = false;
        loop {
            let Some(ch) = self.at(self.pos) else {
                return Err(Fail::At(start));
            };
            if is_newline(ch) {
                return Err(Fail::At(start));
            }
            if !escaped {
                if ch == b'[' as u32 {
                    in_class = true;
                } else if ch == b']' as u32 && in_class {
                    in_class = false;
                } else if ch == b'/' as u32 && !in_class {
                    break;
                }
                escaped = ch == b'\\' as u32;
            } else {
                escaped = false;
            }
            self.pos += 1;
        }
        let pattern = self.input[start..self.pos].to_vec();
        self.pos += 1;
        let flags_start = self.pos;
        let flags = self.read_word1()?;
        if self.contains_esc {
            return Err(Fail::At(flags_start));
        }
        if !flags_valid(&flags) || !pattern_valid(&pattern, &flags) {
            return Err(Fail::At(start));
        }
        self.finish(Tt::Regexp, None)
    }

    /// `readInt(radix, len, maybeLegacyOctal)`: Some(()) when digits
    /// were read (the value is never needed).
    fn read_int(&mut self, radix: u32, len: Option<i64>, maybe_legacy_octal: bool) -> R<bool> {
        let allow_sep = len.is_none();
        let legacy = maybe_legacy_octal && self.code(self.pos) == 48;
        let start = self.pos;
        let mut last = 0u32;
        let e = len.unwrap_or(i64::MAX);
        let mut i: i64 = 0;
        while i < e {
            let code = self.code(self.pos);
            if allow_sep && code == 95 {
                if legacy || last == 95 || i == 0 {
                    return Err(Fail::At(self.pos));
                }
                last = code;
                i += 1;
                self.pos += 1;
                continue;
            }
            let val = if code == u32::MAX {
                u32::MAX
            } else if code >= 97 {
                code - 97 + 10
            } else if code >= 65 {
                code - 65 + 10
            } else if (48..=57).contains(&code) {
                code - 48
            } else {
                u32::MAX
            };
            if val >= radix {
                break;
            }
            last = code;
            i += 1;
            self.pos += 1;
        }
        if allow_sep && last == 95 {
            return Err(Fail::At(self.pos - 1));
        }
        if self.pos == start || len.is_some_and(|l| (self.pos - start) as i64 != l) {
            return Ok(false);
        }
        Ok(true)
    }

    fn read_radix_number(&mut self, radix: u32) -> R<()> {
        self.pos += 2;
        if !self.read_int(radix, None, false)? {
            return Err(Fail::At(self.start + 2));
        }
        if self.code(self.pos) == 110 {
            self.pos += 1;
        } else if is_id_start(self.full_char_code_at(self.pos)) {
            return Err(Fail::At(self.pos));
        }
        self.finish(Tt::Num, None)
    }

    fn read_number(&mut self, starts_with_dot: bool) -> R<()> {
        let start = self.pos;
        if !starts_with_dot && !self.read_int(10, None, true)? {
            return Err(Fail::At(start));
        }
        let mut octal = self.pos - start >= 2 && self.code(start) == 48;
        if octal && self.strict {
            return Err(Fail::At(start));
        }
        let mut next = self.code(self.pos);
        if !octal && !starts_with_dot && next == 110 {
            self.pos += 1;
            if is_id_start(self.full_char_code_at(self.pos)) {
                return Err(Fail::At(self.pos));
            }
            return self.finish(Tt::Num, None);
        }
        if octal
            && self.input[start..self.pos]
                .iter()
                .any(|&c| c == 56 || c == 57)
        {
            octal = false;
        }
        if next == 46 && !octal {
            self.pos += 1;
            self.read_int(10, None, false)?;
            next = self.code(self.pos);
        }
        if (next == 69 || next == 101) && !octal {
            self.pos += 1;
            next = self.code(self.pos);
            if next == 43 || next == 45 {
                self.pos += 1;
            }
            if !self.read_int(10, None, false)? {
                return Err(Fail::At(start));
            }
        }
        if is_id_start(self.full_char_code_at(self.pos)) {
            return Err(Fail::At(self.pos));
        }
        self.finish(Tt::Num, None)
    }

    fn invalid_string_token(&self, pos: usize) -> Fail {
        if self.in_template_element {
            Fail::BadTemplateEscape
        } else {
            Fail::At(pos)
        }
    }

    /// `readHexChar(len)`: the value of exactly `len` hex digits.
    fn read_hex_char(&mut self, len: i64) -> R<u32> {
        let code_pos = self.pos;
        let start = self.pos;
        if !self.read_int(16, Some(len), false)? {
            return Err(self.invalid_string_token(code_pos));
        }
        let mut n: u64 = 0;
        for &c in &self.input[start..self.pos] {
            let d = (c as u8 as char).to_digit(16).unwrap_or(0) as u64;
            n = n.saturating_mul(16).saturating_add(d);
        }
        Ok(n.min(u32::MAX as u64) as u32)
    }

    fn read_code_point(&mut self) -> R<u32> {
        let ch = self.code(self.pos);
        if ch == 123 {
            self.pos += 1;
            let code_pos = self.pos;
            let close = (self.pos..self.input.len()).find(|&i| self.input[i] == b'}' as u16);
            let len = match close {
                Some(c) => (c - self.pos) as i64,
                None => -1 - self.pos as i64,
            };
            let code = self.read_hex_char(len)?;
            self.pos += 1;
            if code > 0x10ffff {
                return Err(self.invalid_string_token(code_pos));
            }
            Ok(code)
        } else {
            self.read_hex_char(4)
        }
    }

    fn read_string(&mut self, quote: u16) -> R<()> {
        let mut out: Vec<u16> = Vec::new();
        self.pos += 1;
        let mut chunk = self.pos;
        loop {
            let Some(ch) = self.at(self.pos) else {
                return Err(Fail::At(self.start));
            };
            if ch == quote as u32 {
                break;
            }
            if ch == 92 {
                out.extend_from_slice(&self.input[chunk..self.pos]);
                self.read_escaped_char(false, &mut out)?;
                chunk = self.pos;
            } else if ch == 0x2028 || ch == 0x2029 {
                self.pos += 1;
            } else {
                if is_newline(ch) {
                    return Err(Fail::At(self.start));
                }
                self.pos += 1;
            }
        }
        out.extend_from_slice(&self.input[chunk..self.pos]);
        self.pos += 1;
        self.finish(Tt::String, Some(out))
    }

    fn try_read_template_token(&mut self) -> R<()> {
        self.in_template_element = true;
        match self.read_tmpl_token() {
            Ok(()) => {}
            Err(Fail::BadTemplateEscape) => self.read_invalid_template_token()?,
            Err(e) => return Err(e),
        }
        self.in_template_element = false;
        Ok(())
    }

    fn read_tmpl_token(&mut self) -> R<()> {
        let mut out: Vec<u16> = Vec::new();
        let mut chunk = self.pos;
        loop {
            let Some(ch) = self.at(self.pos) else {
                return Err(Fail::At(self.start));
            };
            if ch == 96 || (ch == 36 && self.code(self.pos + 1) == 123) {
                if self.pos == self.start && matches!(self.tt, Tt::Template | Tt::InvalidTemplate) {
                    if ch == 36 {
                        return self.finish_op(Tt::DollarBraceL, 2);
                    }
                    return self.finish_op(Tt::BackQuote, 1);
                }
                out.extend_from_slice(&self.input[chunk..self.pos]);
                return self.finish(Tt::Template, Some(out));
            }
            if ch == 92 {
                out.extend_from_slice(&self.input[chunk..self.pos]);
                self.read_escaped_char(true, &mut out)?;
                chunk = self.pos;
            } else if is_newline(ch) {
                out.extend_from_slice(&self.input[chunk..self.pos]);
                self.pos += 1;
                match ch {
                    13 => {
                        if self.code(self.pos) == 10 {
                            self.pos += 1;
                        }
                        out.push(10);
                    }
                    10 => out.push(10),
                    _ => out.push(ch as u16),
                }
                chunk = self.pos;
            } else {
                self.pos += 1;
            }
        }
    }

    fn read_invalid_template_token(&mut self) -> R<()> {
        while self.pos < self.input.len() {
            match self.input[self.pos] {
                92 => self.pos += 1,
                36 if self.code(self.pos + 1) == 123 => {
                    return self.finish(Tt::InvalidTemplate, None);
                }
                96 => return self.finish(Tt::InvalidTemplate, None),
                13 if self.code(self.pos + 1) == 10 => self.pos += 1,
                _ => {}
            }
            self.pos += 1;
        }
        Err(Fail::At(self.start))
    }

    fn read_escaped_char(&mut self, in_template: bool, out: &mut Vec<u16>) -> R<()> {
        self.pos += 1;
        let ch = self.code(self.pos);
        self.pos += 1;
        match ch {
            110 => out.push(10),
            114 => out.push(13),
            120 => {
                let c = self.read_hex_char(2)?;
                out.push(c as u16);
            }
            117 => {
                let c = self.read_code_point()?;
                push_code_point(out, c);
            }
            116 => out.push(9),
            98 => out.push(8),
            118 => out.push(11),
            102 => out.push(12),
            13 => {
                if self.code(self.pos) == 10 {
                    self.pos += 1;
                }
            }
            10 => {}
            _ => {
                if ch == 56 || ch == 57 {
                    if self.strict {
                        return Err(self.invalid_string_token(self.pos - 1));
                    }
                    if in_template {
                        return Err(self.invalid_string_token(self.pos - 1));
                    }
                }
                if (48..=55).contains(&ch) {
                    // `substr(pos - 1, 3).match(/^[0-7]+/)`
                    let from = self.pos - 1;
                    let mut n = 0;
                    while n < 3
                        && from + n < self.input.len()
                        && (48..=55).contains(&self.input[from + n])
                    {
                        n += 1;
                    }
                    let digits = |k: usize| -> u32 {
                        self.input[from..from + k]
                            .iter()
                            .fold(0, |a, &d| a * 8 + (d as u32 - 48))
                    };
                    let mut len = n;
                    let mut octal = digits(len);
                    if octal > 255 {
                        len -= 1;
                        octal = digits(len);
                    }
                    self.pos += len - 1;
                    let next = self.code(self.pos);
                    let is_zero = len == 1 && self.input[from] == 48;
                    if (!is_zero || next == 56 || next == 57) && (self.strict || in_template) {
                        return Err(self.invalid_string_token(self.pos - 1 - len));
                    }
                    out.push(octal as u16);
                    return Ok(());
                }
                if is_newline(ch) {
                    return Ok(());
                }
                if ch == u32::MAX {
                    // `String.fromCharCode(NaN)` is "\0"
                    out.push(0);
                    return Ok(());
                }
                out.push(ch as u16);
            }
        }
        Ok(())
    }

    fn read_word1(&mut self) -> R<Vec<u16>> {
        self.contains_esc = false;
        let mut word: Vec<u16> = Vec::new();
        let mut first = true;
        let mut chunk = self.pos;
        while self.pos < self.input.len() {
            let ch = self.full_char_code_at(self.pos);
            if is_id_char(ch) {
                self.pos += if ch <= 0xffff { 1 } else { 2 };
            } else if ch == 92 {
                self.contains_esc = true;
                word.extend_from_slice(&self.input[chunk..self.pos]);
                let esc_start = self.pos;
                self.pos += 1;
                if self.code(self.pos) != 117 {
                    return Err(self.invalid_string_token(self.pos));
                }
                self.pos += 1;
                let esc = self.read_code_point()?;
                let ok = if first {
                    is_id_start(esc)
                } else {
                    is_id_char(esc)
                };
                if !ok {
                    return Err(self.invalid_string_token(esc_start));
                }
                push_code_point(&mut word, esc);
                chunk = self.pos;
            } else {
                break;
            }
            first = false;
        }
        word.extend_from_slice(&self.input[chunk..self.pos]);
        Ok(word)
    }

    fn read_word(&mut self) -> R<()> {
        let word = self.read_word1()?;
        let tt = keyword(&word).map_or(Tt::Name, Tt::Kw);
        self.finish(tt, Some(word))
    }
}

/// acorn `validateRegExpFlags` (ecmaVersion latest: `gimuysdv`).
fn flags_valid(flags: &[u16]) -> bool {
    const VALID: &str = "gimuysdv";
    let (mut u, mut v) = (false, false);
    for (i, &f) in flags.iter().enumerate() {
        if !VALID.encode_utf16().any(|x| x == f) || flags[i + 1..].contains(&f) {
            return false;
        }
        u |= f == b'u' as u16;
        v |= f == b'v' as u16;
    }
    !(u && v)
}

/// acorn `validateRegExpPattern`, by oxc's ECMAScript regex grammar.
fn pattern_valid(pattern: &[u16], flags: &[u16]) -> bool {
    let (Ok(p), Ok(f)) = (String::from_utf16(pattern), String::from_utf16(flags)) else {
        // a lone surrogate: the input came from a lossy UTF-8 decode and
        // holds none, but an escape cannot put one in a pattern either
        return false;
    };
    let alloc = oxc_allocator::Allocator::default();
    oxc_regular_expression::LiteralParser::new(
        &alloc,
        &p,
        Some(&f),
        oxc_regular_expression::Options::default(),
    )
    .parse()
    .is_ok()
}

/// `strictDirective(0)`: the input opens with a "use strict" directive.
fn strict_directive(input: &[u16]) -> bool {
    let mut start = 0;
    loop {
        start = skip_white_space(input, start);
        let Some((content, len)) = literal_at(input, start) else {
            return false;
        };
        if content.iter().copied().eq("use strict".encode_utf16()) {
            let end = skip_white_space(input, start + len);
            let space = &input[start + len..end];
            let next = input.get(end).copied();
            let is = |c: char| next == Some(c as u16);
            if is(';') || is('}') {
                return true;
            }
            let lb = space.iter().any(|&c| is_newline(c as u32));
            if !lb {
                return false;
            }
            let op = next.is_some_and(|n| "(`.[+-/*%<>=,?^&".encode_utf16().any(|x| x == n));
            let ne = is('!') && input.get(end + 1) == Some(&(b'=' as u16));
            let kw = is('i') && keyword_op_at(input, end);
            return !(op || ne || kw);
        }
        start += len;
        start = skip_white_space(input, start);
        if input.get(start) == Some(&(b';' as u16)) {
            start += 1;
        }
    }
}

/// `skipWhiteSpace` (`(?:\s|\/\/.*|\/\*[^]*?\*\/)*` at `from`): its end.
fn skip_white_space(input: &[u16], from: usize) -> usize {
    let mut k = from;
    loop {
        let Some(&c) = input.get(k) else {
            return k;
        };
        if is_js_space(c as u32) {
            k += 1;
            continue;
        }
        if c == b'/' as u16 && input.get(k + 1) == Some(&(b'/' as u16)) {
            k += 2;
            while k < input.len() && !is_newline(input[k] as u32) {
                k += 1;
            }
            continue;
        }
        if c == b'/' as u16 && input.get(k + 1) == Some(&(b'*' as u16)) {
            let close = (k + 2..input.len().saturating_sub(1))
                .find(|&i| input[i] == b'*' as u16 && input[i + 1] == b'/' as u16);
            match close {
                Some(i) => {
                    k = i + 2;
                    continue;
                }
                None => return k,
            }
        }
        return k;
    }
}

/// `literal` (`^(?:'((?:\\[^]|[^'\\])*?)'|"((?:\\[^]|[^"\\])*?)")`) at
/// `at`: (content, whole length).
fn literal_at(input: &[u16], at: usize) -> Option<(&[u16], usize)> {
    let q = *input.get(at)?;
    if q != b'\'' as u16 && q != b'"' as u16 {
        return None;
    }
    let mut k = at + 1;
    while k < input.len() {
        let c = input[k];
        if c == q {
            return Some((&input[at + 1..k], k + 1 - at));
        }
        if c == b'\\' as u16 {
            if k + 1 >= input.len() {
                return None;
            }
            k += 2;
        } else {
            k += 1;
        }
    }
    None
}

fn keyword_op_at(input: &[u16], pos: usize) -> bool {
    let stop = input.len().min(pos + 11);
    let lx = Lexer::new(input);
    let mut end = pos + 1;
    while end < stop {
        let ch = lx.full_char_code_at(end);
        if !is_id_char(ch) {
            break;
        }
        end += if ch <= 0xffff { 1 } else { 2 };
    }
    let word = &input[pos..end.min(input.len())];
    (end == pos + 2 && word.iter().copied().eq("in".encode_utf16()))
        || (end == pos + 10 && word.iter().copied().eq("instanceof".encode_utf16()))
}

impl<'a> Lexer<'a> {
    fn new(input: &'a [u16]) -> Lexer<'a> {
        Lexer {
            input,
            pos: 0,
            start: 0,
            end: 0,
            last_tok_end: 0,
            tt: Tt::Eof,
            value: None,
            context: vec![Ctx::BStat],
            expr_allowed: true,
            strict: false,
            contains_esc: false,
            in_template_element: false,
        }
    }
}

/// Every token of `input` up to the end or the first error, and the
/// error's position (None: the input tokenized to its end).
pub fn tokenize(input: &[u16]) -> (Vec<Tok>, Option<usize>) {
    let mut lx = Lexer::new(input);
    lx.strict = strict_directive(input);
    if input.len() >= 2 && input[0] == b'#' as u16 && input[1] == b'!' as u16 {
        lx.skip_line_comment(2);
    }
    let mut out = Vec::new();
    loop {
        match lx.next() {
            Ok(()) => {}
            Err(Fail::At(p)) => return (out, Some(p)),
            Err(Fail::BadTemplateEscape) => return (out, Some(0)),
        }
        if lx.tt == Tt::Eof {
            return (out, None);
        }
        out.push(Tok {
            tt: lx.tt,
            value: lx.value.clone(),
        });
    }
}
