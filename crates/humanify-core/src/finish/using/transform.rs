//! `@babel/plugin-transform-explicit-resource-management` 7.29.7 over the
//! Babel-shaped AST, for a CommonJS script (the Program visitor's
//! top-level rewrite only runs on modules, which the desugar refuses).
//!
//! Each `BlockStatement` / `StaticBlock` holding `using` declarations
//! becomes
//!
//! ```text
//! try { var _usingCtx = _usingCtx2(); <body> }
//! catch (_) { _usingCtx.e = _; } finally { [await] _usingCtx.d(); }
//! ```
//!
//! with every `using x = init` rewritten to `const x = _usingCtx.u(init)`
//! (`.a(…)` for `await using`). The try statement inherits the block's
//! `loc`; everything else the template makes has none. A function / try /
//! catch body is replaced by a fresh block around the try.
//!
//! ORDER is the plugin's: Babel's pre-order traversal, except that entering
//! an ASYNC function first transforms its own blocks (a nested traversal
//! that skips nested functions) — so the `_usingCtx<n>` numbering and the
//! helper's name (the first `addHelper` takes the next uid after the first
//! context) follow that order exactly.
//!
//! Refused (an `Err`, never a guess): `for (using x of …)`, a helper
//! global shadowed by a program-scope binding (Babel would rename it
//! file-wide), and any identifier already spelled like a generated uid.

use std::collections::HashSet;

use super::ast::{Binary, Call, Kind, Member, Node};
use super::helpers::helper_declaration;

