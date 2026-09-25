//! The stage-6 visitors (`src/plugins/babel/babel.ts`), merged into ONE
//! traversal in plugin order as `@babel/core` does
//! (`traverse.visitors.merge`): `convertVoidToUndefined`,
//! `flipComparisonsTheRightWayAround`, `makeNumbersLonger`, then the
//! house-patched `babel-plugin-transform-beautifier` 0.1.1 (its eight
//! visitors, `SequenceExpression` replaced by the patch that skips for-loop
//! init/update/test). `@babel/core`'s internal blockHoist plugin rides the
//! same traversal; it never reorders here (no node carries `_blockHoist`)
//! but its `Block`/`SwitchCase` exit visitors make those nodes "visited
//! types" for `shouldVisit`.
//!
//! The visitors are ported line for line, their semantic bugs INCLUDED
//! (finding #42, 00-control §3 "beautifier bugs"): `a ?? b;` becomes
//! `if (!a) b;`; `void <number>` becomes `undefined` with no scope check;
//! the `.concat` fold writes the COOKED string as the template's raw text
//! and appends a string argument to it twice when the quasi's cooked
//! value is non-empty; `Number(raw)` of a literal with a numeric
//! separator is NaN.

use std::collections::HashSet;

use super::ast::{Binary, Field, Kind, NodeId, Quasi, Tree};
use super::template_raw::template_element_cooked;
use super::traverse::{Engine, Key, PathId, VisitFn, Visitor};

type R<T> = Result<T, String>;

/// Which visitors run, one bit each (for the printer-only gate, and for
/// the planted perturbations that prove the byte gate can see a visitor).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Plugins(pub u16);

impl Plugins {
    pub const NONE: Plugins = Plugins(0);
    pub const VOID_TO_UNDEFINED: u16 = 1 << 0;
    pub const FLIP_COMPARISONS: u16 = 1 << 1;
    pub const LONGER_NUMBERS: u16 = 1 << 2;
    pub const VARIABLE_DECLARATION: u16 = 1 << 3;
    pub const FOR_STATEMENT: u16 = 1 << 4;
    pub const SEQUENCE_EXPRESSION: u16 = 1 << 5;
    pub const LOGICAL_EXPRESSION: u16 = 1 << 6;
    pub const UNARY_EXPRESSION: u16 = 1 << 7;
    pub const CONDITIONAL_EXPRESSION: u16 = 1 << 8;
    pub const IF_STATEMENT: u16 = 1 << 9;
    pub const CALL_EXPRESSION: u16 = 1 << 10;
    /// The stage-6 set: all of them.
    pub const STAGE6: Plugins = Plugins((1 << 11) - 1);

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub fn has(self, bit: u16) -> bool {
        self.0 & bit != 0
    }

    /// The visitor bit named `name` (the plant flag's spelling: the
    /// visitor's node type, or the three small plugins' names).
    pub fn bit(name: &str) -> Option<u16> {
        Some(match name {
            "convertVoidToUndefined" => Plugins::VOID_TO_UNDEFINED,
            "flipComparisonsTheRightWayAround" => Plugins::FLIP_COMPARISONS,
            "makeNumbersLonger" => Plugins::LONGER_NUMBERS,
            "VariableDeclaration" => Plugins::VARIABLE_DECLARATION,
            "ForStatement" => Plugins::FOR_STATEMENT,
            "SequenceExpression" => Plugins::SEQUENCE_EXPRESSION,
            "LogicalExpression" => Plugins::LOGICAL_EXPRESSION,
            "UnaryExpression" => Plugins::UNARY_EXPRESSION,
            "ConditionalExpression" => Plugins::CONDITIONAL_EXPRESSION,
            "IfStatement" => Plugins::IF_STATEMENT,
            "CallExpression" => Plugins::CALL_EXPRESSION,
            _ => return None,
        })
    }
}

