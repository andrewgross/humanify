//! `@babel/generator` 7.29.7 (`printer.ts`, `buffer.ts`, `generators/*`,
//! `node/parentheses.ts`) for the two configurations the pipeline prints
//! with, both `compact: false`, no source maps, no `preserveFormat`, no
//! auxiliary comments, `comments: false`:
//!
//! - [`Mode::Beautify`] — stage 6 (`transformWithPlugins`, `retainLines:
//!   false`): `newline()` is real, so every statement of a `printSequence`
//!   and every property of an object literal lands on its own line, and a
//!   single-identifier arrow parameter prints without parentheses. With
//!   comments off there is never a blank line (the only `newline(2)` comes
//!   from a PRINTED comment's line offset), except inside template text.
//! - [`Mode::RetainLines`] — the `using` desugar (`retainLines: true`):
//!   `newline()` is a no-op; every line break comes from catching the
//!   buffer up to a node's `loc` line (at its start, and at the end for `}`
//!   / `)`), or from the newlines inside a multi-line token.
//!
//! The token state machine — the last-char codes (-1 after an append, -2
//! after an integer, -3 after a word), the queued space / semicolon, the
//! token context — is the generator's, byte for byte.
//!
//! Comments are never printed (`shouldPrintComment` is false for every
//! comment without `@license` / `@preserve`, and `format` prints those as
//! a file header instead — finding #46),
//! but ATTACHED comments still steer the output exactly where the
//! generator reads them without printing: a parenthesized expression with
//! a leading block comment keeps its parentheses; a newline-carrying
//! comment at a no-line-terminator position forces `(`…`)`; an arrow's lone
//! parameter with comments keeps its parentheses; an if-branch with
//! leading comments is printed one indent deeper
//! (`printAndIndentOnComments`).

use super::ast::{Binary, Call, Class, Func, Kind, Loc, Member, Method, NodeId, Prop, Quasi, Tree};
use super::jsesc::jsesc_double;

/// The configuration a [`Printer`] runs in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// `retainLines: false` (stage 6).
    Beautify,
    /// `retainLines: true` (the `using` desugar).
    RetainLines,
}

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

    /// `getNewlineCount()`.
    fn newline_count(&self) -> u32 {
        u32::from(self.queued == 0 && self.last == 10)
    }

    /// `hasContent()`.
    fn has_content(&self) -> bool {
        self.last != 0
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
    p.newline(1);
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

/// `commentIsNewline(c)`.
fn comment_is_newline(tree: &Tree, c: u32) -> bool {
    let c = &tree.comments[c as usize];
    !c.block || c.value.contains(['\n', '\r', '\u{2028}', '\u{2029}'])
}

/// One `print(node, …)` call's arguments beyond the node.
#[derive(Clone, Copy, Default)]
struct PrintOpts {
    parent: Option<NodeId>,
    nlta: bool,
    reset_tc: bool,
}

pub struct Printer<'t> {
    tree: &'t Tree,
    buf: Buffer,
    indent: usize,
    flags: u32,
    token_context: u32,
    no_line_terminator: bool,
    /// `_noLineTerminatorAfterNode` (identity only).
    nltan: Option<NodeId>,
    /// Plant (gate red runs): synthesized numbers through Rust's `{}`.
    rust_numbers: bool,
}

impl<'t> Printer<'t> {
    pub fn new(tree: &'t Tree, mode: Mode) -> Printer<'t> {
        Printer {
            tree,
            buf: Buffer::new(),
            indent: 0,
            flags: match mode {
                Mode::Beautify => 0,
                Mode::RetainLines => F_RETAIN_LINES,
            },
            token_context: 0,
            no_line_terminator: false,
            nltan: None,
            rust_numbers: false,
        }
    }

    /// Plant: print a synthesized number with Rust's `{}` instead of JS
    /// `Number::toString`.
    pub fn plant_rust_numbers(&mut self) {
        self.rust_numbers = true;
    }

    fn retain_lines(&self) -> bool {
        self.flags & F_RETAIN_LINES != 0
    }

    fn kind(&self, id: NodeId) -> &'t Kind {
        self.tree.kind(id)
    }