/// The traversal's state: the uids handed out, the helpers added.
pub struct State {
    used: HashSet<String>,
    /// `file.declarations`: helper name → its uid, in `addHelper` order.
    helpers: Vec<(&'static str, String)>,
}

impl State {
    pub fn new() -> State {
        State {
            used: HashSet::new(),
            helpers: Vec::new(),
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

impl Default for State {
    fn default() -> Self {
        State::new()
    }
}

/// How a block's parent decides its replacement.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Parent {
    /// A function, try statement or catch clause: `{ try … }`.
    Wraps,
    /// Anything else: the try statement itself.
    Other,
}

fn is_using(node: &Node) -> bool {
    matches!(node.kind, Kind::VariableDeclaration { kind, .. } if kind == "using" || kind == "await using")
}

fn member(object: Node, property: &str) -> Node {
    Node::synth(Kind::MemberExpression(Member {
        object: Box::new(object),
        property: Box::new(Node::ident(property)),
        computed: false,
        optional: false,
    }))
}

fn call(callee: Node, arguments: Vec<Node>) -> Node {
    Node::synth(Kind::CallExpression(Call {
        callee: Box::new(callee),
        arguments,
        optional: false,
    }))
}

/// `isAnonymousFunctionDefinition(node)`.
fn is_anonymous_function(node: &Node) -> bool {
    match &node.kind {
        Kind::ArrowFunctionExpression(_) => true,
        Kind::FunctionExpression(f) => f.id.is_none(),
        Kind::ClassExpression(c) => c.id.is_none(),
        _ => false,
    }
}

/// The block visitor: rewrite the using declarations of `body`, and build
/// the try statement around it (None when nothing was a using).
fn rewrite_body(body: Vec<Node>, state: &mut State) -> Result<Result<Node, Vec<Node>>, String> {
    let mut body = body;
    let mut ctx: Option<String> = None;
    let mut needs_await = false;
    for stmt in &mut body {
        if !is_using(stmt) {
            continue;
        }
        let ctx_name = ctx
            .get_or_insert_with(|| state.generate_uid("usingCtx"))
            .clone();
        let Kind::VariableDeclaration { kind, declarations } = &mut stmt.kind else {
            unreachable!("is_using checked the kind");
        };
        let is_await = *kind == "await using";
        needs_await |= is_await;
        *kind = "const";
        for decl in declarations {
            let Kind::VariableDeclarator { id, init } = &mut decl.kind else {
                continue;
            };
            let current = init
                .take()
                .ok_or("a using declaration without an initializer")?;
            let arg = if is_anonymous_function(&current) && id.is_identifier() {
                let name = id.identifier_name().expect("identifier").to_string();
                let helper = state.add_helper("setFunctionName");
                call(
                    Node::ident(&helper),
                    vec![
                        *current,
                        Node::synth(Kind::StringLiteral {
                            value: name,
                            raw: None,
                        }),
                    ],
                )
            } else {
                *current
            };
            let method = if is_await { "a" } else { "u" };
            *init = Some(Box::new(call(
                member(Node::ident(&ctx_name), method),
                vec![arg],
            )));
        }
    }
    let Some(ctx) = ctx else {
        return Ok(Err(body));
    };
    let mut dispose = call(member(Node::ident(&ctx), "d"), vec![]);
    if needs_await {
        dispose = Node::synth(Kind::AwaitExpression {
            argument: Box::new(dispose),
        });
    }
    // The template's argument order: the context, THEN the helper.
    let helper = state.add_helper("usingCtx");
    let var = Node::synth(Kind::VariableDeclaration {
        kind: "var",
        declarations: vec![Node::synth(Kind::VariableDeclarator {
            id: Box::new(Node::ident(&ctx)),
            init: Some(Box::new(call(Node::ident(&helper), vec![]))),
        })],
    });
    let mut try_body = vec![var];
    try_body.extend(body);
    let block = |body: Vec<Node>| {
        Node::synth(Kind::BlockStatement {
            directives: Vec::new(),
            body,
        })
    };
    let assign = Node::synth(Kind::ExpressionStatement {
        expression: Box::new(Node::synth(Kind::AssignmentExpression(Binary {
            operator: "=",
            left: Box::new(member(Node::ident(&ctx), "e")),
            right: Box::new(Node::ident("_")),
        }))),
    });
    let handler = Node::synth(Kind::CatchClause {
        param: Some(Box::new(Node::ident("_"))),
        body: Box::new(block(vec![assign])),
    });
    let finalizer = block(vec![Node::synth(Kind::ExpressionStatement {
        expression: Box::new(dispose),
    })]);
    Ok(Ok(Node::synth(Kind::TryStatement {
        block: Box::new(block(try_body)),
        handler: Some(Box::new(handler)),
        finalizer: Some(Box::new(finalizer)),
    })))
}

/// The `BlockStatement|StaticBlock` visitor on one node (in place).
fn visit_block(node: &mut Node, parent: Parent, state: &mut State) -> Result<(), String> {
    match &mut node.kind {
        Kind::BlockStatement { body, .. } => {
            if !body.iter().any(is_using) {
                return Ok(());
            }
            let taken = std::mem::take(body);
            match rewrite_body(taken, state)? {
                Err(body_back) => {
                    if let Kind::BlockStatement { body, .. } = &mut node.kind {
                        *body = body_back;
                    }
                }
                Ok(mut try_stmt) => {
                    // `t.inherits(replacement, path.node)`: loc (+ comments).
                    try_stmt.loc = node.loc;
                    *node = match parent {
                        Parent::Wraps => Node::synth(Kind::BlockStatement {
                            directives: Vec::new(),
                            body: vec![try_stmt],
                        }),
                        Parent::Other => try_stmt,
                    };
                }
            }
        }
        Kind::StaticBlock { body } => {
            if !body.iter().any(is_using) {
                return Ok(());
            }
            let taken = std::mem::take(body);
            let loc = node.loc;
            let replacement = match rewrite_body(taken, state)? {
                Err(body_back) => body_back,
                Ok(mut try_stmt) => {
                    try_stmt.loc = loc;
                    vec![try_stmt]
                }
            };
            if let Kind::StaticBlock { body } = &mut node.kind {
                *body = replacement;
            }
        }
        Kind::ForOfStatement { left, .. } if is_using(left) => {
            return Err("for (using … of …): not ported".into());
        }
        _ => {}
    }
    Ok(())
}

fn one<'n>(out: &mut Vec<&'n mut Node>, n: &'n mut Node) {
    out.push(n);
}

fn opt<'n>(out: &mut Vec<&'n mut Node>, n: &'n mut Option<Box<Node>>) {
    if let Some(n) = n.as_deref_mut() {
        out.push(n);
    }
}

fn all<'n>(out: &mut Vec<&'n mut Node>, v: &'n mut [Node]) {
    out.extend(v.iter_mut());
}