/// The merged visitor table for a plugin set.
struct Merged {
    unary: Vec<VisitFn>,
    binary: Vec<VisitFn>,
    numeric: Vec<VisitFn>,
    var_decl: Vec<VisitFn>,
    for_stmt: Vec<VisitFn>,
    sequence: Vec<VisitFn>,
    logical: Vec<VisitFn>,
    conditional: Vec<VisitFn>,
    if_stmt: Vec<VisitFn>,
    call_exit: Vec<VisitFn>,
}

fn fns(on: bool, f: VisitFn) -> Vec<VisitFn> {
    if on { vec![f] } else { Vec::new() }
}

impl Merged {
    fn new(p: Plugins) -> Merged {
        let mut unary = fns(p.has(Plugins::VOID_TO_UNDEFINED), convert_void_to_undefined);
        if p.has(Plugins::UNARY_EXPRESSION) {
            unary.push(unary_expression);
        }
        Merged {
            unary,
            binary: fns(p.has(Plugins::FLIP_COMPARISONS), flip_comparison),
            numeric: fns(p.has(Plugins::LONGER_NUMBERS), make_number_longer),
            var_decl: fns(p.has(Plugins::VARIABLE_DECLARATION), variable_declaration),
            for_stmt: fns(p.has(Plugins::FOR_STATEMENT), for_statement),
            sequence: fns(p.has(Plugins::SEQUENCE_EXPRESSION), sequence_expression),
            logical: fns(p.has(Plugins::LOGICAL_EXPRESSION), logical_expression),
            conditional: fns(
                p.has(Plugins::CONDITIONAL_EXPRESSION),
                conditional_expression,
            ),
            if_stmt: fns(p.has(Plugins::IF_STATEMENT), if_statement),
            call_exit: fns(p.has(Plugins::CALL_EXPRESSION), concat_call),
        }
    }
}

impl Visitor for Merged {
    fn has(&self, kind: &Kind) -> bool {
        let own = match kind {
            Kind::UnaryExpression { .. } => &self.unary,
            Kind::BinaryExpression(_) => &self.binary,
            Kind::NumericLiteral { .. } => &self.numeric,
            Kind::VariableDeclaration { .. } => &self.var_decl,
            Kind::ForStatement { .. } => &self.for_stmt,
            Kind::SequenceExpression { .. } => &self.sequence,
            Kind::LogicalExpression(_) => &self.logical,
            Kind::ConditionalExpression { .. } => &self.conditional,
            Kind::IfStatement { .. } => &self.if_stmt,
            Kind::CallExpression(_) => &self.call_exit,
            // blockHoist's `Block` / `SwitchCase` exits.
            Kind::BlockStatement { .. } | Kind::Program { .. } | Kind::SwitchCase { .. } => {
                return true;
            }
            _ => return false,
        };
        !own.is_empty()
    }

    fn enter(&self, kind: &Kind) -> &[VisitFn] {
        match kind {
            Kind::UnaryExpression { .. } => &self.unary,
            Kind::BinaryExpression(_) => &self.binary,
            Kind::NumericLiteral { .. } => &self.numeric,
            Kind::VariableDeclaration { .. } => &self.var_decl,
            Kind::ForStatement { .. } => &self.for_stmt,
            Kind::SequenceExpression { .. } => &self.sequence,
            Kind::LogicalExpression(_) => &self.logical,
            Kind::ConditionalExpression { .. } => &self.conditional,
            Kind::IfStatement { .. } => &self.if_stmt,
            _ => &[],
        }
    }

    fn exit(&self, kind: &Kind) -> &[VisitFn] {
        match kind {
            Kind::CallExpression(_) => &self.call_exit,
            _ => &[],
        }
    }
}

/// Run the stage-6 visitors over the tree rooted at `root` (a File).
pub fn run(
    tree: &mut Tree,
    root: NodeId,
    plugins: Plugins,
    undefined_scopes: &HashSet<NodeId>,
    requeue_deferred: bool,
) -> R<()> {
    let merged = Merged::new(plugins);
    let mut engine = Engine::new(tree, &merged, undefined_scopes);
    if requeue_deferred {
        engine.plant_requeue_deferred();
    }
    engine.traverse(root)
}

// -- helpers ---------------------------------------------------------------------

