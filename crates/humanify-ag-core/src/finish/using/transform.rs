//! `@babel/plugin-transform-explicit-resource-management` 7.29.7 as a set
//! of SPLICES into the file's own text, for a CommonJS script (the Program
//! visitor's top-level rewrite only runs on modules, which the desugar
//! refuses).
//!
//! The program the splices build is the plugin's. Each `BlockStatement` /
//! `StaticBlock` holding `using` declarations becomes
//!
//! ```text
//! try { var _usingCtx = _usingCtx2(); <body> }
//! catch (_) { _usingCtx.e = _; } finally { [await] _usingCtx.d(); }
//! ```
//!
//! with every `using x = init` rewritten to `const x = _usingCtx.u(init)`
//! (`.a(…)` for `await using`). A function / try / catch / static-block
//! body keeps its braces around the try; any other block IS the try.
//!
//! The text is NOT reprinted: the `try {var …;` opener goes on the line
//! of the block's `{`, the `} catch … finally {…}` closer on the line of
//! its `}`, and the body between them keeps its bytes and its indentation.
//! A `using` statement changes only its keyword and the call around each
//! initializer. Every other line of the file stays as the writer wrote it
//! (the layout differs from Babel's `retainLines` print; the program does
//! not).
//!
//! ORDER is the plugin's: Babel's pre-order traversal, except that entering
//! an ASYNC function first transforms its own blocks (a nested traversal
//! that skips nested functions) — so the `_usingCtx<n>` numbering and the
//! helper's name (the first `addHelper` takes the next uid after the first
//! context) follow that order exactly.
//!
//! Refused (an `Err`, never a guess): `for (using x of …)`, a node without
//! a source span, and a static block whose `{` cannot be found.

use std::collections::HashSet;

use crate::format::ast::{Kind, NodeId, Tree};

/// One edit of the source text: `[start, end)` becomes `text`.
///
/// `outer` is the span of the construct the edit opens or closes, which
/// orders edits at the same offset: closers first, the innermost
/// construct's first; then openers, the outermost construct's first.
#[derive(Clone, Debug)]
pub struct Splice {
    pub start: u32,
    pub end: u32,
    pub text: String,
    closes: bool,
    outer: (u32, u32),
}

impl Splice {
    fn open(at: u32, outer: (u32, u32), text: String) -> Splice {
        Splice {
            start: at,
            end: at,
            text,
            closes: false,
            outer,
        }
    }

    fn close(at: u32, outer: (u32, u32), text: String) -> Splice {
        Splice {
            start: at,
            end: at,
            text,
            closes: true,
            outer,
        }
    }

    /// The text-order key at one offset (see the type's doc).
    fn order_key(&self) -> (u32, bool, i64) {
        let tie = if self.closes {
            -i64::from(self.outer.0)
        } else {
            -i64::from(self.outer.1)
        };
        (self.start, !self.closes, tie)
    }
}

/// Apply `splices` to `text` (any order in; text order out).
pub fn apply(text: &str, mut splices: Vec<Splice>) -> Result<String, String> {
    splices.sort_by_key(Splice::order_key);
    let mut out = String::with_capacity(text.len() + splices.len() * 48);
    let mut cursor = 0usize;
    for s in splices {
        let (start, end) = (s.start as usize, s.end as usize);
        if start < cursor || end > text.len() || !text.is_char_boundary(start) {
            return Err(format!("using desugar: overlapping splice at {start}"));
        }
        out.push_str(&text[cursor..start]);
        out.push_str(&s.text);
        cursor = end;
    }
    out.push_str(&text[cursor..]);
    Ok(out)
}

/// The traversal's state: the uids handed out, the helpers added, the
/// splices planned.
pub struct State<'s> {
    source: &'s str,
    used: HashSet<String>,
    /// `file.declarations`: helper name → its uid, in `addHelper` order.
    pub helpers: Vec<(&'static str, String)>,
    pub splices: Vec<Splice>,
}