/// The statement kinds' children (false: not a statement kind).
fn statement_children<'n>(kind: &'n mut Kind, out: &mut Vec<&'n mut Node>) -> bool {
    match kind {
        Kind::Program {
            directives, body, ..
        }
        | Kind::BlockStatement { directives, body } => {
            all(out, directives);
            all(out, body);
        }
        Kind::StaticBlock { body } | Kind::ClassBody { body } => all(out, body),
        Kind::Directive { value } => one(out, value),
        Kind::ExpressionStatement { expression } => one(out, expression),
        Kind::WithStatement { object: a, body: b }
        | Kind::LabeledStatement { label: a, body: b }
        | Kind::WhileStatement { test: a, body: b }
        | Kind::DoWhileStatement { body: a, test: b } => {
            one(out, a);
            one(out, b);
        }
        Kind::ReturnStatement { argument }
        | Kind::BreakStatement { label: argument }
        | Kind::ContinueStatement { label: argument } => opt(out, argument),
        Kind::ThrowStatement { argument } => one(out, argument),
        Kind::IfStatement {
            test,
            consequent,
            alternate,
        } => {
            one(out, test);
            one(out, consequent);
            opt(out, alternate);
        }
        Kind::SwitchStatement {
            discriminant,
            cases,
        } => {
            one(out, discriminant);
            all(out, cases);
        }
        Kind::SwitchCase { test, consequent } => {
            opt(out, test);
            all(out, consequent);
        }
        Kind::TryStatement {
            block,
            handler,
            finalizer,
        } => {
            one(out, block);
            opt(out, handler);
            opt(out, finalizer);
        }
        Kind::CatchClause { param, body } => {
            opt(out, param);
            one(out, body);
        }
        _ => return loop_children(kind, out),
    }
    true
}

fn loop_children<'n>(kind: &'n mut Kind, out: &mut Vec<&'n mut Node>) -> bool {
    match kind {
        Kind::ForStatement {
            init,
            test,
            update,
            body,
        } => {
            opt(out, init);
            opt(out, test);
            opt(out, update);
            one(out, body);
        }
        Kind::ForInStatement { left, right, body }
        | Kind::ForOfStatement {
            left, right, body, ..
        } => {
            one(out, left);
            one(out, right);
            one(out, body);
        }
        Kind::VariableDeclaration { declarations, .. } => all(out, declarations),
        Kind::VariableDeclarator { id, init } => {
            one(out, id);
            opt(out, init);
        }
        _ => return false,
    }
    true
}

/// Functions, classes and their members.
fn function_children<'n>(kind: &'n mut Kind, out: &mut Vec<&'n mut Node>) -> bool {
    match kind {
        Kind::FunctionDeclaration(f) | Kind::FunctionExpression(f) => {
            opt(out, &mut f.id);
            all(out, &mut f.params);
            one(out, &mut f.body);
        }
        Kind::ArrowFunctionExpression(f) => {
            all(out, &mut f.params);
            one(out, &mut f.body);
        }
        Kind::ClassDeclaration(c) | Kind::ClassExpression(c) => {
            opt(out, &mut c.id);
            opt(out, &mut c.super_class);
            one(out, &mut c.body);
        }
        Kind::ClassMethod(m) | Kind::ClassPrivateMethod(m) | Kind::ObjectMethod(m) => {
            one(out, &mut m.key);
            all(out, &mut m.func.params);
            one(out, &mut m.func.body);
        }
        Kind::ClassProperty(fd)
        | Kind::ClassPrivateProperty(fd)
        | Kind::ClassAccessorProperty(fd) => {
            one(out, &mut fd.key);
            opt(out, &mut fd.value);
        }
        _ => return false,
    }
    true
}

/// Expressions and patterns.
fn expression_children<'n>(kind: &'n mut Kind, out: &mut Vec<&'n mut Node>) {
    match kind {
        Kind::PrivateName { id: a }
        | Kind::SpreadElement { argument: a }
        | Kind::RestElement { argument: a }
        | Kind::UnaryExpression { argument: a, .. }
        | Kind::UpdateExpression { argument: a, .. }
        | Kind::AwaitExpression { argument: a } => one(out, a),
        Kind::TemplateLiteral { expressions, .. } | Kind::SequenceExpression { expressions } => {
            all(out, expressions)
        }
        Kind::ArrayExpression { elements } | Kind::ArrayPattern { elements } => {
            out.extend(elements.iter_mut().flatten());
        }
        Kind::ObjectExpression { properties } | Kind::ObjectPattern { properties } => {
            all(out, properties)
        }
        Kind::TaggedTemplateExpression { tag: a, quasi: b }
        | Kind::ObjectProperty {
            key: a, value: b, ..
        }
        | Kind::AssignmentPattern { left: a, right: b }
        | Kind::MetaProperty {
            meta: a,
            property: b,
        } => {
            one(out, a);
            one(out, b);
        }
        Kind::BinaryExpression(x) | Kind::LogicalExpression(x) | Kind::AssignmentExpression(x) => {
            one(out, &mut x.left);
            one(out, &mut x.right);
        }
        Kind::ConditionalExpression {
            test,
            consequent,
            alternate,
        } => {
            one(out, test);
            one(out, consequent);
            one(out, alternate);
        }
        Kind::CallExpression(c) | Kind::NewExpression(c) | Kind::OptionalCallExpression(c) => {
            one(out, &mut c.callee);
            all(out, &mut c.arguments);
        }
        Kind::MemberExpression(m) | Kind::OptionalMemberExpression(m) => {
            one(out, &mut m.object);
            one(out, &mut m.property);
        }
        Kind::YieldExpression { argument, .. } => opt(out, argument),
        _ => {}
    }
}