/// The path's node (the visitors only run on live paths).
fn node_of(e: &Engine<'_>, p: PathId) -> R<NodeId> {
    e.node(p)
        .ok_or_else(|| "a visitor on a removed path".to_string())
}

fn parent_of(e: &Engine<'_>, p: PathId) -> R<PathId> {
    e.parent_path(p)
        .ok_or_else(|| "a visitor at the root".to_string())
}

fn is_expression_statement(k: &Kind) -> bool {
    matches!(k, Kind::ExpressionStatement { .. })
}

fn expression_statement(e: &mut Engine<'_>, expression: NodeId) -> NodeId {
    e.build(Kind::ExpressionStatement { expression })
}

fn build_if(
    e: &mut Engine<'_>,
    test: NodeId,
    consequent: NodeId,
    alternate: Option<NodeId>,
) -> NodeId {
    e.build(Kind::IfStatement {
        test,
        consequent,
        alternate,
    })
}

fn block(e: &mut Engine<'_>, body: Vec<NodeId>) -> NodeId {
    e.build(Kind::BlockStatement {
        directives: Vec::new(),
        body,
    })
}

// -- convertVoidToUndefined ----------------------------------------------------------

/// `void <NumericLiteral>` → `undefined` (no scope check — finding #42).
fn convert_void_to_undefined(e: &mut Engine<'_>, p: PathId) -> R<()> {
    let node = node_of(e, p)?;
    if let Kind::UnaryExpression {
        operator: "void",
        argument,
    } = e.kind(node)
        && matches!(e.kind(*argument), Kind::NumericLiteral { .. })
    {
        let id = e.build(Kind::Identifier {
            name: "undefined".into(),
        });
        e.replace_with(p, id)?;
    }
    Ok(())
}

// -- flipComparisonsTheRightWayAround ------------------------------------------------

fn flipped(op: &str) -> Option<&'static str> {
    Some(match op {
        "==" => "==",
        "!=" => "!=",
        "===" => "===",
        "!==" => "!==",
        "<" => ">",
        "<=" => ">=",
        ">" => "<",
        ">=" => "<=",
        _ => return None,
    })
}

/// A literal (template literals included) on the left of a comparison
/// with a non-literal on the right swaps sides.
fn flip_comparison(e: &mut Engine<'_>, p: PathId) -> R<()> {
    let node = node_of(e, p)?;
    let Kind::BinaryExpression(b) = e.kind(node) else {
        return Ok(());
    };
    let (op, left, right) = (b.operator, b.left, b.right);
    if e.kind(left).is_literal()
        && !e.kind(right).is_literal()
        && let Some(op) = flipped(op)
    {
        let id = e.build(Kind::BinaryExpression(Binary {
            operator: op,
            left: right,
            right: left,
        }));
        e.replace_with(p, id)?;
    }
    Ok(())
}

// -- makeNumbersLonger --------------------------------------------------------------

/// A numeric literal whose raw text contains `e` is replaced by a raw-less
/// literal of `Number(raw)` (printed through `value + ""`): `5e3` → `5000`,
/// and `0xe1` → `225` (the hex digit e counts). `Number()` rejects a
/// numeric separator: `1_0e3` → NaN.
fn make_number_longer(e: &mut Engine<'_>, p: PathId) -> R<()> {
    let node = node_of(e, p)?;
    let Kind::NumericLiteral {
        value,
        raw: Some(raw),
    } = e.kind(node)
    else {
        return Ok(());
    };
    if !raw.contains('e') {
        return Ok(());
    }
    let value = if raw.contains('_') { f64::NAN } else { *value };
    let id = e.build(Kind::NumericLiteral { value, raw: None });
    e.replace_with(p, id)
}

// -- the beautifier -----------------------------------------------------------------