impl<'s> State<'s> {
    pub fn new(source: &'s str) -> State<'s> {
        State {
            source,
            used: HashSet::new(),
            helpers: Vec::new(),
            splices: Vec::new(),
        }
    }

    /// `scope.generateUid(name)`: `_name`, then `_name2` … `_name9`,
    /// `_name0`, `_name1`, `_name10` … — the first spelling not yet used.
    /// (The caller has refused any input that spells one already.)
    fn generate_uid(&mut self, name: &str) -> String {
        let base = name
            .trim_start_matches('_')
            .trim_end_matches(|c: char| c.is_ascii_digit());
        let mut i: u32 = 0;
        loop {
            let mut uid = format!("_{base}");
            if i >= 11 {
                uid.push_str(&(i - 1).to_string());
            } else if i >= 9 {
                uid.push_str(&(i - 9).to_string());
            } else if i >= 1 {
                uid.push_str(&(i + 1).to_string());
            }
            i += 1;
            if !self.used.contains(&uid) {
                self.used.insert(uid.clone());
                return uid;
            }
        }
    }

    /// `file.addHelper(name)`: the helper's uid (created on first use).
    fn add_helper(&mut self, name: &'static str) -> String {
        if let Some((_, uid)) = self.helpers.iter().find(|(n, _)| *n == name) {
            return uid.clone();
        }
        let uid = self.generate_uid(name);
        self.helpers.push((name, uid.clone()));
        uid
    }
}

/// How a block's parent decides its replacement.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Parent {
    /// A function, try statement or catch clause: `{ try … }`.
    Wraps,
    /// Anything else: the try statement itself.
    Other,
}

fn is_using(tree: &Tree, node: NodeId) -> bool {
    matches!(tree.kind(node), Kind::VariableDeclaration { kind, .. } if *kind == "using" || *kind == "await using")
}

fn span(tree: &Tree, node: NodeId) -> Result<(u32, u32), String> {
    tree.node(node)
        .span
        .ok_or_else(|| "using desugar: a node without a source span".into())
}

/// The node's span with its outermost parentheses, if any (a
/// parenthesized sequence must stay ONE argument).
fn outer_span(tree: &Tree, node: NodeId) -> Result<(u32, u32), String> {
    let (mut start, mut end) = span(tree, node)?;
    for &(id, s, e) in &tree.parens {
        if id == node && s <= start && e >= end {
            (start, end) = (s, e);
        }
    }
    Ok((start, end))
}

/// `isAnonymousFunctionDefinition(node)`.
fn is_anonymous_function(tree: &Tree, node: NodeId) -> bool {
    match tree.kind(node) {
        Kind::ArrowFunctionExpression(_) => true,
        Kind::FunctionExpression(f) => f.id.is_none(),
        Kind::ClassExpression(c) => c.id.is_none(),
        _ => false,
    }
}

/// Plan one `using` declarator's init: `ctx.u(init)` /
/// `ctx.u(_setFunctionName(init, "x"))`.
fn rewrite_declarator(
    tree: &Tree,
    decl: NodeId,
    ctx_name: &str,
    is_await: bool,
    state: &mut State<'_>,
) -> Result<(), String> {
    let Kind::VariableDeclarator { id, init } = tree.kind(decl) else {
        return Ok(());
    };
    let init = init.ok_or("a using declaration without an initializer")?;
    let method = if is_await { "a" } else { "u" };
    let mut open = format!("{ctx_name}.{method}(");
    let mut close = ")".to_string();
    if let Some(name) = tree.kind(*id).identifier_name()
        && is_anonymous_function(tree, init)
    {
        let name = name.to_string();
        let helper = state.add_helper("setFunctionName");
        open.push_str(&helper);
        open.push('(');
        close = format!(", \"{name}\"))");
    }
    let outer = outer_span(tree, init)?;
    state.splices.push(Splice::open(outer.0, outer, open));
    state.splices.push(Splice::close(outer.1, outer, close));
    Ok(())
}

