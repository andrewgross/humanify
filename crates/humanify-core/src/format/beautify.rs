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
//! The visitors were ported line for line; since the cutover the formatter
//! owes CORRECTNESS, not Babel's bytes, so the TS beautifier's semantic
//! bugs are fixed here (findings #42, #44, #45; 00-control §3 "beautifier
//! bugs"): `a ?? b;` becomes `if (a == null) b;` (was `if (!a) b;`);
//! `void <number>` is kept where a scope declares `undefined`; the
//! `.concat` fold builds a NEW template (never mutating the callee's, which
//! folded an argument twice), escapes its raw text (a backtick, `${`, a
//! backslash) and appends a string tail once; a labeled multi-declarator
//! `var` is left whole (splitting it threw in a loop body and dropped the
//! label elsewhere); a longer number keeps its parsed value (the TS's
//! `Number(raw)` made a literal with a numeric separator NaN).

use std::collections::HashSet;

use super::ast::{Binary, Field, Kind, NodeId, Quasi, Tree};
use super::template_raw::raw_for_string;
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
    plant: Option<super::Plant>,
) -> R<()> {
    let merged = Merged::new(plugins);
    let mut engine = Engine::new(tree, &merged, undefined_scopes);
    engine.plant(plant);
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

/// A literal that runs no code: any literal but a template literal with
/// expressions.
fn is_inert_literal(k: &Kind) -> bool {
    match k {
        Kind::TemplateLiteral { expressions, .. } => expressions.is_empty(),
        k => k.is_literal(),
    }
}

/// A loop statement whose head expressions run once PER TURN (a for's
/// test / update, a while's or do-while's test) or after the body (a
/// do-while's test): code in its head must not be hoisted before it.
fn is_loop_with_repeated_head(k: &Kind) -> bool {
    matches!(
        k,
        Kind::ForStatement { .. } | Kind::WhileStatement { .. } | Kind::DoWhileStatement { .. }
    )
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

/// `void <NumericLiteral>` → `undefined`, unless a scope declares
/// `undefined` (there the identifier is that binding, not the global —
/// finding #42).
fn convert_void_to_undefined(e: &mut Engine<'_>, p: PathId) -> R<()> {
    let node = node_of(e, p)?;
    if let Kind::UnaryExpression {
        operator: "void",
        argument,
    } = e.kind(node)
        && matches!(e.kind(*argument), Kind::NumericLiteral { .. })
        && !e.has_undefined_binding(p)
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

/// A literal that runs no code (a template literal only without
/// expressions) on the left of a comparison with a non-literal on the
/// right swaps sides. A template with expressions stays put — swapping
/// would run the right side's code first (the TS flipped any literal, so
/// `` `${f()}` === g() `` ran `g` first) — and so does a regex under a
/// RELATIONAL operator, where both sides are converted to primitives in
/// source order (the regex's `toString` would run after the right side's
/// `valueOf`); under `==` only one side is ever converted.
fn flip_comparison(e: &mut Engine<'_>, p: PathId) -> R<()> {
    let node = node_of(e, p)?;
    let Kind::BinaryExpression(b) = e.kind(node) else {
        return Ok(());
    };
    let (op, left, right) = (b.operator, b.left, b.right);
    let relational = matches!(op, "<" | "<=" | ">" | ">=");
    if is_inert_literal(e.kind(left))
        && !(relational && matches!(e.kind(left), Kind::RegExpLiteral { .. }))
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
/// and `0xe1` → `225` (the hex digit e counts). The parsed value is used,
/// so a numeric separator keeps its number (`1_0e3` → `10000`; the TS's
/// `Number(raw)` made it NaN).
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
    let value = *value;
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
    if e.path_is(pp, |k| matches!(k, Kind::LabeledStatement { .. })) {
        // A labeled declaration stays whole (finding #44): splitting it
        // lifts the declarations out of the label, which dropped the label
        // and, as a loop's body, left a null body behind (a throw).
        return Ok(());
    }
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
/// `a; return b;` — except in a loop's head (a for's init / test / update,
/// a while's or do-while's test).
fn sequence_expression(e: &mut Engine<'_>, p: PathId) -> R<()> {
    let node = node_of(e, p)?;
    let Kind::SequenceExpression { expressions } = e.kind(node).clone() else {
        return Ok(());
    };
    let pp = parent_of(e, p)?;
    if !e.path_is(pp, Kind::is_statement) {
        return Ok(());
    }
    // A loop head's sequence stays (the house patch skipped a for-loop's;
    // a while / do-while test was hoisted too, running once — the TS bug).
    if e.path_is(pp, is_loop_with_repeated_head) {
        return Ok(());
    }
    let (last, rest) = expressions.split_last().ok_or("an empty sequence")?;
    let stmts = rest.iter().map(|&x| expression_statement(e, x)).collect();
    e.insert_before(pp, stmts)?;
    e.replace_with(p, *last)
}

/// `a && b;` → `if (a) b;`, `a || b;` → `if (!a) b;`, `a ?? b;` →
/// `if (a == null) b;` (finding #42: the TS wrote `if (!a)`, which also
/// runs `b` for `0` / `""` / `false`). `== null` is exactly the test `??`
/// makes — true for null and undefined only (`document.all` aside, which
/// Node does not have) — evaluates `a` once, and keeps `??` statements in
/// the same `if` shape as the `&&` / `||` ones.
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
    let test = match op {
        "&&" => left,
        "??" => {
            let null = e.build(Kind::NullLiteral);
            e.build(Kind::BinaryExpression(Binary {
                operator: "==",
                left,
                right: null,
            }))
        }
        _ => e.build(Kind::UnaryExpression {
            operator: "!",
            argument: left,
        }),
    };
    let consequent = expression_statement(e, right);
    let stmt = build_if(e, test, consequent, None);
    e.replace_with(pp, stmt)
}

/// `!0` → `true`, `!<n>` → `false`; `void <inert literal>` → `undefined`
/// unless a scope declares `undefined`; a statement-level `void x` is
/// split: `x;` before the statement, then `return;` / `undefined` — not in
/// a loop head, and (outside a return) not where `undefined` is local.
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
    // A template with expressions is not inert: `void `${f()}`` runs `f`
    // (the TS dropped it); it takes the statement path below.
    if e.path_is(arg_path, is_inert_literal) {
        if !e.has_undefined_binding(p) {
            let id = e.build(Kind::Identifier {
                name: "undefined".into(),
            });
            e.replace_with(p, id)?;
        }
        return Ok(());
    }
    let pp = parent_of(e, p)?;
    // Not out of a loop head (the TS hoisted a for-update's `void a()`
    // before the loop, running it once).
    if !e.path_is(pp, Kind::is_statement) || e.path_is(pp, is_loop_with_repeated_head) {
        return Ok(());
    }
    let is_return = e.path_is(pp, |k| matches!(k, Kind::ReturnStatement { .. }));
    // Where `undefined` is a local binding the `void` cannot be replaced,
    // so it is not split either (the TS hoisted the argument AND kept the
    // `void`, running it twice).
    if !is_return && e.has_undefined_binding(p) {
        return Ok(());
    }
    let stmt = expression_statement(e, argument);
    e.insert_before(pp, vec![stmt])?;
    if is_return {
        e.remove(p)
    } else {
        let id = e.build(Kind::Identifier {
            name: "undefined".into(),
        });
        e.replace_with(p, id)
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
/// `` `…`.concat(x) `` → `` `…${x}` ``; `` `…`.concat("s") `` → `` `…s` ``.
///
/// The fold always builds a NEW template (finding #45: the TS pushed onto
/// the callee's template in place, so a call visited twice — a statement
/// logical's `if` is built around the call before its children are
/// visited — appended its argument twice and ran it twice). A string's
/// text enters the template's raw ESCAPED ([`raw_for_string`]: the literal's
/// own source escapes, a backtick and `${` escaped — finding #44's throw,
/// and the TS's cooked-as-raw that lost backslashes), and a string tail is
/// appended once (the TS appended it twice when the quasi's cooked value
/// was non-empty).
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
    let tail = match e.kind(arg) {
        Kind::StringLiteral { value, raw } => Some((value.clone(), raw.clone())),
        _ => None,
    };
    let folded = match e.kind(object).clone() {
        Kind::StringLiteral { value, raw } => match tail {
            Some((tail, _)) => Kind::StringLiteral {
                value: format!("{value}{tail}"),
                raw: None,
            },
            None => Kind::TemplateLiteral {
                quasis: vec![
                    Quasi {
                        raw: raw_for_string(&value, raw.as_deref()),
                        cooked: Some(value),
                    },
                    empty_quasi(),
                ],
                expressions: vec![arg],
            },
        },
        Kind::TemplateLiteral {
            mut quasis,
            mut expressions,
        } => {
            match tail {
                Some((tail, raw)) => {
                    let last = quasis.last_mut().ok_or("a template without quasis")?;
                    last.raw.push_str(&raw_for_string(&tail, raw.as_deref()));
                    if let Some(cooked) = last.cooked.as_mut() {
                        cooked.push_str(&tail);
                    }
                }
                None => {
                    quasis.push(empty_quasi());
                    expressions.push(arg);
                }
            }
            Kind::TemplateLiteral {
                quasis,
                expressions,
            }
        }
        _ => return Ok(()),
    };
    let id = e.build(folded);
    e.replace_with(p, id)
}

fn empty_quasi() -> Quasi {
    Quasi {
        raw: String::new(),
        cooked: Some(String::new()),
    }
}
