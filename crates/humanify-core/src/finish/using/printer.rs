//! `@babel/generator` 7.29.7 (`printer.ts`, `buffer.ts`, `generators/*`,
//! `node/parentheses.ts`) for the one configuration the `using` desugar
//! prints with: `retainLines: true`, `compact: false`, no source maps, no
//! `preserveFormat`, no auxiliary comments — and no comments at all (the
//! desugar refuses a file that has any, see `super::desugar_using`).
//!
//! Under `retainLines`, `newline()` is a no-op: every line break comes from
//! catching the buffer up to a node's `loc` line (at its start, and at the
//! end for `}` / `)`), or from the newlines inside a multi-line token.
//! Nodes without `loc` (the transform's) therefore print on the current
//! line. The token state machine — the last-char codes (-1 after an
//! append, -2 after an integer, -3 after a word), the queued space /
//! semicolon, the token context — is the generator's, byte for byte.

use super::ast::{Binary, Call, Class, Field, Func, Kind, Loc, Member, Method, Node};

const F_CONCISE: u32 = 4;
const F_RETAIN_LINES: u32 = 8;

/// `TokenContext`.
const TC_EXPRESSION_STATEMENT: u32 = 1;
const TC_ARROW_BODY: u32 = 2;
const TC_EXPORT_DEFAULT: u32 = 4;
const TC_FOR_INIT_HEAD: u32 = 16;
const TC_FOR_IN_HEAD: u32 = 32;
const TC_FOR_OF_HEAD: u32 = 64;
const TC_ACCUMULATE: u32 = 128;

/// The generator's `Buffer`, map-less.
struct Buffer {
    out: String,
    /// `_last`: a char code, or -1 (after an append / indent), 0 (empty).
    last: i32,
    /// `_queuedChar` (0 = none).
    queued: i32,
    /// `_position.line`.
    line: u32,
}

fn push_code(out: &mut String, code: i32) {
    out.push(char::from_u32(code as u32).expect("an ASCII code"));
}

impl Buffer {
    fn new() -> Buffer {
        Buffer {
            out: String::new(),
            last: 0,
            queued: 0,
            line: 1,
        }
    }

    fn get(mut self) -> String {
        let last = self.last;
        if self.queued != 32 {
            self.flush();
        }
        if last == 10 {
            let trimmed = self
                .out
                .trim_end_matches(humanify_model::js::is_js_whitespace)
                .len();
            self.out.truncate(trimmed);
        }
        self.out
    }

    fn append(&mut self, s: &str, maybe_newline: bool) {
        self.flush();
        self.last = -1;
        self.out.push_str(s);
        // Map-less: lines advance only for a token that may carry newlines.
        if maybe_newline {
            self.line += s.matches('\n').count() as u32;
        }
    }

    fn append_char(&mut self, code: i32) {
        self.flush();
        self.append_char_raw(code);
    }

    fn append_char_raw(&mut self, code: i32) {
        self.last = code;
        push_code(&mut self.out, code);
        if code == 10 {
            self.line += 1;
        }
    }

    fn append_indent(&mut self, repeat: usize) {
        self.last = -1;
        self.out.extend(std::iter::repeat_n(' ', repeat));
    }

    fn queue(&mut self, code: i32) {
        self.flush();
        self.queued = code;
    }

    fn flush(&mut self) {
        let q = self.queued;
        if q != 0 {
            self.append_char_raw(q);
            self.queued = 0;
        }
    }

    fn last_char(&self, check_queue: bool) -> i32 {
        if check_queue && self.queued != 0 {
            self.queued
        } else {
            self.last
        }
    }
}

type Sep = fn(&mut Printer, bool);

fn comma_separator(p: &mut Printer, last: bool) {
    p.token_char(b',');
    if !last {
        p.space();
    }
}

fn comma_separator_with_newline(p: &mut Printer, _last: bool) {
    p.token_char(b',');
    p.newline();
}

/// `PRECEDENCE` (node/parentheses.ts).
fn precedence(op: &str) -> Option<u32> {
    Some(match op {
        "||" => 0,
        "??" => 1,
        "&&" => 2,
        "|" => 3,
        "^" => 4,
        "&" => 5,
        "==" | "===" | "!=" | "!==" => 6,
        "<" | ">" | "<=" | ">=" | "in" | "instanceof" => 7,
        ">>" | "<<" | ">>>" => 8,
        "+" | "-" => 9,
        "*" | "/" | "%" => 10,
        "**" => 11,
        _ => return None,
    })
}

fn ptr(n: &Node) -> *const Node {
    n as *const Node
}

fn is(a: &Node, b: &Node) -> bool {
    std::ptr::eq(a, b)
}

pub struct Printer {
    buf: Buffer,
    indent: usize,
    flags: u32,
    token_context: u32,
    no_line_terminator: bool,
    /// `_noLineTerminatorAfterNode` (identity only).
    nltan: Option<*const Node>,
}

impl Printer {
    pub fn new() -> Printer {
        Printer {
            buf: Buffer::new(),
            indent: 0,
            flags: F_RETAIN_LINES,
            token_context: 0,
            no_line_terminator: false,
            nltan: None,
        }
    }

    /// `generate(file)`: print the Program and take the buffer.
    pub fn generate(mut self, program: &Node) -> String {
        if let Kind::Program {
            interpreter: Some(value),
            ..
        } = &program.kind
        {
            // InterpreterDirective (line 1): `#!…` then a hard newline.
            self.catch_up(1);
            self.token(&format!("#!{value}"), false, false);
            self.hard_newline();
        }
        self.print(program, None, false, false);
        self.buf.get()
    }

    // -- the token layer (printer.ts) -------------------------------------------

    fn indent_with(&mut self, flags: u32) {
        if flags & (1 | 2 | F_CONCISE) != 0 {
            return;
        }
        self.indent += 2;
    }

    fn dedent_with(&mut self, flags: u32) {
        if flags & (1 | 2 | F_CONCISE) != 0 {
            return;
        }
        self.indent -= 2;
    }

    fn semicolon(&mut self, force: bool) {
        if force {
            self.append_char(59, false);
        } else {
            self.queue_char(59);
        }
        self.no_line_terminator = false;
    }