/// What the try statement around one block needs.
struct Context {
    ctx: String,
    helper: String,
    needs_await: bool,
}

/// The block visitor: plan the using declarations of `body` (None when
/// nothing was a using). Each one is marked `const` in the tree, so a
/// later pass over the same block finds nothing left to do.
fn rewrite_body(
    tree: &mut Tree,
    body: &[NodeId],
    state: &mut State<'_>,
) -> Result<Option<Context>, String> {
    let mut ctx: Option<String> = None;
    let mut needs_await = false;
    for &stmt in body {
        if !is_using(tree, stmt) {
            continue;
        }
        let ctx_name = ctx
            .get_or_insert_with(|| state.generate_uid("usingCtx"))
            .clone();
        let Kind::VariableDeclaration { kind, declarations } = tree.kind_mut(stmt) else {
            unreachable!("is_using checked the kind");
        };
        let is_await = *kind == "await using";
        needs_await |= is_await;
        *kind = "const";
        let declarations = declarations.clone();
        let stmt_span = span(tree, stmt)?;
        let first = span(tree, *declarations.first().ok_or("an empty declaration")?)?;
        state.splices.push(Splice {
            start: stmt_span.0,
            end: first.0,
            text: "const ".into(),
            closes: false,
            outer: stmt_span,
        });
        for decl in declarations {
            rewrite_declarator(tree, decl, &ctx_name, is_await, state)?;
        }
    }
    let Some(ctx) = ctx else {
        return Ok(None);
    };
    // The template's argument order: the context, THEN the helper.
    let helper = state.add_helper("usingCtx");
    Ok(Some(Context {
        ctx,
        helper,
        needs_await,
    }))
}

impl Context {
    fn opener(&self) -> String {
        format!("var {} = {}();", self.ctx, self.helper)
    }

    fn closer(&self) -> String {
        let wait = if self.needs_await { "await " } else { "" };
        format!(
            " catch (_) {{{ctx}.e = _;}} finally {{{wait}{ctx}.d();}}",
            ctx = self.ctx
        )
    }

    /// `{ try { … } catch … }`: the try goes INSIDE the braces, opened
    /// at `open` (just past the `{`, or past the directive prologue).
    fn wrap_inside(&self, block: (u32, u32), open: u32, state: &mut State<'_>) {
        state.splices.push(Splice::open(
            open,
            block,
            format!("try {{{}", self.opener()),
        ));
        state.splices.push(Splice::close(
            block.1 - 1,
            block,
            format!("}}{}", self.closer()),
        ));
    }

    /// The block BECOMES the try block.
    fn become_try(&self, block: (u32, u32), state: &mut State<'_>) {
        state
            .splices
            .push(Splice::open(block.0, block, "try ".into()));
        state
            .splices
            .push(Splice::open(block.0 + 1, block, self.opener()));
        state
            .splices
            .push(Splice::close(block.1, block, self.closer()));
    }
}

/// The offset just past a static block's `{` (whitespace and comments may
/// sit between `static` and it).
fn static_block_open(source: &str, start: u32) -> Result<u32, String> {
    let bytes = source.as_bytes();
    let mut i = start as usize + "static".len();
    while i < bytes.len() {
        let rest = &source[i..];
        if rest.starts_with("//") {
            i += rest.find('\n').unwrap_or(rest.len());
        } else if rest.starts_with("/*") {
            i += rest.find("*/").map_or(rest.len(), |e| e + 2);
        } else if bytes[i].is_ascii_whitespace() {
            i += 1;
        } else if bytes[i] == b'{' {
            return Ok(i as u32 + 1);
        } else {
            break;
        }
    }
    Err("using desugar: a static block without its `{`".into())
}

