//! Which values run no code and read no binding when they are evaluated:
//! a lazy chunk's declarations of these stay at the top level, and no
//! statement's place in a file depends on them.

use oxc_ast::AstKind;
use oxc_ast::ast::{
    Argument, ArrayExpressionElement, CallExpression, Class, ClassElement, Expression,
    ObjectPropertyKind, PropertyKey,
};
use oxc_semantic::{AstNodes, Scoping};
use oxc_syntax::operator::UnaryOperator;

use crate::babel_view::unparen;

/// Built-in constructors whose call with inert arguments runs no code of
/// the program.
const CONSTRUCTORS: [&str; 5] = ["Set", "Map", "WeakMap", "WeakSet", "RegExp"];

/// Built-in classes a class may extend and stay inert.
const BASES: [&str; 9] = [
    "Error",
    "TypeError",
    "RangeError",
    "SyntaxError",
    "ReferenceError",
    "EvalError",
    "URIError",
    "AggregateError",
    "Object",
];

/// Built-in constants.
const CONSTANTS: [&str; 3] = ["undefined", "NaN", "Infinity"];

/// Node built-in modules: loading one runs no code of the program.
const BUILT_INS: [&str; 31] = [
    "assert",
    "async_hooks",
    "buffer",
    "child_process",
    "crypto",
    "dns",
    "events",
    "fs",
    "fs/promises",
    "http",
    "https",
    "module",
    "net",
    "os",
    "path",
    "path/posix",
    "path/win32",
    "perf_hooks",
    "querystring",
    "readline",
    "stream",
    "stream/promises",
    "string_decoder",
    "timers",
    "timers/promises",
    "tls",
    "tty",
    "url",
    "util",
    "worker_threads",
    "zlib",
];

/// `spec` names a Node built-in whose load runs no code of the program.
pub(crate) fn built_in_module(spec: &str) -> bool {
    BUILT_INS.contains(&spec.strip_prefix("node:").unwrap_or(spec))
}

pub(crate) struct Inert<'s, 'a> {
    pub scoping: &'s Scoping,
    /// With the nodes, a parameter named `require` is the program's
    /// `require` (a CommonJS wrapper's).
    pub nodes: Option<&'s AstNodes<'a>>,
}

impl Inert<'_, '_> {
    /// `e` is the program's `require`: the global, or a parameter.
    fn require(&self, e: &Expression<'_>) -> bool {
        let Expression::Identifier(id) = unparen(e) else {
            return false;
        };
        if id.name != "require" {
            return false;
        }
        let Some(symbol) = self.scoping.get_reference(id.reference_id()).symbol_id() else {
            return true;
        };
        let declared = self.scoping.symbol_declaration(symbol);
        self.nodes.is_some_and(|nodes| {
            matches!(nodes.kind(declared), AstKind::FormalParameter(_))
                || matches!(nodes.parent_kind(declared), AstKind::FormalParameter(_))
        })
    }

    /// `e` is the global `name` (no binding of the chunk shadows it).
    fn global<'a>(&self, e: &'a Expression<'a>) -> Option<&'a str> {
        match unparen(e) {
            Expression::Identifier(id) => self
                .scoping
                .get_reference(id.reference_id())
                .symbol_id()
                .is_none()
                .then_some(id.name.as_str()),
            _ => None,
        }
    }

    fn global_in(&self, e: &Expression<'_>, names: &[&str]) -> bool {
        self.global(e).is_some_and(|n| names.contains(&n))
    }

    /// `<global>.<property>` for one of `members`.
    fn global_member(&self, e: &Expression<'_>, object: &str, members: &[&str]) -> bool {
        match unparen(e) {
            Expression::StaticMemberExpression(m) => {
                self.global_in(&m.object, &[object]) && members.contains(&m.property.name.as_str())
            }
            _ => false,
        }
    }