    fn right_brace(&mut self, node: &Node) {
        self.catch_up_end(node.loc);
        self.token_char(b'}');
    }

    fn right_parens(&mut self, node: &Node) {
        self.catch_up_end(node.loc);
        self.token_char(b')');
    }

    fn space(&mut self) {
        let last = self.buf.last_char(true);
        if last != 0 && last != 32 && last != 10 {
            self.queue_char(32);
        }
    }

    fn word_nlt(&mut self, s: &str, no_line_terminator_after: bool) {
        self.token_context &= TC_ACCUMULATE;
        let last = self.buf.last_char(false);
        if last == -2 || last == -3 || (last == 47 && s.starts_with('/')) {
            self.queue_char(32);
        }
        self.append(s, false);
        self.buf.last = -3;
        self.no_line_terminator = no_line_terminator_after;
    }

    fn word(&mut self, s: &str) {
        self.word_nlt(s, false);
    }

    fn number(&mut self, s: &str, value: f64) {
        self.word(s);
        let non_decimal =
            s.len() > 2 && s.as_bytes()[0] == b'0' && matches!(s.as_bytes()[1], b'b' | b'o' | b'x');
        let zero_decimal = {
            // /\.0+$/
            let t = s.trim_end_matches('0');
            t.len() < s.len() && t.ends_with('.')
        };
        if value.is_finite()
            && value.trunc() == value
            && !non_decimal
            && !s.contains(['e', 'E'])
            && !zero_decimal
            && !s.ends_with('.')
        {
            self.buf.last = -2;
        }
    }

    fn token(&mut self, s: &str, maybe_newline: bool, may_need_space: bool) {
        self.token_context &= TC_ACCUMULATE;
        if may_need_space {
            let first = s.as_bytes()[0];
            let last = self.buf.last_char(false);
            if ((first == b'-' && s == "--") || first == b'=') && last == 33
                || first == b'+' && last == 43
                || first == b'-' && last == 45
                || first == b'.' && last == -2
            {
                self.queue_char(32);
            }
        }
        self.append(s, maybe_newline);
        self.no_line_terminator = false;
    }

    fn token_char(&mut self, c: u8) {
        self.token_context &= TC_ACCUMULATE;
        let last = self.buf.last_char(false);
        if c == b'+' && last == 43 || c == b'-' && last == 45 || c == b'.' && last == -2 {
            self.queue_char(32);
        }
        self.append_char(i32::from(c), false);
        self.no_line_terminator = false;
    }

    /// `newline()`: a no-op under `retainLines`.
    fn newline(&mut self) {}

    fn ends_with(&self, c: i32) -> bool {
        self.buf.last_char(true) == c
    }

    /// `_newline()`.
    fn hard_newline(&mut self) {
        if self.buf.queued == 32 {
            self.buf.queued = 0;
        }
        self.append_char(10, true);
    }

    fn append(&mut self, s: &str, maybe_newline: bool) {
        self.maybe_indent();
        self.buf.append(s, maybe_newline);
    }

    fn append_char(&mut self, c: i32, no_indent: bool) {
        if !no_indent {
            self.maybe_indent();
        }
        self.buf.append_char(c);
    }

    fn queue_char(&mut self, c: i32) {
        self.buf.queue(c);
        self.buf.last = -1;
    }

    fn maybe_indent(&mut self) {
        if self.ends_with(10) && self.indent > 0 {
            self.buf.append_indent(self.indent);
        }
    }

    fn catch_up(&mut self, line: u32) {
        let current = self.buf.line;
        for _ in current..line {
            self.hard_newline();
        }
    }

    fn catch_up_end(&mut self, loc: Option<Loc>) {
        if let Some(l) = loc {
            self.catch_up(l.end);
        }
    }

    fn enter_delimited(&mut self) -> Option<*const Node> {
        let old = self.nltan;
        if old.is_some() {
            self.nltan = None;
        }
        old
    }

    // -- print / printJoin -------------------------------------------------------

    fn print_opt(&mut self, node: Option<&Node>, parent: &Node) {
        if let Some(n) = node {
            self.print(n, Some(parent), false, false);
        }
    }

    fn p(&mut self, node: &Node, parent: &Node) {
        self.print(node, Some(parent), false, false);
    }

    /// `resetTokenContext`: leave a for-head's accumulating context for
    /// this node; the value to restore after it (0 = nothing to restore).
    fn reset_token_context(&mut self, reset_tc: bool) -> u32 {
        if !reset_tc {
            return 0;
        }
        let old = self.token_context;
        if old & TC_ACCUMULATE != 0 {
            self.token_context = 0;
            old
        } else {
            0
        }
    }

    /// Whether `node` prints inside parentheses, and whether they are the
    /// retainLines kind (a no-line-terminator position whose node starts on
    /// a later line: `return (\n  …)`).
    fn parens_for(&self, node: &Node, parent: Option<&Node>, flags: u32) -> (bool, bool) {
        let needed = parent.is_some_and(|p| {
            parent_needs_parens(node, p) || needs_parens(node, p, self.token_context)
        });
        if needed {
            return (true, false);
        }
        let later_line = self.no_line_terminator
            && flags & F_RETAIN_LINES != 0
            && node.loc.is_some_and(|l| l.start > self.buf.line);
        (later_line, later_line)
    }

    /// Open the parentheses; the token context to restore afterwards.
    fn open_parens(&mut self, indent_parenthesized: bool, reset_tc: bool, old_tc: u32) -> u32 {
        self.token_char(b'(');
        if indent_parenthesized {
            self.indent_with(self.flags);
        }
        let old_tc = if reset_tc { old_tc } else { self.token_context };
        if old_tc & TC_ACCUMULATE != 0 {
            self.token_context = 0;
        }
        old_tc
    }