    /// `generate(file)`: print the Program (under a File root, its
    /// interpreter first) and take the buffer.
    pub fn generate(mut self, root: NodeId) -> String {
        let program = match self.kind(root) {
            Kind::File { program } => *program,
            _ => root,
        };
        if let Kind::Program {
            interpreter: Some(value),
            ..
        } = self.kind(program)
        {
            // InterpreterDirective: `#!…` then a hard newline.
            self.catch_up(1);
            self.token(&format!("#!{value}"), false, false);
            self.hard_newline();
        }
        self.print(program, PrintOpts::default());
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

    fn right_brace(&mut self, node: NodeId) {
        self.catch_up_end(self.tree.node(node).loc);
        self.token_char(b'}');
    }

    fn right_parens(&mut self, node: NodeId) {
        self.catch_up_end(self.tree.node(node).loc);
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

    /// `newline(i)`: a no-op under `retainLines`; otherwise at most one
    /// line break past the ones already at the end of the buffer.
    fn newline(&mut self, i: u32) {
        self.newline_with(i, self.flags);
    }

    fn newline_with(&mut self, i: u32, flags: u32) {
        if i == 0 {
            return;
        }
        if flags & (F_RETAIN_LINES | 2) != 0 {
            return;
        }
        if flags & F_CONCISE != 0 {
            self.space();
            return;
        }
        let i = i.min(2).saturating_sub(self.buf.newline_count());
        for _ in 0..i {
            self.hard_newline();
        }
    }

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

    /// `catchUp(line)`: only under `retainLines`.
    fn catch_up(&mut self, line: u32) {
        if !self.retain_lines() {
            return;
        }
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

    fn enter_delimited(&mut self) -> Option<NodeId> {
        let old = self.nltan;
        if old.is_some() {
            self.nltan = None;
        }
        old
    }

    // -- print / printJoin -------------------------------------------------------

    fn print_opt(&mut self, node: Option<NodeId>, parent: NodeId) {
        if let Some(n) = node {
            self.p(n, parent);
        }
    }

    fn p(&mut self, node: NodeId, parent: NodeId) {
        self.print(
            node,
            PrintOpts {
                parent: Some(parent),
                ..PrintOpts::default()
            },
        );
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

    /// The leading comments' effect on parentheses when nothing else asks
    /// for them: a parenthesized node whose first leading comment is a
    /// block keeps its parentheses, except in the four parent positions
    /// that are delimited already.
    fn leading_comment_parens(&self, node: NodeId, parent: Option<NodeId>) -> bool {
        let n = self.tree.node(node);
        if !n.parenthesized {
            return false;
        }
        let Some(first) = n.comments.as_ref().and_then(|c| c.leading.first()) else {
            return false;
        };
        if !self.tree.comments[*first as usize].block {
            return false;
        }
        let Some(parent) = parent else {
            return true;
        };
        match self.kind(parent) {
            Kind::ExpressionStatement { .. }
            | Kind::VariableDeclarator { .. }
            | Kind::AssignmentExpression(_)
            | Kind::ReturnStatement { .. } => false,
            Kind::CallExpression(c) | Kind::OptionalCallExpression(c) | Kind::NewExpression(c) => {
                c.callee == node
            }
            _ => true,
        }
    }

    /// Whether `node` prints inside parentheses, and whether they are the
    /// indented kind (a no-line-terminator position whose node would start
    /// on a later line — `retainLines` — or carries a newline comment).
    fn parens_for(&self, node: NodeId, parent: Option<NodeId>, flags: u32) -> (bool, bool) {
        let needed = parent.is_some_and(|p| {
            parent_needs_parens(self.tree, node, p)
                || needs_parens(self.tree, node, p, self.token_context)
        });
        if needed || self.leading_comment_parens(node, parent) {
            return (true, false);
        }
        if !self.no_line_terminator {
            return (false, false);
        }
        let n = self.tree.node(node);
        let newline_comment = n
            .comments
            .as_ref()
            .is_some_and(|c| c.leading.iter().any(|&c| comment_is_newline(self.tree, c)));
        let later_line =
            flags & F_RETAIN_LINES != 0 && n.loc.is_some_and(|l| l.start > self.buf.line);
        let indented = newline_comment || later_line;
        (indented, indented)
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

    fn trailing_newline_comment(&self, node: NodeId) -> bool {
        self.tree
            .node(node)
            .comments
            .as_ref()
            .is_some_and(|c| c.trailing.iter().any(|&c| comment_is_newline(self.tree, c)))
    }

    fn print(&mut self, node: NodeId, opts: PrintOpts) {
        let PrintOpts {
            parent,
            nlta,
            reset_tc,
        } = opts;
        let flags = self.flags;
        if self.tree.node(node).compact {
            self.flags |= F_CONCISE;
        }
        let mut old_tc = self.reset_token_context(reset_tc);
        let (mut should_parens, indent_parenthesized) = self.parens_for(node, parent, flags);
        // `undefined` / a saved value (restored only when non-null).
        let mut saved_nltan: Option<Option<NodeId>> = None;
        let mut nlta = nlta;
        if !should_parens {
            nlta = nlta
                || parent
                    .is_some_and(|p| self.nltan == Some(p) && self.tree.is_last_child(p, node));
            if nlta {
                if self.trailing_newline_comment(node) {
                    if self.kind(node).is_expression() {
                        should_parens = true;
                    }
                } else {
                    saved_nltan = Some(self.nltan);
                    self.nltan = Some(node);
                }
            }
        }
        if should_parens {
            old_tc = self.open_parens(indent_parenthesized, reset_tc, old_tc);
            saved_nltan = Some(self.nltan);
            self.nltan = None;
        }
        if !matches!(self.kind(node), Kind::Program { .. })
            && let Some(loc) = self.tree.node(node).loc
        {
            self.catch_up(loc.start);
        }
        self.print_method(node, parent);
        if should_parens {
            if indent_parenthesized {
                self.dedent_with(self.flags);
                self.newline(1);
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

    /// `printJoin(nodes, statement, indent, separator,
    /// printTrailingSeparator, resetTokenContext)`.
    #[allow(clippy::too_many_arguments)]
    fn print_join(
        &mut self,
        nodes: &[NodeId],
        parent: NodeId,
        statement: bool,
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
            && let Some(loc) = self.tree.node(nodes[0]).loc
            && loc.start != self.buf.line
        {
            indent = Some(true);
        }
        let indent = indent == Some(true);
        if indent {
            self.indent_with(flags);
        }
        let len = nodes.len();
        for (i, &node) in nodes.iter().enumerate() {
            if node.is_none() {
                continue;
            }
            if statement && i == 0 && self.buf.has_content() {
                self.newline_with(1, flags);
            }
            self.print(
                node,
                PrintOpts {
                    parent: Some(parent),
                    nlta: false,
                    reset_tc,
                },
            );
            if let Some(sep) = separator {
                if i < len - 1 {
                    sep(self, false);
                } else if print_trailing_separator {
                    sep(self, true);
                }
            }
            if statement {
                self.newline_with(1, flags);
            }
        }
        if indent {
            self.dedent_with(flags);
        }
    }

    /// `printSequence(nodes, indent)`: statements, one per line.
    fn print_sequence(&mut self, nodes: &[NodeId], parent: NodeId, indent: bool, reset_tc: bool) {
        self.print_join(nodes, parent, true, Some(indent), None, false, reset_tc);
    }

    /// `printList(items)` with the default comma separator.
    fn print_list(&mut self, items: &[NodeId], parent: NodeId, reset_tc: bool) {
        self.print_join(
            items,
            parent,
            false,
            None,
            Some(comma_separator),
            false,
            reset_tc,
        );
    }

    fn print_block(&mut self, body: NodeId, parent: NodeId) {
        if !matches!(self.kind(body), Kind::EmptyStatement) {
            self.space();
        }
        self.p(body, parent);
    }

    /// `printAndIndentOnComments(node)`.
    fn print_and_indent_on_comments(&mut self, node: NodeId, parent: NodeId) {
        let indent = self
            .tree
            .node(node)
            .comments
            .as_ref()
            .is_some_and(|c| !c.leading.is_empty());
        if indent {
            self.indent_with(self.flags);
        }
        self.p(node, parent);
        if indent {
            self.dedent_with(self.flags);
        }
    }

    // -- the node printers (generators/*) ---------------------------------------

    fn print_method(&mut self, node: NodeId, parent: Option<NodeId>) {
        let kind = self.kind(node);
        match kind {
            Kind::File { program } => self.p(*program, node),
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
            | Kind::LabeledStatement { .. } => self.print_statement(node),
            Kind::IfStatement { .. }
            | Kind::SwitchStatement { .. }
            | Kind::SwitchCase { .. }
            | Kind::TryStatement { .. }
            | Kind::CatchClause { .. }
            | Kind::WhileStatement { .. }
            | Kind::DoWhileStatement { .. } => self.print_control(node),
            Kind::ForStatement { .. }
            | Kind::ForInStatement { .. }
            | Kind::ForOfStatement { .. } => self.print_loop(node),
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
            | Kind::Import => self.print_atom(node),
            Kind::ArrayExpression { .. }
            | Kind::ArrayPattern { .. }
            | Kind::ObjectExpression { .. }
            | Kind::ObjectPattern { .. }
            | Kind::ObjectProperty { .. }
            | Kind::SpreadElement { .. }
            | Kind::RestElement { .. } => self.print_collection(node),
            Kind::UnaryExpression { .. }
            | Kind::UpdateExpression { .. }
            | Kind::BinaryExpression { .. }
            | Kind::LogicalExpression { .. }
            | Kind::AssignmentExpression { .. }
            | Kind::AssignmentPattern { .. }
            | Kind::ConditionalExpression { .. } => self.print_operator(node),
            Kind::CallExpression { .. }
            | Kind::OptionalCallExpression { .. }
            | Kind::NewExpression { .. }
            | Kind::MemberExpression { .. }
            | Kind::OptionalMemberExpression { .. }
            | Kind::SequenceExpression { .. }
            | Kind::YieldExpression { .. }
            | Kind::AwaitExpression { .. }
            | Kind::MetaProperty { .. } => self.print_access(node),
            Kind::ImportDeclaration { .. }
            | Kind::ImportSpecifier { .. }
            | Kind::ImportDefaultSpecifier { .. }
            | Kind::ImportNamespaceSpecifier { .. }
            | Kind::ImportAttribute { .. }
            | Kind::ExportNamedDeclaration { .. }
            | Kind::ExportDefaultDeclaration { .. }
            | Kind::ExportAllDeclaration { .. }
            | Kind::ExportSpecifier { .. }
            | Kind::ExportNamespaceSpecifier { .. } => self.print_module(node),
        }
    }

    /// `Program` / `BlockStatement`'s directives + body.
    fn directives_and_body(
        &mut self,
        node: NodeId,
        directives: &[NodeId],
        body: &[NodeId],
        block: bool,
    ) {
        if !directives.is_empty() {
            let newline = if body.is_empty() { 1 } else { 2 };
            self.print_sequence(directives, node, block, block);
            let last = *directives.last().expect("non-empty");
            let has_trailing = self
                .tree
                .node(last)
                .comments
                .as_ref()
                .is_some_and(|c| !c.trailing.is_empty());
            if !has_trailing {
                self.newline(newline);
            }
        }
        // BlockStatement's printSequence resets a for-head's token context
        // (`printSequence(body, true, true)`); Program's does not.
        self.print_sequence(body, node, block, block);
    }

    fn print_statement(&mut self, node: NodeId) {
        match self.kind(node) {
            Kind::Program {
                directives, body, ..
            } => self.directives_and_body(node, directives, body, false),
            Kind::Directive { value } => {
                self.p(*value, node);
                self.semicolon(false);
            }
            Kind::DirectiveLiteral { raw } => self.token(raw, false, false),
            Kind::BlockStatement { directives, body } => {
                self.token_char(b'{');
                let old = self.enter_delimited();
                self.directives_and_body(node, directives, body, true);
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
                    self.newline(1);
                    self.print_sequence(body, node, true, false);
                    self.right_brace(node);
                }
            }
            Kind::ExpressionStatement { expression } => {
                self.token_context |= TC_EXPRESSION_STATEMENT;
                self.p(*expression, node);
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
                self.p(*object, node);
                self.token_char(b')');
                self.print_block(*body, node);
            }
            Kind::ReturnStatement { argument } => {
                self.word("return");
                self.after_keyword(*argument, node);
            }
            Kind::ThrowStatement { argument } => {
                self.word("throw");
                self.after_keyword(Some(*argument), node);
            }
            Kind::BreakStatement { label } => {
                self.word("break");
                self.after_keyword(*label, node);
            }
            Kind::ContinueStatement { label } => {
                self.word("continue");
                self.after_keyword(*label, node);
            }
            Kind::LabeledStatement { label, body } => {
                self.p(*label, node);
                self.token_char(b':');
                self.space();
                self.p(*body, node);
            }
            _ => unreachable!("dispatched by print_method"),
        }
    }

    fn print_control(&mut self, node: NodeId) {
        match self.kind(node) {
            Kind::IfStatement {
                test,
                consequent,
                alternate,
            } => self.if_statement(node, *test, *consequent, *alternate),
            Kind::SwitchStatement {
                discriminant,
                cases,
            } => {
                self.word("switch");
                self.space();
                self.token_char(b'(');
                self.p(*discriminant, node);
                self.token_char(b')');
                self.space();
                self.token_char(b'{');
                self.print_sequence(cases, node, true, false);
                self.right_brace(node);
            }
            Kind::SwitchCase { test, consequent } => {
                match test {
                    Some(t) => {
                        self.word("case");
                        self.space();
                        self.p(*t, node);
                        self.token_char(b':');
                    }
                    None => {
                        self.word("default");
                        self.token_char(b':');
                    }
                }
                if !consequent.is_empty() {
                    self.newline(1);
                    self.print_sequence(consequent, node, true, false);
                }
            }
            Kind::TryStatement {
                block,
                handler,
                finalizer,
            } => {
                self.word("try");
                self.space();
                self.p(*block, node);
                self.space();
                self.print_opt(*handler, node);
                if let Some(f) = finalizer {
                    self.space();
                    self.word("finally");
                    self.space();
                    self.p(*f, node);
                }
            }
            Kind::CatchClause { param, body } => {
                self.word("catch");
                self.space();
                if let Some(param) = param {
                    self.token_char(b'(');
                    self.p(*param, node);
                    self.token_char(b')');
                    self.space();
                }
                self.p(*body, node);
            }
            Kind::WhileStatement { test, body } => {
                self.word("while");
                self.space();
                self.token_char(b'(');
                self.p(*test, node);
                self.token_char(b')');
                self.print_block(*body, node);
            }
            Kind::DoWhileStatement { body, test } => {
                self.word("do");
                self.space();
                self.p(*body, node);
                self.space();
                self.word("while");
                self.space();
                self.token_char(b'(');
                self.p(*test, node);
                self.token_char(b')');
                self.semicolon(false);
            }
            _ => unreachable!("dispatched by print_method"),
        }
    }

    fn print_loop(&mut self, node: NodeId) {
        match self.kind(node) {
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
                self.print_opt(*init, node);
                self.token_context = 0;
                self.token_char(b';');
                if let Some(t) = test {
                    self.space();
                    self.p(*t, node);
                }
                self.token_char(b';');
                if let Some(u) = update {
                    self.space();
                    self.p(*u, node);
                }
                self.token_char(b')');
                self.print_block(*body, node);
            }
            Kind::ForInStatement { left, right, body } => {
                self.word("for");
                self.space();
                self.token_char(b'(');
                self.token_context |= TC_FOR_IN_HEAD | TC_ACCUMULATE;
                self.p(*left, node);
                self.token_context = 0;
                self.space();
                self.word("in");
                self.space();
                self.p(*right, node);
                self.token_char(b')');
                self.print_block(*body, node);
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
                self.p(*left, node);
                self.space();
                self.word("of");
                self.space();
                self.p(*right, node);
                self.token_char(b')');
                self.print_block(*body, node);
            }
            _ => unreachable!("dispatched by print_method"),
        }
    }

    fn print_declaration(&mut self, node: NodeId, parent: Option<NodeId>) {
        match self.kind(node) {
            Kind::FunctionDeclaration(f) | Kind::FunctionExpression(f) => {
                self.function_head(f, node);
                self.space();
                self.p(f.body, node);
            }
            Kind::ArrowFunctionExpression(f) => self.arrow(f, node),
            Kind::VariableDeclaration { kind, declarations } => {
                self.variable_declaration(node, kind, declarations, parent)
            }
            Kind::VariableDeclarator { id, init } => {
                self.p(*id, node);
                if let Some(init) = init {
                    self.space();
                    self.token_char(b'=');
                    self.space();
                    self.p(*init, node);
                }
            }
            Kind::ClassDeclaration(c) | Kind::ClassExpression(c) => self.class(c, node),
            Kind::ClassBody { body } => {
                self.token_char(b'{');
                if body.is_empty() {
                    self.token_char(b'}');
                } else {
                    let old = self.enter_delimited();
                    self.print_join(body, node, true, Some(true), None, true, true);
                    self.nltan = old;
                    if !self.ends_with(10) {
                        self.newline(1);
                    }
                    self.right_brace(node);
                }
            }
            Kind::ClassMethod(m) | Kind::ClassPrivateMethod(m) => {
                if let Some(l) = self.tree.node(m.key).loc {
                    self.catch_up(l.end);
                }
                if m.is_static {
                    self.word("static");
                    self.space();
                }
                self.method_head(m, node);
                self.space();
                self.p(m.func.body, node);
            }
            Kind::ObjectMethod(m) => {
                self.method_head(m, node);
                self.space();
                self.p(m.func.body, node);
            }
            Kind::ClassProperty(fd) => {
                if !fd.is_static
                    && let Some(l) = self.tree.node(fd.key).loc
                {
                    self.catch_up(l.end);
                }
                self.field(fd, node, false);
            }
            Kind::ClassPrivateProperty(fd) => self.field(fd, node, false),
            Kind::ClassAccessorProperty(fd) => {
                if let Some(l) = self.tree.node(fd.key).loc {
                    self.catch_up(l.end);
                }
                self.field(fd, node, true);
            }
            _ => unreachable!("dispatched by print_method"),
        }
    }

    fn template(&mut self, node: NodeId, quasis: &[Quasi], expressions: &[NodeId]) {
        let mut part = String::from("`");
        for i in 0..quasis.len().saturating_sub(1) {
            part.push_str(&quasis[i].raw);
            part.push_str("${");
            self.token(&part, true, false);
            self.p(expressions[i], node);
            part = String::from("}");
        }
        if let Some(last) = quasis.last() {
            part.push_str(&last.raw);
        }
        part.push('`');
        self.token(&part, true, false);
    }

    fn print_atom(&mut self, node: NodeId) {
        match self.kind(node) {
            Kind::Identifier { name } => self.word(name),
            Kind::PrivateName { id } => {
                self.token_char(b'#');
                self.p(*id, node);
            }
            Kind::StringLiteral { value, raw } => match raw {
                Some(raw) => self.token(raw, false, false),
                None => {
                    let v = jsesc_double(value);
                    self.token(&v, false, false);
                }
            },
            Kind::NumericLiteral { value, raw } => {
                let s = match raw {
                    Some(r) => r.clone(),
                    None if self.rust_numbers => format!("{value}"),
                    None => humanify_model::js::number_to_string(*value),
                };
                self.number(&s, *value);
            }
            Kind::BigIntLiteral { raw } => self.word(raw),
            Kind::BooleanLiteral { value } => self.word(if *value { "true" } else { "false" }),
            Kind::NullLiteral => self.word("null"),
            Kind::RegExpLiteral { text } => self.word(text),
            Kind::TemplateLiteral {
                quasis,
                expressions,
            } => self.template(node, quasis, expressions),
            Kind::TaggedTemplateExpression { tag, quasi } => {
                self.p(*tag, node);
                self.p(*quasi, node);
            }
            Kind::ThisExpression => self.word("this"),
            Kind::Super => self.word("super"),
            Kind::Import => self.word("import"),
            _ => unreachable!("dispatched by print_method"),
        }
    }

    fn print_collection(&mut self, node: NodeId) {
        match self.kind(node) {
            Kind::ArrayExpression { elements } | Kind::ArrayPattern { elements } => {
                self.token_char(b'[');
                let old = self.enter_delimited();
                let len = elements.len();
                for (i, &el) in elements.iter().enumerate() {
                    if el.is_none() {
                        self.token_char(b',');
                        continue;
                    }
                    if i > 0 {
                        self.space();
                    }
                    self.print(
                        el,
                        PrintOpts {
                            parent: Some(node),
                            nlta: false,
                            reset_tc: true,
                        },
                    );
                    if i < len - 1 {
                        self.token_char(b',');
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
                        true,
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
            } => self.object_property(node, *key, *value, *computed, *shorthand),
            Kind::SpreadElement { argument } | Kind::RestElement { argument } => {
                self.token("...", false, false);
                self.p(*argument, node);
            }
            _ => unreachable!("dispatched by print_method"),
        }
    }

    fn object_property(
        &mut self,
        node: NodeId,
        key: NodeId,
        value: NodeId,
        computed: bool,
        shorthand: bool,
    ) {
        if computed {
            self.token_char(b'[');
            self.p(key, node);
            self.token_char(b']');
        } else {
            let key_name = self.kind(key).identifier_name();
            if let (Kind::AssignmentPattern { left, .. }, Some(k)) = (self.kind(value), key_name)
                && self.kind(*left).identifier_name() == Some(k)
            {
                self.p(value, node);
                return;
            }
            self.p(key, node);
            if shorthand
                && let (Some(k), Some(v)) = (key_name, self.kind(value).identifier_name())
                && k == v
            {
                return;
            }
        }
        self.token_char(b':');
        self.space();
        self.p(value, node);
    }

    fn print_operator(&mut self, node: NodeId) {
        match self.kind(node) {
            Kind::UnaryExpression { operator, argument } => {
                let first = operator.as_bytes()[0];
                if first.is_ascii_lowercase() {
                    self.word(operator);
                    self.space();
                } else {
                    self.token_char(first);
                }
                self.p(*argument, node);
            }
            Kind::UpdateExpression {
                operator,
                prefix,
                argument,
            } => {
                if *prefix {
                    self.token(operator, false, true);
                    self.p(*argument, node);
                } else {
                    self.print(
                        *argument,
                        PrintOpts {
                            parent: Some(node),
                            nlta: true,
                            reset_tc: false,
                        },
                    );
                    self.token(operator, false, true);
                }
            }
            Kind::BinaryExpression(bx) => {
                self.p(bx.left, node);
                self.space();
                if bx.operator.starts_with('i') {
                    self.word(bx.operator);
                } else {
                    self.token(bx.operator, false, true);
                    self.buf.last = i32::from(*bx.operator.as_bytes().last().expect("operator"));
                }
                self.space();
                self.p(bx.right, node);
            }
            Kind::LogicalExpression(bx) | Kind::AssignmentExpression(bx) => {
                self.p(bx.left, node);
                self.space();
                self.token(bx.operator, false, true);
                self.space();
                self.p(bx.right, node);
            }
            Kind::AssignmentPattern { left, right } => {
                self.p(*left, node);
                self.space();
                self.token_char(b'=');
                self.space();
                self.p(*right, node);
            }
            Kind::ConditionalExpression {
                test,
                consequent,
                alternate,
            } => {
                self.p(*test, node);
                self.space();
                self.token_char(b'?');
                self.space();
                self.p(*consequent, node);
                self.space();
                self.token_char(b':');
                self.space();
                self.p(*alternate, node);
            }
            _ => unreachable!("dispatched by print_method"),
        }
    }

    fn print_access(&mut self, node: NodeId) {
        match self.kind(node) {
            Kind::CallExpression(c) => {
                self.p(c.callee, node);
                self.call_arguments(c, node);
            }
            Kind::OptionalCallExpression(c) => {
                self.p(c.callee, node);
                if c.optional {
                    self.token("?.", false, false);
                }
                self.call_arguments(c, node);
            }
            Kind::NewExpression(c) => {
                self.word("new");
                self.space();
                self.p(c.callee, node);
                self.call_arguments(c, node);
            }
            Kind::MemberExpression(m) => self.member(m, node, false),
            Kind::OptionalMemberExpression(m) => self.member(m, node, true),
            Kind::SequenceExpression { expressions } => {
                self.print_list(expressions, node, false);
            }
            Kind::YieldExpression { argument, delegate } => {
                if *delegate {
                    self.word_nlt("yield", true);
                    self.token_char(b'*');
                    if let Some(a) = argument {
                        self.space();
                        self.p(*a, node);
                    }
                } else if let Some(a) = argument {
                    self.word_nlt("yield", true);
                    self.space();
                    self.p(*a, node);
                } else {
                    self.word("yield");
                }
            }
            Kind::AwaitExpression { argument } => {
                self.word("await");
                self.space();
                self.p(*argument, node);
            }
            Kind::MetaProperty { meta, property } => {
                self.p(*meta, node);
                self.token_char(b'.');
                self.p(*property, node);
            }
            _ => unreachable!("dispatched by print_method"),
        }
    }

    fn print_module(&mut self, node: NodeId) {
        match self.kind(node) {
            Kind::ImportDeclaration {
                specifiers,
                source,
                attributes,
                phase,
            } => self.import_declaration(node, specifiers, *source, attributes, *phase),
            Kind::ImportSpecifier { imported, local } => {
                self.p(*imported, node);
                if self.kind(*local).identifier_name() != self.kind(*imported).identifier_name() {
                    self.space();
                    self.word("as");
                    self.space();
                    self.p(*local, node);
                }
            }
            Kind::ImportDefaultSpecifier { local } => self.p(*local, node),
            Kind::ImportNamespaceSpecifier { local } => {
                self.token_char(b'*');
                self.space();
                self.word("as");
                self.space();
                self.p(*local, node);
            }
            Kind::ImportAttribute { key, value } => {
                self.p(*key, node);
                self.token_char(b':');
                self.space();
                self.p(*value, node);
            }
            Kind::ExportNamedDeclaration {
                declaration,
                specifiers,
                source,
                attributes,
            } => self.export_named(node, *declaration, specifiers, *source, attributes),
            Kind::ExportDefaultDeclaration { declaration } => {
                self.word("export");
                self.space();
                self.word("default");
                self.space();
                self.token_context |= TC_EXPORT_DEFAULT;
                self.p(*declaration, node);
                if !self.kind(*declaration).is_statement() {
                    self.semicolon(false);
                }
            }
            Kind::ExportAllDeclaration { source, attributes } => {
                self.word("export");
                self.space();
                self.token_char(b'*');
                self.space();
                self.word("from");
                self.space();
                self.module_source(node, *source, attributes, false);
                self.semicolon(false);
            }
            Kind::ExportSpecifier { local, exported } => {
                self.p(*local, node);
                if self.kind(*local).identifier_name() != self.kind(*exported).identifier_name() {
                    self.space();
                    self.word("as");
                    self.space();
                    self.p(*exported, node);
                }
            }
            Kind::ExportNamespaceSpecifier { exported } => {
                self.token_char(b'*');
                self.space();
                self.word("as");
                self.space();
                self.p(*exported, node);
            }
            _ => unreachable!("dispatched by print_method"),
        }
    }

    /// A module source, then its `with { … }` attributes when it has any.
    fn module_source(&mut self, node: NodeId, source: NodeId, attributes: &[NodeId], _brace: bool) {
        if attributes.is_empty() {
            self.p(source, node);
            return;
        }
        self.print(
            source,
            PrintOpts {
                parent: Some(node),
                nlta: true,
                reset_tc: false,
            },
        );
        self.space();
        self.word("with");
        self.space();
        self.token("{", false, false);
        self.space();
        self.print_list(attributes, node, false);
        self.space();
        self.token("}", false, false);
    }

    fn import_declaration(
        &mut self,
        node: NodeId,
        specifiers: &[NodeId],
        source: NodeId,
        attributes: &[NodeId],
        phase: Option<&'static str>,
    ) {
        self.word("import");
        self.space();
        if let Some(phase) = phase {
            self.word(phase);
            self.space();
        }
        let has_specifiers = !specifiers.is_empty();
        let mut rest = specifiers;
        while let Some((&first, tail)) = rest.split_first() {
            if !matches!(
                self.kind(first),
                Kind::ImportDefaultSpecifier { .. } | Kind::ImportNamespaceSpecifier { .. }
            ) {
                break;
            }
            self.p(first, node);
            rest = tail;
            if !rest.is_empty() {
                self.token_char(b',');
                self.space();
            }
        }
        let has_brace = !rest.is_empty();
        if has_brace {
            self.token_char(b'{');
            self.space();
            self.print_list(rest, node, false);
            self.space();
            self.token_char(b'}');
        }
        if has_specifiers {
            self.space();
            self.word("from");
            self.space();
        }
        self.module_source(node, source, attributes, has_brace);
        self.semicolon(false);
    }

    fn export_named(
        &mut self,
        node: NodeId,
        declaration: Option<NodeId>,
        specifiers: &[NodeId],
        source: Option<NodeId>,
        attributes: &[NodeId],
    ) {
        self.word("export");
        self.space();
        if let Some(declar) = declaration {
            self.p(declar, node);
            if !self.kind(declar).is_statement() {
                self.semicolon(false);
            }
            return;
        }
        let mut rest = specifiers;
        let mut has_special = false;
        while let Some((&first, tail)) = rest.split_first() {
            if !matches!(self.kind(first), Kind::ExportNamespaceSpecifier { .. }) {
                break;
            }
            has_special = true;
            self.p(first, node);
            rest = tail;
            if !rest.is_empty() {
                self.token_char(b',');
                self.space();
            }
        }
        let mut has_brace = false;
        if !rest.is_empty() || !has_special {
            has_brace = true;
            self.token_char(b'{');
            if !rest.is_empty() {
                self.space();
                self.print_list(rest, node, false);
                self.space();
            }
            self.token_char(b'}');
        }
        if let Some(source) = source {
            self.space();
            self.word("from");
            self.space();
            self.module_source(node, source, attributes, has_brace);
        }
        self.semicolon(false);
    }

    fn after_keyword(&mut self, arg: Option<NodeId>, parent: NodeId) {
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
        node: NodeId,
        test: NodeId,
        consequent: NodeId,
        alternate: Option<NodeId>,
    ) {
        self.word("if");
        self.space();
        self.token_char(b'(');
        self.p(test, node);
        self.token_char(b')');
        self.space();
        let needs_block = alternate.is_some()
            && matches!(
                self.kind(last_statement(self.tree, consequent)),
                Kind::IfStatement { .. }
            );
        if needs_block {
            self.token_char(b'{');
            self.newline(1);
            self.indent_with(self.flags);
        }
        self.print_and_indent_on_comments(consequent, node);
        if needs_block {
            self.dedent_with(self.flags);
            self.newline(1);
            self.token_char(b'}');
        }
        if let Some(alt) = alternate {
            if self.ends_with(125) {
                self.space();
            }
            self.word("else");
            self.space();
            self.print_and_indent_on_comments(alt, node);
        }
    }

    fn variable_declaration(
        &mut self,
        node: NodeId,
        kind: &str,
        declarations: &[NodeId],
        parent: Option<NodeId>,
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
                self.kind(p),
                Kind::ForStatement { .. }
                    | Kind::ForInStatement { .. }
                    | Kind::ForOfStatement { .. }
            )
        });
        let has_inits = !parent_is_for
            && declarations
                .iter()
                .any(|&d| matches!(self.kind(d), Kind::VariableDeclarator { init: Some(_), .. }));
        self.print_join(
            declarations,
            node,
            false,
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
            match self.kind(p) {
                Kind::ForStatement { init: Some(i), .. } if *i == node => return,
                Kind::ForInStatement { left, .. } | Kind::ForOfStatement { left, .. }
                    if *left == node =>
                {
                    return;
                }
                _ => {}
            }
        }
        self.semicolon(false);
    }

    /// `_parameters(params, ")")` after the `(`.
    fn params(&mut self, params: &[NodeId], node: NodeId) {
        self.token_char(b'(');
        let old = self.enter_delimited();
        let len = params.len();
        for (i, &p) in params.iter().enumerate() {
            self.print(
                p,
                PrintOpts {
                    parent: Some(node),
                    nlta: false,
                    reset_tc: true,
                },
            );
            if i < len - 1 {
                self.token_char(b',');
                self.space();
            }
        }
        self.token_char(b')');
        self.nltan = old;
    }

    /// `_shouldPrintArrowParamsParens`.
    fn arrow_params_need_parens(&self, f: &Func) -> bool {
        if f.params.len() != 1 {
            return true;
        }
        let first = f.params[0];
        let has_comments = self
            .tree
            .node(first)
            .comments
            .as_ref()
            .is_some_and(|c| !c.leading.is_empty() || !c.trailing.is_empty());
        if !matches!(self.kind(first), Kind::Identifier { .. }) || has_comments {
            return true;
        }
        self.retain_lines()
    }

    fn arrow(&mut self, f: &Func, node: NodeId) {
        if f.is_async {
            self.word_nlt("async", true);
            self.space();
        }
        if self.arrow_params_need_parens(f) {
            self.params(&f.params, node);
            self.no_line_terminator = true;
        } else {
            self.print(
                f.params[0],
                PrintOpts {
                    parent: Some(node),
                    nlta: true,
                    reset_tc: false,
                },
            );
        }
        self.space();
        self.token("=>", false, false);
        self.space();
        self.token_context |= TC_ARROW_BODY;
        self.p(f.body, node);
    }

    fn function_head(&mut self, f: &Func, node: NodeId) {
        if f.is_async {
            self.word("async");
            self.space();
        }
        self.word("function");
        if f.generator {
            self.token_char(b'*');
        }
        self.space();
        self.print_opt(f.id, node);
        self.params(&f.params, node);
        self.no_line_terminator = false;
    }

    fn method_head(&mut self, m: &Method, node: NodeId) {
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
            self.p(m.key, node);
            self.token_char(b']');
        } else {
            self.p(m.key, node);
        }
        self.params(&m.func.params, node);
        self.no_line_terminator = false;
    }

    fn field(&mut self, fd: &Prop, node: NodeId, accessor: bool) {
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
            self.p(fd.key, node);
            self.token_char(b']');
        } else {
            self.p(fd.key, node);
        }
        if let Some(v) = fd.value {
            self.space();
            self.token_char(b'=');
            self.space();
            self.p(v, node);
        }
        self.semicolon(false);
    }

    fn class(&mut self, c: &Class, node: NodeId) {
        self.word("class");
        if let Some(id) = c.id {
            self.space();
            self.p(id, node);
        }
        if let Some(sc) = c.super_class {
            self.space();
            self.word("extends");
            self.space();
            self.p(sc, node);
        }
        self.space();
        self.p(c.body, node);
    }

    fn call_arguments(&mut self, c: &Call, node: NodeId) {
        self.token_char(b'(');
        let old = self.enter_delimited();
        self.print_list(&c.arguments, node, true);
        self.nltan = old;
        self.right_parens(node);
    }

    fn member(&mut self, m: &Member, node: NodeId, optional_type: bool) {
        self.p(m.object, node);
        let mut computed = m.computed;
        if matches!(self.kind(m.property), Kind::NumericLiteral { .. }) {
            computed = true;
        }
        if optional_type {
            if m.optional {
                self.token("?.", false, false);
            }
            if computed {
                self.token_char(b'[');
                self.p(m.property, node);
                self.token_char(b']');
            } else {
                if !m.optional {
                    self.token_char(b'.');
                }
                self.p(m.property, node);
            }
        } else if computed {
            let old = self.enter_delimited();
            self.token_char(b'[');
            self.print(
                m.property,
                PrintOpts {
                    parent: Some(node),
                    nlta: false,
                    reset_tc: true,
                },
            );
            self.token_char(b']');
            self.nltan = old;
        } else {
            self.token_char(b'.');
            self.p(m.property, node);
        }
    }
}

/// `getLastStatement` (statements.ts): follow `.body` while it is a
/// statement node.
fn last_statement(tree: &Tree, stmt: NodeId) -> NodeId {
    let body = match tree.kind(stmt) {
        Kind::WithStatement { body, .. }
        | Kind::LabeledStatement { body, .. }
        | Kind::WhileStatement { body, .. }
        | Kind::DoWhileStatement { body, .. }
        | Kind::ForStatement { body, .. }
        | Kind::ForInStatement { body, .. }
        | Kind::ForOfStatement { body, .. } => *body,
        _ => return stmt,
    };
    if tree.kind(body).is_statement() {
        last_statement(tree, body)
    } else {
        stmt
    }
}

// -- parentheses (node/index.ts + node/parentheses.ts) ---------------------------

fn is_or_has_call_expression(tree: &Tree, node: NodeId) -> bool {
    match tree.kind(node) {
        Kind::CallExpression(_) => true,
        Kind::MemberExpression(m) => is_or_has_call_expression(tree, m.object),
        _ => false,
    }
}

/// `parentNeedsParens`.
fn parent_needs_parens(tree: &Tree, node: NodeId, parent: NodeId) -> bool {
    matches!(tree.kind(parent), Kind::NewExpression(c) if c.callee == node)
        && is_or_has_call_expression(tree, node)
}

fn is_class_extends_clause(tree: &Tree, node: NodeId, parent: NodeId) -> bool {
    matches!(tree.kind(parent), Kind::ClassDeclaration(c) | Kind::ClassExpression(c)
        if c.super_class == Some(node))
}

fn has_postfix_part(tree: &Tree, node: NodeId, parent: NodeId) -> bool {
    match tree.kind(parent) {
        Kind::MemberExpression(m) | Kind::OptionalMemberExpression(m) => m.object == node,
        Kind::CallExpression(c) | Kind::OptionalCallExpression(c) | Kind::NewExpression(c) => {
            c.callee == node
        }
        Kind::TaggedTemplateExpression { tag, .. } => *tag == node,
        _ => false,
    }
}

fn binary_like(tree: &Tree, node: NodeId, parent: NodeId, op: &str, logical: bool) -> bool {
    if is_class_extends_clause(tree, node, parent) {
        return true;
    }
    if has_postfix_part(tree, node, parent)
        || matches!(
            tree.kind(parent),
            Kind::UnaryExpression { .. }
                | Kind::SpreadElement { .. }
                | Kind::AwaitExpression { .. }
        )
    {
        return true;
    }
    let (parent_pos, parent_binary): (Option<u32>, Option<&Binary>) = match tree.kind(parent) {
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
                pb.left == node
            } else {
                pb.right == node
            })
        {
            return true;
        }
        if logical
            && matches!(tree.kind(parent), Kind::LogicalExpression(_))
            && ((node_pos == 1 && parent_pos != 1) || (parent_pos == 1 && node_pos != 1))
        {
            return true;
        }
    }
    false
}

fn unary_like(tree: &Tree, node: NodeId, parent: NodeId) -> bool {
    has_postfix_part(tree, node, parent)
        || matches!(tree.kind(parent), Kind::BinaryExpression(b) if b.operator == "**" && b.left == node)
        || is_class_extends_clause(tree, node, parent)
}

fn conditional_like(tree: &Tree, node: NodeId, parent: NodeId) -> bool {
    match tree.kind(parent) {
        Kind::UnaryExpression { .. }
        | Kind::SpreadElement { .. }
        | Kind::BinaryExpression(_)
        | Kind::LogicalExpression(_)
        | Kind::AwaitExpression { .. } => return true,
        Kind::ConditionalExpression { test, .. } if *test == node => return true,
        _ => {}
    }
    unary_like(tree, node, parent)
}

fn needs_paren_before_expression_brace(tc: u32) -> bool {
    tc & (TC_EXPRESSION_STATEMENT | TC_ARROW_BODY) != 0
}

/// The per-type `needsParens` table.
fn needs_parens(tree: &Tree, node: NodeId, parent: NodeId, tc: u32) -> bool {
    match tree.kind(node) {
        Kind::UpdateExpression { .. } => {
            has_postfix_part(tree, node, parent) || is_class_extends_clause(tree, node, parent)
        }
        Kind::ObjectExpression { .. } => needs_paren_before_expression_brace(tc),
        Kind::BinaryExpression(b) => {
            binary_like(tree, node, parent, b.operator, false)
                || (tc & TC_ACCUMULATE != 0 && b.operator == "in")
        }
        Kind::LogicalExpression(b) => binary_like(tree, node, parent, b.operator, true),
        Kind::SequenceExpression { .. } => sequence_needs_parens(tree, node, parent),
        Kind::YieldExpression { .. } | Kind::AwaitExpression { .. } => {
            matches!(
                tree.kind(parent),
                Kind::BinaryExpression(_)
                    | Kind::LogicalExpression(_)
                    | Kind::UnaryExpression { .. }
                    | Kind::SpreadElement { .. }
            ) || has_postfix_part(tree, node, parent)
                || (matches!(tree.kind(parent), Kind::AwaitExpression { .. })
                    && matches!(tree.kind(node), Kind::YieldExpression { .. }))
                || matches!(tree.kind(parent), Kind::ConditionalExpression { test, .. } if *test == node)
                || is_class_extends_clause(tree, node, parent)
        }
        Kind::ClassExpression(_) | Kind::FunctionExpression(_) => {
            tc & (TC_EXPRESSION_STATEMENT | TC_EXPORT_DEFAULT) != 0
        }
        Kind::UnaryExpression { .. } | Kind::SpreadElement { .. } => unary_like(tree, node, parent),
        Kind::ConditionalExpression { .. } | Kind::ArrowFunctionExpression(_) => {
            conditional_like(tree, node, parent)
        }
        Kind::OptionalMemberExpression(_) | Kind::OptionalCallExpression(_) => {
            match tree.kind(parent) {
                Kind::CallExpression(c) => c.callee == node,
                Kind::MemberExpression(m) => m.object == node,
                _ => false,
            }
        }
        Kind::AssignmentExpression(b) => {
            if needs_paren_before_expression_brace(tc)
                && matches!(tree.kind(b.left), Kind::ObjectPattern { .. })
            {
                return true;
            }
            conditional_like(tree, node, parent)
        }
        Kind::Identifier { name } => identifier_needs_parens(tree, node, name, parent, tc),
        _ => false,
    }
}

fn sequence_needs_parens(tree: &Tree, node: NodeId, parent: NodeId) -> bool {
    match tree.kind(parent) {
        Kind::SequenceExpression { .. } | Kind::TemplateLiteral { .. } => return false,
        Kind::MemberExpression(m) | Kind::OptionalMemberExpression(m) if m.property == node => {
            return false;
        }
        Kind::ClassDeclaration(_) | Kind::ExportDefaultDeclaration { .. } => return true,
        Kind::ForOfStatement { right, .. } => return *right == node,
        _ => {}
    }
    !tree.kind(parent).is_statement()
}

fn identifier_needs_parens(tree: &Tree, node: NodeId, name: &str, parent: NodeId, tc: u32) -> bool {
    if let Kind::AssignmentExpression(Binary { left, right, .. }) = tree.kind(parent)
        && tree.node(node).parenthesized
        && *left == node
    {
        let anonymous = match tree.kind(*right) {
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
        tree.kind(parent),
        Kind::MemberExpression(_) | Kind::OptionalMemberExpression(_)
    );
    if (tc & TC_FOR_OF_HEAD != 0 || member_parent && tc & head != 0) && name == "let" {
        let followed_by_bracket = match tree.kind(parent) {
            Kind::MemberExpression(m) => m.object == node && m.computed,
            Kind::OptionalMemberExpression(m) => m.object == node && m.computed && !m.optional,
            _ => false,
        };
        if followed_by_bracket && tc & head != 0 {
            return true;
        }
        return tc & TC_FOR_OF_HEAD != 0;
    }
    matches!(tree.kind(parent), Kind::ForOfStatement { left, is_await, .. }
        if *left == node && name == "async" && !is_await)
}
