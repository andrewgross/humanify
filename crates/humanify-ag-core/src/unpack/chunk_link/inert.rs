//! Which values run no code and read no binding when they are evaluated:
//! a lazy chunk's declarations of these stay at the top level.

use oxc_ast::ast::{
    Argument, ArrayExpressionElement, Class, ClassElement, Expression, ObjectPropertyKind,
    PropertyKey,
};
use oxc_semantic::Scoping;
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

pub(super) struct Inert<'s> {
    pub scoping: &'s Scoping,
}

impl Inert<'_> {
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
                known && !c.optional && self.arguments(&c.arguments)
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