    fn print(&mut self, node: &Node, parent: Option<&Node>, nlta: bool, reset_tc: bool) {
        let flags = self.flags;
        if node.compact {
            self.flags |= F_CONCISE;
        }
        let mut old_tc = self.reset_token_context(reset_tc);
        let (should_parens, indent_parenthesized) = self.parens_for(node, parent, flags);
        // `undefined` / a saved value (restored only when non-null).
        let mut saved_nltan: Option<Option<*const Node>> = None;
        let mut nlta = nlta;
        if should_parens {
            old_tc = self.open_parens(indent_parenthesized, reset_tc, old_tc);
            saved_nltan = Some(self.nltan);
            self.nltan = None;
        } else {
            nlta =
                nlta || parent.is_some_and(|p| self.nltan == Some(ptr(p)) && p.is_last_child(node));
            if nlta {
                saved_nltan = Some(self.nltan);
                self.nltan = Some(ptr(node));
            }
        }
        if !matches!(node.kind, Kind::Program { .. })
            && let Some(loc) = node.loc
        {
            self.catch_up(loc.start);
        }
        self.print_method(node, parent);
        if should_parens {
            if indent_parenthesized {
                self.dedent_with(self.flags);
                self.newline();
            }
            self.token_char(b')');
            self.no_line_terminator = nlta;
        } else if nlta && !self.no_line_terminator {
            self.no_line_terminator = true;
        }
        if old_tc != 0 {
            self.token_context = old_tc;
        }
        self.flags = flags;
        if let Some(Some(p)) = saved_nltan {
            self.nltan = Some(p);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn print_join(
        &mut self,
        nodes: &[Node],
        parent: &Node,
        indent: Option<bool>,
        separator: Option<Sep>,
        print_trailing_separator: bool,
        reset_tc: bool,
    ) {
        if nodes.is_empty() {
            return;
        }
        let flags = self.flags;
        let mut indent = indent;
        if indent.is_none()
            && flags & F_RETAIN_LINES != 0
            && let Some(loc) = nodes[0].loc
            && loc.start != self.buf.line
        {
            indent = Some(true);
        }
        let indent = indent == Some(true);
        if indent {
            self.indent_with(flags);
        }
        let len = nodes.len();
        for (i, node) in nodes.iter().enumerate() {
            self.print(node, Some(parent), false, reset_tc);
            if let Some(sep) = separator {
                if i < len - 1 {
                    sep(self, false);
                } else if print_trailing_separator {
                    sep(self, true);
                }
            }
        }
        if indent {
            self.dedent_with(flags);
        }
    }

    fn print_sequence(&mut self, nodes: &[Node], parent: &Node, indent: bool) {
        self.print_join(nodes, parent, Some(indent), None, false, false);
    }

    fn print_list(&mut self, items: &[Node], parent: &Node, indent: Option<bool>, reset_tc: bool) {
        self.print_join(
            items,
            parent,
            indent,
            Some(comma_separator),
            false,
            reset_tc,
        );
    }

    fn print_block(&mut self, body: &Node, parent: &Node) {
        if !matches!(body.kind, Kind::EmptyStatement) {
            self.space();
        }
        self.p(body, parent);
    }

    // -- the node printers (generators/*) ---------------------------------------

    fn print_method(&mut self, node: &Node, parent: Option<&Node>) {
        match &node.kind {
            Kind::Program { .. }
            | Kind::Directive { .. }
            | Kind::DirectiveLiteral { .. }
            | Kind::BlockStatement { .. }
            | Kind::StaticBlock { .. }
            | Kind::ExpressionStatement { .. }
            | Kind::EmptyStatement
            | Kind::DebuggerStatement
            | Kind::WithStatement { .. }
            | Kind::ReturnStatement { .. }
            | Kind::ThrowStatement { .. }
            | Kind::BreakStatement { .. }
            | Kind::ContinueStatement { .. }
            | Kind::LabeledStatement { .. } => self.print_statement(node, parent),
            Kind::IfStatement { .. }
            | Kind::SwitchStatement { .. }
            | Kind::SwitchCase { .. }
            | Kind::TryStatement { .. }
            | Kind::CatchClause { .. }
            | Kind::WhileStatement { .. }
            | Kind::DoWhileStatement { .. } => self.print_control(node, parent),
            Kind::ForStatement { .. }
            | Kind::ForInStatement { .. }
            | Kind::ForOfStatement { .. } => self.print_loop(node, parent),
            Kind::FunctionDeclaration { .. }
            | Kind::FunctionExpression { .. }
            | Kind::ArrowFunctionExpression { .. }
            | Kind::VariableDeclaration { .. }
            | Kind::VariableDeclarator { .. }
            | Kind::ClassDeclaration { .. }
            | Kind::ClassExpression { .. }
            | Kind::ClassBody { .. }
            | Kind::ClassMethod { .. }
            | Kind::ClassPrivateMethod { .. }
            | Kind::ObjectMethod { .. }
            | Kind::ClassProperty { .. }
            | Kind::ClassPrivateProperty { .. }
            | Kind::ClassAccessorProperty { .. } => self.print_declaration(node, parent),
            Kind::Identifier { .. }
            | Kind::PrivateName { .. }
            | Kind::StringLiteral { .. }
            | Kind::NumericLiteral { .. }
            | Kind::BigIntLiteral { .. }
            | Kind::BooleanLiteral { .. }
            | Kind::NullLiteral
            | Kind::RegExpLiteral { .. }
            | Kind::TemplateLiteral { .. }
            | Kind::TaggedTemplateExpression { .. }
            | Kind::ThisExpression
            | Kind::Super
            | Kind::Import => self.print_atom(node, parent),
            Kind::ArrayExpression { .. }
            | Kind::ArrayPattern { .. }
            | Kind::ObjectExpression { .. }
            | Kind::ObjectPattern { .. }
            | Kind::ObjectProperty { .. }
            | Kind::SpreadElement { .. }
            | Kind::RestElement { .. } => self.print_collection(node, parent),
            Kind::UnaryExpression { .. }
            | Kind::UpdateExpression { .. }
            | Kind::BinaryExpression { .. }
            | Kind::LogicalExpression { .. }
            | Kind::AssignmentExpression { .. }
            | Kind::AssignmentPattern { .. }
            | Kind::ConditionalExpression { .. } => self.print_operator(node, parent),
            Kind::CallExpression { .. }
            | Kind::OptionalCallExpression { .. }
            | Kind::NewExpression { .. }
            | Kind::MemberExpression { .. }
            | Kind::OptionalMemberExpression { .. }
            | Kind::SequenceExpression { .. }
            | Kind::YieldExpression { .. }
            | Kind::AwaitExpression { .. }
            | Kind::MetaProperty { .. } => self.print_access(node, parent),
        }
    }

    fn print_statement(&mut self, node: &Node, _parent: Option<&Node>) {
        match &node.kind {
            Kind::Program {
                directives, body, ..
            } => {
                if !directives.is_empty() {
                    self.print_sequence(directives, node, false);
                    self.newline();
                }
                self.print_sequence(body, node, false);
            }
            Kind::Directive { value } => {
                self.p(value, node);
                self.semicolon(false);
            }
            Kind::DirectiveLiteral { raw } => self.token(raw, false, false),
            Kind::BlockStatement { directives, body } => {
                self.token_char(b'{');
                let old = self.enter_delimited();
                if !directives.is_empty() {
                    self.print_sequence(directives, node, true);
                    self.newline();
                }
                self.print_sequence(body, node, true);
                self.nltan = old;
                self.right_brace(node);
            }
            Kind::StaticBlock { body } => {
                self.word("static");
                self.space();
                self.token_char(b'{');
                if body.is_empty() {
                    self.token_char(b'}');
                } else {
                    self.newline();
                    self.print_sequence(body, node, true);
                    self.right_brace(node);
                }
            }
            Kind::ExpressionStatement { expression } => {
                self.token_context |= TC_EXPRESSION_STATEMENT;
                self.p(expression, node);
                self.semicolon(false);
            }
            Kind::EmptyStatement => self.semicolon(true),
            Kind::DebuggerStatement => {
                self.word("debugger");
                self.semicolon(false);
            }
            Kind::WithStatement { object, body } => {
                self.word("with");
                self.space();
                self.token_char(b'(');
                self.p(object, node);
                self.token_char(b')');
                self.print_block(body, node);
            }
            Kind::ReturnStatement { argument } => {
                self.word("return");
                self.after_keyword(argument.as_deref(), node);
            }
            Kind::ThrowStatement { argument } => {
                self.word("throw");
                self.after_keyword(Some(argument), node);
            }
            Kind::BreakStatement { label } => {
                self.word("break");
                self.after_keyword(label.as_deref(), node);
            }
            Kind::ContinueStatement { label } => {
                self.word("continue");
                self.after_keyword(label.as_deref(), node);
            }
            Kind::LabeledStatement { label, body } => {
                self.p(label, node);
                self.token_char(b':');
                self.space();
                self.p(body, node);
            }
            _ => unreachable!("dispatched by print_method"),
        }
    }

    fn print_control(&mut self, node: &Node, _parent: Option<&Node>) {
        match &node.kind {
            Kind::IfStatement {
                test,
                consequent,
                alternate,
            } => self.if_statement(node, test, consequent, alternate.as_deref()),
            Kind::SwitchStatement {
                discriminant,
                cases,
            } => {
                self.word("switch");
                self.space();
                self.token_char(b'(');
                self.p(discriminant, node);
                self.token_char(b')');
                self.space();
                self.token_char(b'{');
                self.print_sequence(cases, node, true);
                self.right_brace(node);
            }
            Kind::SwitchCase { test, consequent } => {
                match test {
                    Some(t) => {
                        self.word("case");
                        self.space();
                        self.p(t, node);
                        self.token_char(b':');
                    }
                    None => {
                        self.word("default");
                        self.token_char(b':');
                    }
                }
                if !consequent.is_empty() {
                    self.newline();
                    self.print_sequence(consequent, node, true);
                }
            }
            Kind::TryStatement {
                block,
                handler,
                finalizer,
            } => {
                self.word("try");
                self.space();
                self.p(block, node);
                self.space();
                self.print_opt(handler.as_deref(), node);
                if let Some(f) = finalizer {
                    self.space();
                    self.word("finally");
                    self.space();
                    self.p(f, node);
                }
            }
            Kind::CatchClause { param, body } => {
                self.word("catch");
                self.space();
                if let Some(param) = param {
                    self.token_char(b'(');
                    self.p(param, node);
                    self.token_char(b')');
                    self.space();
                }
                self.p(body, node);
            }
            Kind::WhileStatement { test, body } => {
                self.word("while");
                self.space();
                self.token_char(b'(');
                self.p(test, node);
                self.token_char(b')');
                self.print_block(body, node);
            }
            Kind::DoWhileStatement { body, test } => {
                self.word("do");
                self.space();
                self.p(body, node);
                self.space();
                self.word("while");
                self.space();
                self.token_char(b'(');
                self.p(test, node);
                self.token_char(b')');
                self.semicolon(false);
            }
            _ => unreachable!("dispatched by print_method"),
        }
    }

    fn print_loop(&mut self, node: &Node, _parent: Option<&Node>) {
        match &node.kind {
            Kind::ForStatement {
                init,
                test,
                update,
                body,
            } => {
                self.word("for");
                self.space();
                self.token_char(b'(');
                self.token_context |= TC_FOR_INIT_HEAD | TC_ACCUMULATE;
                self.print_opt(init.as_deref(), node);
                self.token_context = 0;
                self.token_char(b';');
                if let Some(t) = test {
                    self.space();
                    self.p(t, node);
                }
                self.token_char(b';');
                if let Some(u) = update {
                    self.space();
                    self.p(u, node);
                }
                self.token_char(b')');
                self.print_block(body, node);
            }
            Kind::ForInStatement { left, right, body } => {
                self.word("for");
                self.space();
                self.token_char(b'(');
                self.token_context |= TC_FOR_IN_HEAD | TC_ACCUMULATE;
                self.p(left, node);
                self.token_context = 0;
                self.space();
                self.word("in");
                self.space();
                self.p(right, node);
                self.token_char(b')');
                self.print_block(body, node);
            }
            Kind::ForOfStatement {
                is_await,
                left,
                right,
                body,
            } => {
                self.word("for");
                self.space();
                if *is_await {
                    self.word("await");
                    self.space();
                }
                self.token_char(b'(');
                self.token_context |= TC_FOR_OF_HEAD;
                self.p(left, node);
                self.space();
                self.word("of");
                self.space();
                self.p(right, node);
                self.token_char(b')');
                self.print_block(body, node);
            }
            _ => unreachable!("dispatched by print_method"),
        }
    }

    fn print_declaration(&mut self, node: &Node, parent: Option<&Node>) {
        match &node.kind {
            Kind::FunctionDeclaration(f) | Kind::FunctionExpression(f) => {
                self.function_head(f, node);
                self.space();
                self.p(&f.body, node);
            }
            Kind::ArrowFunctionExpression(f) => {
                if f.is_async {
                    self.word_nlt("async", true);
                    self.space();
                }
                // `_shouldPrintArrowParamsParens` is true under retainLines.
                self.params(&f.params, node);
                self.no_line_terminator = true;
                self.space();
                self.token("=>", false, false);
                self.space();
                self.token_context |= TC_ARROW_BODY;
                self.p(&f.body, node);
            }
            Kind::VariableDeclaration { kind, declarations } => {
                self.variable_declaration(node, kind, declarations, parent)
            }
            Kind::VariableDeclarator { id, init } => {
                self.p(id, node);
                if let Some(init) = init {
                    self.space();
                    self.token_char(b'=');
                    self.space();
                    self.p(init, node);
                }
            }
            Kind::ClassDeclaration(c) | Kind::ClassExpression(c) => self.class(c, node),
            Kind::ClassBody { body } => {
                self.token_char(b'{');
                if body.is_empty() {
                    self.token_char(b'}');
                } else {
                    let old = self.enter_delimited();
                    self.print_join(body, node, Some(true), None, true, true);
                    self.nltan = old;
                    if !self.ends_with(10) {
                        self.newline();
                    }
                    self.right_brace(node);
                }
            }
            Kind::ClassMethod(m) | Kind::ClassPrivateMethod(m) => {
                if let Some(l) = m.key.loc {
                    self.catch_up(l.end);
                }
                if m.is_static {
                    self.word("static");
                    self.space();
                }
                self.method_head(m, node);
                self.space();
                self.p(&m.func.body, node);
            }
            Kind::ObjectMethod(m) => {
                self.method_head(m, node);
                self.space();
                self.p(&m.func.body, node);
            }
            Kind::ClassProperty(fd) => {
                if !fd.is_static
                    && let Some(l) = fd.key.loc
                {
                    self.catch_up(l.end);
                }
                self.field(fd, node, false);
            }
            Kind::ClassPrivateProperty(fd) => self.field(fd, node, false),
            Kind::ClassAccessorProperty(fd) => {
                if let Some(l) = fd.key.loc {
                    self.catch_up(l.end);
                }
                self.field(fd, node, true);
            }
            _ => unreachable!("dispatched by print_method"),
        }
    }

    fn print_atom(&mut self, node: &Node, _parent: Option<&Node>) {
        match &node.kind {
            Kind::Identifier { name } => self.word(name),
            Kind::PrivateName { id } => {
                self.token_char(b'#');
                self.p(id, node);
            }
            Kind::StringLiteral { value, raw } => match raw {
                Some(raw) => self.token(raw, false, false),
                None => {
                    let v = jsesc_double(value);
                    self.token(&v, false, false);
                }
            },
            Kind::NumericLiteral { value, raw } => {
                let s = raw
                    .clone()
                    .unwrap_or_else(|| humanify_model::js::number_to_string(*value));
                self.number(&s, *value);
            }
            Kind::BigIntLiteral { raw } => self.word(raw),
            Kind::BooleanLiteral { value } => self.word(if *value { "true" } else { "false" }),
            Kind::NullLiteral => self.word("null"),
            Kind::RegExpLiteral { text } => self.word(text),
            Kind::TemplateLiteral {
                quasis,
                expressions,
            } => {
                let mut part = String::from("`");
                for i in 0..quasis.len().saturating_sub(1) {
                    part.push_str(&quasis[i]);
                    part.push_str("${");
                    self.token(&part, true, false);
                    self.p(&expressions[i], node);
                    part = String::from("}");
                }
                if let Some(last) = quasis.last() {
                    part.push_str(last);
                }
                part.push('`');
                self.token(&part, true, false);
            }
            Kind::TaggedTemplateExpression { tag, quasi } => {
                self.p(tag, node);
                self.p(quasi, node);
            }
            Kind::ThisExpression => self.word("this"),
            Kind::Super => self.word("super"),
            Kind::Import => self.word("import"),
            _ => unreachable!("dispatched by print_method"),
        }
    }

    fn print_collection(&mut self, node: &Node, _parent: Option<&Node>) {
        match &node.kind {
            Kind::ArrayExpression { elements } | Kind::ArrayPattern { elements } => {
                self.token_char(b'[');
                let old = self.enter_delimited();
                let len = elements.len();
                for (i, el) in elements.iter().enumerate() {
                    match el {
                        Some(el) => {
                            if i > 0 {
                                self.space();
                            }
                            self.print(el, Some(node), false, true);
                            if i < len - 1 {
                                self.token_char(b',');
                            }
                        }
                        None => self.token_char(b','),
                    }
                }
                self.nltan = old;
                self.token_char(b']');
            }
            Kind::ObjectExpression { properties } | Kind::ObjectPattern { properties } => {
                self.token_char(b'{');
                if !properties.is_empty() {
                    let old = self.enter_delimited();
                    self.space();
                    self.print_join(
                        properties,
                        node,
                        Some(true),
                        Some(comma_separator),
                        false,
                        true,
                    );
                    self.space();
                    self.nltan = old;
                }
                self.right_brace(node);
            }
            Kind::ObjectProperty {
                key,
                value,
                computed,
                shorthand,
            } => {
                if *computed {
                    self.token_char(b'[');
                    self.p(key, node);
                    self.token_char(b']');
                } else {
                    if let (Kind::AssignmentPattern { left, .. }, Some(k)) =
                        (&value.kind, key.identifier_name())
                        && left.identifier_name() == Some(k)
                    {
                        self.p(value, node);
                        return;
                    }
                    self.p(key, node);
                    if *shorthand
                        && let (Some(k), Some(v)) = (key.identifier_name(), value.identifier_name())
                        && k == v
                    {
                        return;
                    }
                }
                self.token_char(b':');
                self.space();
                self.p(value, node);
            }
            Kind::SpreadElement { argument } | Kind::RestElement { argument } => {
                self.token("...", false, false);
                self.p(argument, node);
            }
            _ => unreachable!("dispatched by print_method"),
        }
    }

    fn print_operator(&mut self, node: &Node, _parent: Option<&Node>) {
        match &node.kind {
            Kind::UnaryExpression { operator, argument } => {
                let first = operator.as_bytes()[0];
                if first.is_ascii_lowercase() {
                    self.word(operator);
                    self.space();
                } else {
                    self.token_char(first);
                }
                self.p(argument, node);
            }
            Kind::UpdateExpression {
                operator,
                prefix,
                argument,
            } => {
                if *prefix {
                    self.token(operator, false, true);
                    self.p(argument, node);
                } else {
                    self.print(argument, Some(node), true, false);
                    self.token(operator, false, true);
                }
            }
            Kind::BinaryExpression(bx) => {
                self.p(&bx.left, node);
                self.space();
                if bx.operator.starts_with('i') {
                    self.word(bx.operator);
                } else {
                    self.token(bx.operator, false, true);
                    self.buf.last = i32::from(*bx.operator.as_bytes().last().expect("operator"));
                }
                self.space();
                self.p(&bx.right, node);
            }
            Kind::LogicalExpression(bx) | Kind::AssignmentExpression(bx) => {
                self.p(&bx.left, node);
                self.space();
                self.token(bx.operator, false, true);
                self.space();
                self.p(&bx.right, node);
            }
            Kind::AssignmentPattern { left, right } => {
                self.p(left, node);
                self.space();
                self.token_char(b'=');
                self.space();
                self.p(right, node);
            }
            Kind::ConditionalExpression {
                test,
                consequent,
                alternate,
            } => {
                self.p(test, node);
                self.space();
                self.token_char(b'?');
                self.space();
                self.p(consequent, node);
                self.space();
                self.token_char(b':');
                self.space();
                self.p(alternate, node);
            }
            _ => unreachable!("dispatched by print_method"),
        }
    }

    fn print_access(&mut self, node: &Node, _parent: Option<&Node>) {
        match &node.kind {
            Kind::CallExpression(c) => {
                self.p(&c.callee, node);
                self.call_arguments(c, node);
            }
            Kind::OptionalCallExpression(c) => {
                self.p(&c.callee, node);
                if c.optional {
                    self.token("?.", false, false);
                }
                self.call_arguments(c, node);
            }
            Kind::NewExpression(c) => {
                self.word("new");
                self.space();
                self.p(&c.callee, node);
                self.call_arguments(c, node);
            }
            Kind::MemberExpression(m) => self.member(m, node, false),
            Kind::OptionalMemberExpression(m) => self.member(m, node, true),
            Kind::SequenceExpression { expressions } => {
                self.print_list(expressions, node, None, false);
            }
            Kind::YieldExpression { argument, delegate } => {
                if *delegate {
                    self.word_nlt("yield", true);
                    self.token_char(b'*');
                    if let Some(a) = argument {
                        self.space();
                        self.p(a, node);
                    }
                } else if let Some(a) = argument {
                    self.word_nlt("yield", true);
                    self.space();
                    self.p(a, node);
                } else {
                    self.word("yield");
                }
            }
            Kind::AwaitExpression { argument } => {
                self.word("await");
                self.space();
                self.p(argument, node);
            }
            Kind::MetaProperty { meta, property } => {
                self.p(meta, node);
                self.token_char(b'.');
                self.p(property, node);
            }
            _ => unreachable!("dispatched by print_method"),
        }
    }

    fn after_keyword(&mut self, arg: Option<&Node>, parent: &Node) {
        if let Some(arg) = arg {
            self.space();
            // printTerminatorless
            self.no_line_terminator = true;
            self.p(arg, parent);
        }
        self.semicolon(false);
    }

    fn if_statement(
        &mut self,
        node: &Node,
        test: &Node,
        consequent: &Node,
        alternate: Option<&Node>,
    ) {
        self.word("if");
        self.space();
        self.token_char(b'(');
        self.p(test, node);
        self.token_char(b')');
        self.space();
        let needs_block = alternate.is_some()
            && matches!(last_statement(consequent).kind, Kind::IfStatement { .. });
        if needs_block {
            self.token_char(b'{');
            self.newline();
            self.indent_with(self.flags);
        }
        self.p(consequent, node);
        if needs_block {
            self.dedent_with(self.flags);
            self.newline();
            self.token_char(b'}');
        }
        if let Some(alt) = alternate {
            if self.ends_with(125) {
                self.space();
            }
            self.word("else");
            self.space();
            self.p(alt, node);
        }
    }

    fn variable_declaration(
        &mut self,
        node: &Node,
        kind: &str,
        declarations: &[Node],
        parent: Option<&Node>,
    ) {
        match kind {
            "await using" => {
                self.word("await");
                self.space();
                self.word_nlt("using", true);
            }
            "using" => self.word_nlt("using", true),
            other => self.word(other),
        }
        self.space();
        let parent_is_for = parent.is_some_and(|p| {
            matches!(
                p.kind,
                Kind::ForStatement { .. }
                    | Kind::ForInStatement { .. }
                    | Kind::ForOfStatement { .. }
            )
        });
        let has_inits = !parent_is_for
            && declarations
                .iter()
                .any(|d| matches!(d.kind, Kind::VariableDeclarator { init: Some(_), .. }));
        self.print_join(
            declarations,
            node,
            Some(declarations.len() > 1),
            Some(if has_inits {
                comma_separator_with_newline
            } else {
                comma_separator
            }),
            false,
            false,
        );
        if let Some(p) = parent {
            match &p.kind {
                Kind::ForStatement { init: Some(i), .. } if is(i, node) => return,
                Kind::ForInStatement { left, .. } | Kind::ForOfStatement { left, .. }
                    if is(left, node) =>
                {
                    return;
                }
                _ => {}
            }
        }
        self.semicolon(false);
    }

    fn params(&mut self, params: &[Node], node: &Node) {
        self.token_char(b'(');
        let old = self.enter_delimited();
        let len = params.len();
        for (i, p) in params.iter().enumerate() {
            self.print(p, Some(node), false, true);
            if i < len - 1 {
                self.token_char(b',');
                self.space();
            }
        }
        self.token_char(b')');
        self.nltan = old;
    }

    fn function_head(&mut self, f: &Func, node: &Node) {
        if f.is_async {
            self.word("async");
            self.space();
        }
        self.word("function");
        if f.generator {
            self.token_char(b'*');
        }
        self.space();
        self.print_opt(f.id.as_deref(), node);
        self.params(&f.params, node);
        self.no_line_terminator = false;
    }

    fn method_head(&mut self, m: &Method, node: &Node) {
        if m.kind == "get" || m.kind == "set" {
            self.word(m.kind);
            self.space();
        }
        if m.func.is_async {
            self.word_nlt("async", true);
            self.space();
        }
        if (m.kind == "method" || m.kind == "init") && m.func.generator {
            self.token_char(b'*');
        }
        if m.computed {
            self.token_char(b'[');
            self.p(&m.key, node);
            self.token_char(b']');
        } else {
            self.p(&m.key, node);
        }
        self.params(&m.func.params, node);
        self.no_line_terminator = false;
    }

    fn field(&mut self, fd: &Field, node: &Node, accessor: bool) {
        if fd.is_static {
            self.word("static");
            self.space();
        }
        if accessor {
            self.word_nlt("accessor", true);
            self.space();
        }
        if fd.computed {
            self.token_char(b'[');
            self.p(&fd.key, node);
            self.token_char(b']');
        } else {
            self.p(&fd.key, node);
        }
        if let Some(v) = &fd.value {
            self.space();
            self.token_char(b'=');
            self.space();
            self.p(v, node);
        }
        self.semicolon(false);
    }

    fn class(&mut self, c: &Class, node: &Node) {
        self.word("class");
        if let Some(id) = &c.id {
            self.space();
            self.p(id, node);
        }
        if let Some(sc) = &c.super_class {
            self.space();
            self.word("extends");
            self.space();
            self.p(sc, node);
        }
        self.space();
        self.p(&c.body, node);
    }

    fn call_arguments(&mut self, c: &Call, node: &Node) {
        self.token_char(b'(');
        let old = self.enter_delimited();
        self.print_list(&c.arguments, node, None, true);
        self.nltan = old;
        self.right_parens(node);
    }

    fn member(&mut self, m: &Member, node: &Node, optional_type: bool) {
        self.p(&m.object, node);
        let mut computed = m.computed;
        if matches!(m.property.kind, Kind::NumericLiteral { .. }) {
            computed = true;
        }
        if optional_type {
            if m.optional {
                self.token("?.", false, false);
            }
            if computed {
                self.token_char(b'[');
                self.p(&m.property, node);
                self.token_char(b']');
            } else {
                if !m.optional {
                    self.token_char(b'.');
                }
                self.p(&m.property, node);
            }
        } else if computed {
            let old = self.enter_delimited();
            self.token_char(b'[');
            self.print(&m.property, Some(node), false, true);
            self.token_char(b']');
            self.nltan = old;
        } else {
            self.token_char(b'.');
            self.p(&m.property, node);
        }
    }
}

impl Default for Printer {
    fn default() -> Self {
        Printer::new()
    }
}

/// `getLastStatement` (statements.ts): follow `.body` while it is a
/// statement node.
fn last_statement(stmt: &Node) -> &Node {
    let body = match &stmt.kind {
        Kind::WithStatement { body, .. }
        | Kind::LabeledStatement { body, .. }
        | Kind::WhileStatement { body, .. }
        | Kind::DoWhileStatement { body, .. }
        | Kind::ForStatement { body, .. }
        | Kind::ForInStatement { body, .. }
        | Kind::ForOfStatement { body, .. } => body,
        _ => return stmt,
    };
    if body.is_statement() {
        last_statement(body)
    } else {
        stmt
    }
}

/// `jsesc(value, { quotes: "double", wrap: true })` for the transform's
/// synthesized names (the only raw-less strings it prints).
fn jsesc_double(value: &str) -> String {
    let mut out = String::from("\"");
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ' '..='~' => out.push(c),
            _ => {
                let mut units = [0u16; 2];
                for u in c.encode_utf16(&mut units) {
                    out.push_str(&format!("\\u{:04X}", u));
                }
            }
        }
    }
    out.push('"');
    out
}