/// `var a, b = 1, c` → `var a, c; var b = 1;` (the init-less declarators
/// first, one declaration per initialized declarator); in a for-init, all
/// but the last `var` declarator move before the loop.
fn variable_declaration(e: &mut Engine<'_>, p: PathId) -> R<()> {
    let node = node_of(e, p)?;
    let Kind::VariableDeclaration { kind, declarations } = e.kind(node).clone() else {
        return Ok(());
    };
    if declarations.len() <= 1 {
        return Ok(());
    }
    let pp = parent_of(e, p)?;
    if e.path_is(pp, |k| matches!(k, Kind::ForStatement { .. }))
        && e.key(p) == Key::Field(Field::Init)
    {
        if kind != "var" {
            return Ok(());
        }
        let (last, rest) = declarations.split_last().expect("len > 1");
        let before = e.build(Kind::VariableDeclaration {
            kind,
            declarations: rest.to_vec(),
        });
        e.insert_before(pp, vec![before])?;
        let single = e.build(Kind::VariableDeclaration {
            kind,
            declarations: vec![*last],
        });
        return e.replace_with(p, single);
    }
    let (non_empty, empty): (Vec<NodeId>, Vec<NodeId>) = declarations
        .iter()
        .partition(|&&d| matches!(e.kind(d), Kind::VariableDeclarator { init: Some(_), .. }));
    if non_empty.is_empty() {
        return Ok(());
    }
    let mut decls = Vec::new();
    if !empty.is_empty() {
        decls.push(e.build(Kind::VariableDeclaration {
            kind,
            declarations: empty,
        }));
    }
    for d in non_empty {
        decls.push(e.build(Kind::VariableDeclaration {
            kind,
            declarations: vec![d],
        }));
    }
    let export = e.path_is(pp, |k| {
        matches!(
            k,
            Kind::ExportNamedDeclaration { .. }
                | Kind::ExportDefaultDeclaration { .. }
                | Kind::ExportAllDeclaration { .. }
        )
    });
    if !export {
        return e.replace_with_multiple(p, decls);
    }
    let exports = decls
        .into_iter()
        .map(|d| {
            e.build(Kind::ExportNamedDeclaration {
                declaration: Some(d),
                specifiers: Vec::new(),
                source: None,
                attributes: Vec::new(),
            })
        })
        .collect();
    e.replace_with_multiple(pp, exports)
}

/// `for (…) x;` → `for (…) { x; }` (only ForStatement).
fn for_statement(e: &mut Engine<'_>, p: PathId) -> R<()> {
    let body_path = e.get(p, Field::Body)?;
    let body = node_of(e, body_path)?;
    if matches!(e.kind(body), Kind::BlockStatement { .. }) {
        return Ok(());
    }
    let body_path = e.get(p, Field::Body)?;
    let body = node_of(e, body_path)?;
    let b = block(e, vec![body]);
    e.set(p, Field::Body, b)
}

/// The house patch: `a, b, c;` → `a; b; c;` and `return a, b;` →
/// `a; return b;` — except in a for-loop's init / test / update.
fn sequence_expression(e: &mut Engine<'_>, p: PathId) -> R<()> {
    let node = node_of(e, p)?;
    let Kind::SequenceExpression { expressions } = e.kind(node).clone() else {
        return Ok(());
    };
    let pp = parent_of(e, p)?;
    if !e.path_is(pp, Kind::is_statement) {
        return Ok(());
    }
    if e.path_is(pp, |k| matches!(k, Kind::ForStatement { .. }))
        && matches!(
            e.key(p),
            Key::Field(Field::Update | Field::Init | Field::Test)
        )
    {
        return Ok(());
    }
    let (last, rest) = expressions.split_last().ok_or("an empty sequence")?;
    let stmts = rest.iter().map(|&x| expression_statement(e, x)).collect();
    e.insert_before(pp, stmts)?;
    e.replace_with(p, *last)
}

/// `a && b;` → `if (a) b;`; ANY other logical operator (`||` and `??`
/// alike — finding #42) → `if (!a) b;`.
fn logical_expression(e: &mut Engine<'_>, p: PathId) -> R<()> {
    let node = node_of(e, p)?;
    let Kind::LogicalExpression(b) = e.kind(node) else {
        return Ok(());
    };
    let (op, left, right) = (b.operator, b.left, b.right);
    let pp = parent_of(e, p)?;
    if !e.path_is(pp, is_expression_statement) {
        return Ok(());
    }
    let test = if op == "&&" {
        left
    } else {
        e.build(Kind::UnaryExpression {
            operator: "!",
            argument: left,
        })
    };
    let consequent = expression_statement(e, right);
    let stmt = build_if(e, test, consequent, None);
    e.replace_with(pp, stmt)
}