    /// A number, string, boolean, null or built-in constant, or an
    /// operator on them.
    fn primitive(&self, e: &Expression<'_>) -> bool {
        match unparen(e) {
            Expression::BooleanLiteral(_)
            | Expression::NullLiteral(_)
            | Expression::NumericLiteral(_)
            | Expression::BigIntLiteral(_)
            | Expression::StringLiteral(_) => true,
            Expression::Identifier(_) => self.global_in(e, &CONSTANTS),
            Expression::TemplateLiteral(t) => t.expressions.is_empty(),
            Expression::UnaryExpression(u) => {
                matches!(
                    u.operator,
                    UnaryOperator::Void | UnaryOperator::LogicalNot | UnaryOperator::UnaryNegation
                ) && self.primitive(&u.argument)
            }
            _ => false,
        }
    }

    /// `require("<built-in>")` through the program's `require`.
    fn built_in_load(&self, c: &CallExpression<'_>) -> bool {
        self.require(&c.callee)
            && matches!(c.arguments.as_slice(), [Argument::StringLiteral(s)] if built_in_module(&s.value))
    }

    fn arguments(&self, args: &[Argument<'_>]) -> bool {
        args.iter()
            .all(|a| a.as_expression().is_some_and(|e| self.value(e)))
    }

    /// Evaluating `e` reads no binding of the program and runs none of
    /// its code.
    pub fn value(&self, e: &Expression<'_>) -> bool {
        match unparen(e) {
            Expression::FunctionExpression(_)
            | Expression::ArrowFunctionExpression(_)
            | Expression::RegExpLiteral(_) => true,
            Expression::UnaryExpression(u) => {
                matches!(
                    u.operator,
                    UnaryOperator::Void | UnaryOperator::LogicalNot | UnaryOperator::UnaryNegation
                ) && self.value(&u.argument)
            }
            Expression::ArrayExpression(a) => a.elements.iter().all(|el| match el {
                ArrayExpressionElement::SpreadElement(_) => false,
                ArrayExpressionElement::Elision(_) => true,
                other => other.as_expression().is_some_and(|e| self.value(e)),
            }),
            Expression::ObjectExpression(o) => o.properties.iter().all(|p| match p {
                ObjectPropertyKind::SpreadProperty(_) => false,
                ObjectPropertyKind::ObjectProperty(p) => {
                    self.key(&p.key, p.computed) && self.value(&p.value)
                }
            }),
            Expression::ClassExpression(c) => self.class(c),
            Expression::NewExpression(n) => {
                self.global_in(&n.callee, &CONSTRUCTORS) && self.arguments(&n.arguments)
            }
            Expression::CallExpression(c) => {
                let known = self.global_in(&c.callee, &["Symbol"])
                    || self.global_member(&c.callee, "Symbol", &["for"])
                    || self.global_member(&c.callee, "Object", &["freeze"]);
                !c.optional && (self.built_in_load(c) || known && self.arguments(&c.arguments))
            }
            _ => self.primitive(e),
        }
    }

    /// A property or member key: a plain name, or a computed primitive or
    /// well-known symbol.
    fn key(&self, key: &PropertyKey<'_>, computed: bool) -> bool {
        if !computed {
            return true;
        }
        key.as_expression().is_some_and(|e| {
            self.primitive(e)
                || self.global_member(e, "Symbol", &["iterator", "asyncIterator", "toStringTag"])
        })
    }

    /// Evaluating the class reads no binding of the program and runs
    /// none of its code: it extends nothing or a built-in, and every
    /// static member's value is inert.
    pub fn class(&self, c: &Class<'_>) -> bool {
        let base = c
            .heritage
            .as_ref()
            .is_none_or(|h| self.global_in(&h.expression, &BASES));
        base && c.decorators.is_empty()
            && c.body.body.iter().all(|el| match el {
                ClassElement::StaticBlock(_) => false,
                ClassElement::MethodDefinition(m) => {
                    m.decorators.is_empty() && self.key(&m.key, m.computed)
                }
                ClassElement::PropertyDefinition(p) => {
                    p.decorators.is_empty()
                        && self.key(&p.key, p.computed)
                        && (!p.r#static || p.value.as_ref().is_none_or(|v| self.value(v)))
                }
                ClassElement::AccessorProperty(p) => {
                    p.decorators.is_empty()
                        && self.key(&p.key, p.computed)
                        && (!p.r#static || p.value.as_ref().is_none_or(|v| self.value(v)))
                }
                ClassElement::TSIndexSignature(_) => true,
            })
    }
}