// -- parentheses (node/index.ts + node/parentheses.ts) ---------------------------

fn is_or_has_call_expression(node: &Node) -> bool {
    match &node.kind {
        Kind::CallExpression(_) => true,
        Kind::MemberExpression(m) => is_or_has_call_expression(&m.object),
        _ => false,
    }
}

/// `parentNeedsParens`.
fn parent_needs_parens(node: &Node, parent: &Node) -> bool {
    matches!(&parent.kind, Kind::NewExpression(c) if is(&c.callee, node))
        && is_or_has_call_expression(node)
}

fn is_class_extends_clause(node: &Node, parent: &Node) -> bool {
    matches!(&parent.kind, Kind::ClassDeclaration(c) | Kind::ClassExpression(c)
        if c.super_class.as_deref().is_some_and(|s| is(s, node)))
}

fn has_postfix_part(node: &Node, parent: &Node) -> bool {
    match &parent.kind {
        Kind::MemberExpression(m) | Kind::OptionalMemberExpression(m) => is(&m.object, node),
        Kind::CallExpression(c) | Kind::OptionalCallExpression(c) | Kind::NewExpression(c) => {
            is(&c.callee, node)
        }
        Kind::TaggedTemplateExpression { tag, .. } => is(tag, node),
        _ => false,
    }
}