/// `!0` → `true`, `!<n>` → `false`; `void <literal>` → `undefined`
/// unless a scope declares `undefined`; a statement-level `void x` is
/// split: `x;` before the statement, then `return;` / `undefined`.
fn unary_expression(e: &mut Engine<'_>, p: PathId) -> R<()> {
    let node = node_of(e, p)?;
    let Kind::UnaryExpression { operator, argument } = *e.kind(node) else {
        return Ok(());
    };
    if operator == "!"
        && let Kind::NumericLiteral { value, .. } = e.kind(argument)
    {
        let value = *value == 0.0;
        let b = e.build(Kind::BooleanLiteral { value });
        return e.replace_with(p, b);
    }
    if operator != "void" {
        return Ok(());
    }
    let arg_path = e.get(p, Field::Argument)?;
    if e.path_is(arg_path, Kind::is_literal) {
        if !e.has_undefined_binding(p) {
            let id = e.build(Kind::Identifier {
                name: "undefined".into(),
            });
            e.replace_with(p, id)?;
        }
        return Ok(());
    }
    let pp = parent_of(e, p)?;
    if !e.path_is(pp, Kind::is_statement) {
        return Ok(());
    }
    let stmt = expression_statement(e, argument);
    e.insert_before(pp, vec![stmt])?;
    if e.path_is(pp, |k| matches!(k, Kind::ReturnStatement { .. })) {
        e.remove(p)
    } else {
        if !e.has_undefined_binding(p) {
            let id = e.build(Kind::Identifier {
                name: "undefined".into(),
            });
            e.replace_with(p, id)?;
        }
        Ok(())
    }
}

/// `v = c ? a : b;` / `return c ? a : b;` / `c ? a : b;` → if/else.
fn conditional_expression(e: &mut Engine<'_>, p: PathId) -> R<()> {
    let node = node_of(e, p)?;
    let Kind::ConditionalExpression {
        test,
        consequent,
        alternate,
    } = *e.kind(node)
    else {
        return Ok(());
    };
    let pp = parent_of(e, p)?;
    let ppp = e.parent_path(pp);
    if e.path_is(pp, |k| matches!(k, Kind::AssignmentExpression(_)))
        && ppp.is_some_and(|q| e.path_is(q, is_expression_statement))
    {
        let assign = node_of(e, pp)?;
        let Kind::AssignmentExpression(b) = e.kind(assign) else {
            unreachable!("checked above");
        };
        let (op, left) = (b.operator, b.left);
        if !matches!(e.kind(left), Kind::Identifier { .. }) {
            return Ok(());
        }
        let a1 = e.build(Kind::AssignmentExpression(Binary {
            operator: op,
            left,
            right: consequent,
        }));
        let a2 = e.build(Kind::AssignmentExpression(Binary {
            operator: op,
            left,
            right: alternate,
        }));
        let s1 = expression_statement(e, a1);
        let s2 = expression_statement(e, a2);
        let stmt = build_if(e, test, s1, Some(s2));
        return e.replace_with(ppp.expect("checked above"), stmt);
    }
    if e.path_is(pp, |k| matches!(k, Kind::ReturnStatement { .. })) {
        let r1 = e.build(Kind::ReturnStatement {
            argument: Some(consequent),
        });
        let r2 = e.build(Kind::ReturnStatement {
            argument: Some(alternate),
        });
        let stmt = build_if(e, test, r1, Some(r2));
        return e.replace_with(pp, stmt);
    }
    if e.path_is(pp, is_expression_statement) {
        let s1 = expression_statement(e, consequent);
        let s2 = expression_statement(e, alternate);
        let stmt = build_if(e, test, s1, Some(s2));
        return e.replace_with(pp, stmt);
    }
    Ok(())
}

