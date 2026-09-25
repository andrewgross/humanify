//! `@babel/plugin-transform-explicit-resource-management` 7.29.7 over the
//! Babel-shaped arena (`crate::format::ast`), for a CommonJS script (the
//! Program visitor's top-level rewrite only runs on modules, which the
//! desugar refuses).
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

use crate::format::ast::{Binary, Call, Kind, Member, Node, NodeId, Tree};

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
enum Parent {
    /// A function, try statement or catch clause: `{ try … }`.
    Wraps,
    /// Anything else: the try statement itself.
    Other,
}

fn is_using(tree: &Tree, node: NodeId) -> bool {
    matches!(tree.kind(node), Kind::VariableDeclaration { kind, .. } if *kind == "using" || *kind == "await using")
}

fn member(tree: &mut Tree, object: NodeId, property: &str) -> NodeId {
    let property = tree.ident(property);
    tree.synth(Kind::MemberExpression(Member {
        object,
        property,
        computed: false,
        optional: false,
    }))
}

fn call(tree: &mut Tree, callee: NodeId, arguments: Vec<NodeId>) -> NodeId {
    tree.synth(Kind::CallExpression(Call {
        callee,
        arguments,
        optional: false,
    }))
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

/// Rewrite one `using` declarator's init in place.
fn rewrite_declarator(
    tree: &mut Tree,
    decl: NodeId,
    ctx_name: &str,
    is_await: bool,
    state: &mut State,
) -> Result<(), String> {
    let Kind::VariableDeclarator { id, init } = tree.kind(decl).clone() else {
        return Ok(());
    };
    let current = init.ok_or("a using declaration without an initializer")?;
    let arg = match tree.kind(id).identifier_name() {
        Some(name) if is_anonymous_function(tree, current) => {
            let name = name.to_string();
            let helper = state.add_helper("setFunctionName");
            let callee = tree.ident(&helper);
            let lit = tree.synth(Kind::StringLiteral {
                value: name,
                raw: None,
            });
            call(tree, callee, vec![current, lit])
        }
        _ => current,
    };
    let method = if is_await { "a" } else { "u" };
    let ctx = tree.ident(ctx_name);
    let callee = member(tree, ctx, method);
    let new_init = call(tree, callee, vec![arg]);
    if let Kind::VariableDeclarator { init, .. } = tree.kind_mut(decl) {
        *init = Some(new_init);
    }
    Ok(())
}

/// The try statement around a rewritten body.
fn try_statement(
    tree: &mut Tree,
    ctx: &str,
    needs_await: bool,
    body: Vec<NodeId>,
    state: &mut State,
) -> NodeId {
    let ctx_id = tree.ident(ctx);
    let d = member(tree, ctx_id, "d");
    let mut dispose = call(tree, d, vec![]);
    if needs_await {
        dispose = tree.synth(Kind::AwaitExpression { argument: dispose });
    }
    // The template's argument order: the context, THEN the helper.
    let helper = state.add_helper("usingCtx");
    let helper_id = tree.ident(&helper);
    let init = call(tree, helper_id, vec![]);
    let id = tree.ident(ctx);
    let declarator = tree.synth(Kind::VariableDeclarator {
        id,
        init: Some(init),
    });
    let var = tree.synth(Kind::VariableDeclaration {
        kind: "var",
        declarations: vec![declarator],
    });
    let mut try_body = vec![var];
    try_body.extend(body);
    let ctx_id = tree.ident(ctx);
    let left = member(tree, ctx_id, "e");
    let right = tree.ident("_");
    let assign_expr = tree.synth(Kind::AssignmentExpression(Binary {
        operator: "=",
        left,
        right,
    }));
    let assign = tree.synth(Kind::ExpressionStatement {
        expression: assign_expr,
    });
    let catch_body = block(tree, vec![assign]);
    let param = tree.ident("_");
    let handler = tree.synth(Kind::CatchClause {
        param: Some(param),
        body: catch_body,
    });
    let dispose_stmt = tree.synth(Kind::ExpressionStatement {
        expression: dispose,
    });
    let finalizer = block(tree, vec![dispose_stmt]);
    let try_block = block(tree, try_body);
    tree.synth(Kind::TryStatement {
        block: try_block,
        handler: Some(handler),
        finalizer: Some(finalizer),
    })
}

fn block(tree: &mut Tree, body: Vec<NodeId>) -> NodeId {
    tree.synth(Kind::BlockStatement {
        directives: Vec::new(),
        body,
    })
}

/// The block visitor: rewrite the using declarations of `body`, and build
/// the try statement around it (None when nothing was a using).
fn rewrite_body(
    tree: &mut Tree,
    body: &[NodeId],
    state: &mut State,
) -> Result<Option<NodeId>, String> {
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
        for decl in declarations.clone() {
            rewrite_declarator(tree, decl, &ctx_name, is_await, state)?;
        }
    }
    let Some(ctx) = ctx else {
        return Ok(None);
    };
    Ok(Some(try_statement(
        tree,
        &ctx,
        needs_await,
        body.to_vec(),
        state,
    )))
}

/// The `BlockStatement|StaticBlock` visitor on one node (in place).
fn visit_block(
    tree: &mut Tree,
    node: NodeId,
    parent: Parent,
    state: &mut State,
) -> Result<(), String> {
    match tree.kind(node).clone() {
        Kind::BlockStatement { body, .. } => {
            if !body.iter().any(|&s| is_using(tree, s)) {
                return Ok(());
            }
            if let Some(try_stmt) = rewrite_body(tree, &body, state)? {
                // `t.inherits(replacement, path.node)`: loc (+ comments).
                tree.node_mut(try_stmt).loc = tree.node(node).loc;
                match parent {
                    Parent::Wraps => {
                        *tree.node_mut(node) = Node::synth(Kind::BlockStatement {
                            directives: Vec::new(),
                            body: vec![try_stmt],
                        });
                    }
                    Parent::Other => {
                        let replacement = tree.node(try_stmt).clone();
                        *tree.node_mut(node) = replacement;
                    }
                }
            }
        }
        Kind::StaticBlock { body } => {
            if !body.iter().any(|&s| is_using(tree, s)) {
                return Ok(());
            }
            if let Some(try_stmt) = rewrite_body(tree, &body, state)? {
                tree.node_mut(try_stmt).loc = tree.node(node).loc;
                *tree.kind_mut(node) = Kind::StaticBlock {
                    body: vec![try_stmt],
                };
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
    state: &mut State,
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
    state: &mut State,
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

/// Run the plugin over a Program; the helpers land at the front of its
/// body (`unshiftContainer`, so the LAST added is first).
pub fn transform_program(tree: &mut Tree, program: NodeId) -> Result<(), String> {
    let mut state = State::new();
    traverse(tree, program, Parent::Other, &mut state)?;
    let mut helper_ids = Vec::new();
    for (name, uid) in &state.helpers {
        helper_ids.push(helper_declaration(tree, name, uid)?);
    }
    if let Kind::Program { body, .. } = tree.kind_mut(program) {
        for id in helper_ids {
            body.insert(0, id);
        }
    }
    Ok(())
}