fn binary_like(node: &Node, parent: &Node, op: &str, logical: bool) -> bool {
    if is_class_extends_clause(node, parent) {
        return true;
    }
    if has_postfix_part(node, parent)
        || matches!(
            parent.kind,
            Kind::UnaryExpression { .. }
                | Kind::SpreadElement { .. }
                | Kind::AwaitExpression { .. }
        )
    {
        return true;
    }
    let (parent_pos, parent_binary) = match &parent.kind {
        Kind::BinaryExpression(b) => (precedence(b.operator), Some(b)),
        Kind::LogicalExpression(b) => (precedence(b.operator), None),
        _ => (None, None),
    };
    if let Some(parent_pos) = parent_pos {
        let node_pos = precedence(op).expect("a binary operator");
        if parent_pos > node_pos {
            return true;
        }
        if parent_pos == node_pos
            && let Some(pb) = parent_binary
            && (if node_pos == 11 {
                is(&pb.left, node)
            } else {
                is(&pb.right, node)
            })
        {
            return true;
        }
        if logical
            && matches!(parent.kind, Kind::LogicalExpression(_))
            && ((node_pos == 1 && parent_pos != 1) || (parent_pos == 1 && node_pos != 1))
        {
            return true;
        }
    }
    false
}

fn unary_like(node: &Node, parent: &Node) -> bool {
    has_postfix_part(node, parent)
        || matches!(&parent.kind, Kind::BinaryExpression(b) if b.operator == "**" && is(&b.left, node))
        || is_class_extends_clause(node, parent)
}