/// A node's children, mutable, in `VISITOR_KEYS` order, each with how it
/// would be replaced if it were a block.
pub(super) fn children_mut(node: &mut Node) -> Vec<(&mut Node, Parent)> {
    let wraps = matches!(
        node.kind,
        Kind::TryStatement { .. } | Kind::CatchClause { .. }
    ) || node.is_function();
    let p = if wraps { Parent::Wraps } else { Parent::Other };
    let mut out: Vec<&mut Node> = Vec::new();
    let kind = &mut node.kind;
    if matches!(
        kind,
        Kind::FunctionDeclaration(_)
            | Kind::FunctionExpression(_)
            | Kind::ArrowFunctionExpression(_)
            | Kind::ClassDeclaration(_)
            | Kind::ClassExpression(_)
            | Kind::ClassMethod(_)
            | Kind::ClassPrivateMethod(_)
            | Kind::ObjectMethod(_)
            | Kind::ClassProperty(_)
            | Kind::ClassPrivateProperty(_)
            | Kind::ClassAccessorProperty(_)
    ) {
        function_children(kind, &mut out);
    } else if is_statement_like(kind) {
        statement_children(kind, &mut out);
    } else {
        expression_children(kind, &mut out);
    }
    out.into_iter().map(|n| (n, p)).collect()
}

fn is_statement_like(kind: &Kind) -> bool {
    matches!(
        kind,
        Kind::Program { .. }
            | Kind::BlockStatement { .. }
            | Kind::StaticBlock { .. }
            | Kind::ClassBody { .. }
            | Kind::Directive { .. }
            | Kind::ExpressionStatement { .. }
            | Kind::WithStatement { .. }
            | Kind::LabeledStatement { .. }
            | Kind::WhileStatement { .. }
            | Kind::DoWhileStatement { .. }
            | Kind::ReturnStatement { .. }
            | Kind::BreakStatement { .. }
            | Kind::ContinueStatement { .. }
            | Kind::ThrowStatement { .. }
            | Kind::IfStatement { .. }
            | Kind::SwitchStatement { .. }
            | Kind::SwitchCase { .. }
            | Kind::TryStatement { .. }
            | Kind::CatchClause { .. }
            | Kind::ForStatement { .. }
            | Kind::ForInStatement { .. }
            | Kind::ForOfStatement { .. }
            | Kind::VariableDeclaration { .. }
            | Kind::VariableDeclarator { .. }
    )
}

fn is_async_function(node: &Node) -> bool {
    node.func().is_some_and(|f| f.is_async)
}

/// The main traversal (enter visitors, then the children).
fn traverse(node: &mut Node, parent: Parent, state: &mut State) -> Result<(), String> {
    // Function: an async function's own blocks first (the skip-nested pass).
    if is_async_function(node) {
        for (child, p) in children_mut(node) {
            traverse_skipping_functions(child, p, state)?;
        }
    }
    visit_block(node, parent, state)?;
    for (child, p) in children_mut(node) {
        traverse(child, p, state)?;
    }
    Ok(())
}

/// `path.traverse(transformUsingDeclarationsVisitorSkipFn)`.
fn traverse_skipping_functions(
    node: &mut Node,
    parent: Parent,
    state: &mut State,
) -> Result<(), String> {
    if node.is_function() {
        return Ok(()); // path.skip()
    }
    visit_block(node, parent, state)?;
    for (child, p) in children_mut(node) {
        traverse_skipping_functions(child, p, state)?;
    }
    Ok(())
}

/// Run the plugin over a Program; the helpers land at the front of its
/// body (`unshiftContainer`, so the LAST added is first).
pub fn transform_program(program: &mut Node) -> Result<(), String> {
    let mut state = State::new();
    traverse(program, Parent::Other, &mut state)?;
    if let Kind::Program { body, .. } = &mut program.kind {
        for (name, uid) in &state.helpers {
            body.insert(0, helper_declaration(name, uid)?);
        }
    }
    Ok(())
}