/// The `BlockStatement|StaticBlock` visitor on one node.
fn visit_block(
    tree: &mut Tree,
    node: NodeId,
    parent: Parent,
    state: &mut State<'_>,
) -> Result<(), String> {
    match tree.kind(node).clone() {
        Kind::BlockStatement { body, directives } => {
            if !body.iter().any(|&s| is_using(tree, s)) {
                return Ok(());
            }
            if let Some(ctx) = rewrite_body(tree, &body, state)? {
                let block = span(tree, node)?;
                match parent {
                    Parent::Wraps => {
                        // A directive stays a directive: the try opens after it.
                        let open = match directives.last() {
                            Some(&d) => span(tree, d)?.1,
                            None => block.0 + 1,
                        };
                        ctx.wrap_inside(block, open, state);
                    }
                    Parent::Other => ctx.become_try(block, state),
                }
            }
        }
        Kind::StaticBlock { body } => {
            if !body.iter().any(|&s| is_using(tree, s)) {
                return Ok(());
            }
            if let Some(ctx) = rewrite_body(tree, &body, state)? {
                let block = span(tree, node)?;
                let open = static_block_open(state.source, block.0)?;
                ctx.wrap_inside(block, open, state);
            }
        }
        Kind::ForOfStatement { left, .. } if is_using(tree, left) => {
            return Err("for (using … of …): not ported".into());
        }
        _ => {}
    }
    Ok(())
}

/// How `node`'s children would be replaced if they were blocks.
fn parent_of_children(tree: &Tree, node: NodeId) -> Parent {
    let kind = tree.kind(node);
    if matches!(kind, Kind::TryStatement { .. } | Kind::CatchClause { .. }) || kind.is_function() {
        Parent::Wraps
    } else {
        Parent::Other
    }
}

fn is_async_function(tree: &Tree, node: NodeId) -> bool {
    tree.kind(node).func().is_some_and(|f| f.is_async)
}

/// The main traversal (enter visitors, then the children).
fn traverse(
    tree: &mut Tree,
    node: NodeId,
    parent: Parent,
    state: &mut State<'_>,
) -> Result<(), String> {
    // Function: an async function's own blocks first (the skip-nested pass).
    if is_async_function(tree, node) {
        let p = parent_of_children(tree, node);
        for child in tree.children(node) {
            traverse_skipping_functions(tree, child, p, state)?;
        }
    }
    visit_block(tree, node, parent, state)?;
    let p = parent_of_children(tree, node);
    for child in tree.children(node) {
        traverse(tree, child, p, state)?;
    }
    Ok(())
}

/// `path.traverse(transformUsingDeclarationsVisitorSkipFn)`.
fn traverse_skipping_functions(
    tree: &mut Tree,
    node: NodeId,
    parent: Parent,
    state: &mut State<'_>,
) -> Result<(), String> {
    if tree.kind(node).is_function() {
        return Ok(()); // path.skip()
    }
    visit_block(tree, node, parent, state)?;
    let p = parent_of_children(tree, node);
    for child in tree.children(node) {
        traverse_skipping_functions(tree, child, p, state)?;
    }
    Ok(())
}

/// Run the plugin over a Program: the splices for its blocks and the
/// helpers it needs, in `addHelper` order (the caller places them).
pub fn plan_program(tree: &mut Tree, program: NodeId, state: &mut State<'_>) -> Result<(), String> {
    traverse(tree, program, Parent::Other, state)
}

/// The splice that puts the helpers' text on a line of its own above the
/// program's first statement (after any hashbang and directive prologue,
/// where Babel's `unshiftContainer` puts them).
pub fn helpers_splice(tree: &Tree, program: NodeId, text: String) -> Result<Splice, String> {
    let Kind::Program { body, .. } = tree.kind(program) else {
        return Err("using desugar: not a program".into());
    };
    let first = *body.first().ok_or("using desugar: an empty program")?;
    let at = span(tree, first)?.0;
    Ok(Splice::open(at, (at, u32::MAX), format!("{text}\n")))
}