fn conditional_like(node: &Node, parent: &Node) -> bool {
    match &parent.kind {
        Kind::UnaryExpression { .. }
        | Kind::SpreadElement { .. }
        | Kind::BinaryExpression(_)
        | Kind::LogicalExpression(_)
        | Kind::AwaitExpression { .. } => return true,
        Kind::ConditionalExpression { test, .. } if is(test, node) => return true,
        _ => {}
    }
    unary_like(node, parent)
}

fn needs_paren_before_expression_brace(tc: u32) -> bool {
    tc & (TC_EXPRESSION_STATEMENT | TC_ARROW_BODY) != 0
}

/// The per-type `needsParens` table.
fn needs_parens(node: &Node, parent: &Node, tc: u32) -> bool {
    match &node.kind {
        Kind::UpdateExpression { .. } => {
            has_postfix_part(node, parent) || is_class_extends_clause(node, parent)
        }
        Kind::ObjectExpression { .. } => needs_paren_before_expression_brace(tc),
        Kind::BinaryExpression(b) => {
            binary_like(node, parent, b.operator, false)
                || (tc & TC_ACCUMULATE != 0 && b.operator == "in")
        }
        Kind::LogicalExpression(b) => binary_like(node, parent, b.operator, true),
        Kind::SequenceExpression { .. } => sequence_needs_parens(node, parent),
        Kind::YieldExpression { .. } | Kind::AwaitExpression { .. } => {
            matches!(
                parent.kind,
                Kind::BinaryExpression(_)
                    | Kind::LogicalExpression(_)
                    | Kind::UnaryExpression { .. }
                    | Kind::SpreadElement { .. }
            ) || has_postfix_part(node, parent)
                || (matches!(parent.kind, Kind::AwaitExpression { .. })
                    && matches!(node.kind, Kind::YieldExpression { .. }))
                || matches!(&parent.kind, Kind::ConditionalExpression { test, .. } if is(test, node))
                || is_class_extends_clause(node, parent)
        }
        Kind::ClassExpression(_) | Kind::FunctionExpression(_) => {
            tc & (TC_EXPRESSION_STATEMENT | TC_EXPORT_DEFAULT) != 0
        }
        Kind::UnaryExpression { .. } | Kind::SpreadElement { .. } => unary_like(node, parent),
        Kind::ConditionalExpression { .. } | Kind::ArrowFunctionExpression(_) => {
            conditional_like(node, parent)
        }
        Kind::OptionalMemberExpression(_) | Kind::OptionalCallExpression(_) => match &parent.kind {
            Kind::CallExpression(c) => is(&c.callee, node),
            Kind::MemberExpression(m) => is(&m.object, node),
            _ => false,
        },
        Kind::AssignmentExpression(b) => {
            if needs_paren_before_expression_brace(tc)
                && matches!(b.left.kind, Kind::ObjectPattern { .. })
            {
                return true;
            }
            conditional_like(node, parent)
        }
        Kind::Identifier { name } => identifier_needs_parens(node, name, parent, tc),
        _ => false,
    }
}