/// Wrap if-branches in blocks, except an else branch that will itself
/// become an if (`else if`): an if, a conditional or logical expression
/// statement, or `id = c ? a : b;`.
fn if_statement(e: &mut Engine<'_>, p: PathId) -> R<()> {
    let node = node_of(e, p)?;
    let Kind::IfStatement {
        consequent,
        alternate,
        ..
    } = *e.kind(node)
    else {
        return Ok(());
    };
    if !matches!(e.kind(consequent), Kind::BlockStatement { .. }) {
        let b = block(e, vec![consequent]);
        e.set(p, Field::Consequent, b)?;
    }
    let Some(alternate) = alternate else {
        return Ok(());
    };
    match e.kind(alternate) {
        Kind::BlockStatement { .. } | Kind::IfStatement { .. } => return Ok(()),
        Kind::ExpressionStatement { expression } => match e.kind(*expression) {
            Kind::ConditionalExpression { .. } | Kind::LogicalExpression(_) => return Ok(()),
            Kind::AssignmentExpression(b)
                if matches!(e.kind(b.left), Kind::Identifier { .. })
                    && matches!(e.kind(b.right), Kind::ConditionalExpression { .. }) =>
            {
                return Ok(());
            }
            _ => {}
        },
        _ => {}
    }
    let b = block(e, vec![alternate]);
    e.set(p, Field::Alternate, b)
}

/// (exit) `"s".concat(x)` → `` `s${x}` ``; `"a".concat("b")` → `"ab"`;
/// `` `…`.concat(x) `` → `` `…${x}` ``; `` `…`.concat("s") `` appends to
/// the last quasi's raw — TWICE when its cooked value is non-empty.
fn concat_call(e: &mut Engine<'_>, p: PathId) -> R<()> {
    let node = node_of(e, p)?;
    let Kind::CallExpression(c) = e.kind(node) else {
        return Ok(());
    };
    let Kind::MemberExpression(m) = e.kind(c.callee) else {
        return Ok(());
    };
    if e.kind(m.property).identifier_name() != Some("concat") || c.arguments.len() != 1 {
        return Ok(());
    }
    let (object, arg) = (m.object, c.arguments[0]);
    if !e.kind(arg).is_expression() {
        return Ok(());
    }
    if let Kind::StringLiteral { value, .. } = e.kind(object) {
        let value = value.clone();
        if let Kind::StringLiteral { value: tail, .. } = e.kind(arg) {
            let joined = format!("{value}{tail}");
            let s = e.build(Kind::StringLiteral {
                value: joined,
                raw: None,
            });
            return e.replace_with(p, s);
        }
        // `t.templateElement({ raw: value, cooked: value })`: the
        // validator throws on a raw that would end the template and
        // recomputes cooked from it (finding #43).
        let cooked = template_element_cooked(&value)?;
        let t = e.build(Kind::TemplateLiteral {
            quasis: vec![
                Quasi { raw: value, cooked },
                Quasi {
                    raw: String::new(),
                    cooked: Some(String::new()),
                },
            ],
            expressions: vec![arg],
        });
        return e.replace_with(p, t);
    }
    if !matches!(e.kind(object), Kind::TemplateLiteral { .. }) {
        return Ok(());
    }
    let tail = match e.kind(arg) {
        Kind::StringLiteral { value, .. } => Some(value.clone()),
        _ => None,
    };
    let Kind::TemplateLiteral {
        quasis,
        expressions,
    } = e.tree.kind_mut(object)
    else {
        unreachable!("checked above");
    };
    let string_tail = tail.is_some();
    match tail {
        Some(tail) => {
            let last = quasis.last_mut().ok_or("a template without quasis")?;
            last.raw.push_str(&tail);
            if last.cooked.as_deref().is_some_and(|c| !c.is_empty()) {
                last.raw.push_str(&tail);
            }
        }
        None => {
            quasis.push(Quasi {
                raw: String::new(),
                cooked: Some(String::new()),
            });
            expressions.push(arg);
        }
    }
    if string_tail {
        e.replace_with(p, object)?;
    }
    e.replace_with(p, object)
}