fn sequence_needs_parens(node: &Node, parent: &Node) -> bool {
    match &parent.kind {
        Kind::SequenceExpression { .. } | Kind::TemplateLiteral { .. } => return false,
        Kind::MemberExpression(m) | Kind::OptionalMemberExpression(m) if is(&m.property, node) => {
            return false;
        }
        Kind::ClassDeclaration(_) => return true,
        Kind::ForOfStatement { right, .. } => return is(right, node),
        _ => {}
    }
    !parent.is_statement()
}

fn identifier_needs_parens(node: &Node, name: &str, parent: &Node, tc: u32) -> bool {
    if let Kind::AssignmentExpression(Binary { left, right, .. }) = &parent.kind
        && node.parenthesized
        && is(left, node)
    {
        let anonymous = match &right.kind {
            Kind::FunctionExpression(f) => f.id.is_none(),
            Kind::ClassExpression(c) => c.id.is_none(),
            _ => false,
        };
        if anonymous {
            return true;
        }
    }
    let head = TC_EXPRESSION_STATEMENT | TC_FOR_INIT_HEAD | TC_FOR_IN_HEAD;
    let member_parent = matches!(
        parent.kind,
        Kind::MemberExpression(_) | Kind::OptionalMemberExpression(_)
    );
    if (tc & TC_FOR_OF_HEAD != 0 || member_parent && tc & head != 0) && name == "let" {
        let followed_by_bracket = match &parent.kind {
            Kind::MemberExpression(m) => is(&m.object, node) && m.computed,
            Kind::OptionalMemberExpression(m) => is(&m.object, node) && m.computed && !m.optional,
            _ => false,
        };
        if followed_by_bracket && tc & head != 0 {
            return true;
        }
        return tc & TC_FOR_OF_HEAD != 0;
    }
    matches!(&parent.kind, Kind::ForOfStatement { left, is_await, .. }
        if is(left, node) && name == "async" && !is_await)
}
