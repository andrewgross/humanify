//! oxc's AST → the Babel-shaped [`ast`](super::ast) the generator prints.
//!
//! The translations are the ones the port has already pinned (lesson 2,
//! 14): parentheses become `extra.parenthesized`; an optional chain's links
//! are `Optional*` iff their own `optional` or their object/callee spine
//! reaches an optional link (never through parentheses); object/class
//! methods are single method nodes; `import(…)` is a call on `Import`;
//! `#x in o` is a BinaryExpression over a PrivateName. Lines are Babel's:
//! `\r\n`, `\r`, `\n`, U+2028 and U+2029 each end a line.
//!
//! Anything the desugar never meets in a CommonJS tree (TypeScript, JSX,
//! decorators, module declarations) is an `Err`, never a guess.

use oxc_ast::ast::{
    Argument, ArrayExpressionElement, ArrowFunctionBody, AssignmentTarget,
    AssignmentTargetMaybeDefault, AssignmentTargetProperty, BindingPattern, ChainElement,
    ClassElement, Expression, ForStatementInit, ForStatementLeft, FormalParameters, Function,
    FunctionBody, MemberExpression, MethodDefinitionKind, ObjectPropertyKind, Program, PropertyKey,
    PropertyKind, SimpleAssignmentTarget, Statement, VariableDeclaration, VariableDeclarationKind,
};
use oxc_span::{GetSpan, Span};

use super::ast::{Binary, Call, Class, Field, Func, Kind, Loc, Member, Method, Node, P};

type R<T> = Result<T, String>;

/// Babel's line numbering over one text.
pub struct Lines {
    /// Byte offset where each line starts (line 1 at index 0).
    starts: Vec<usize>,
}

impl Lines {
    pub fn new(text: &str) -> Lines {
        let bytes = text.as_bytes();
        let mut starts = vec![0];
        let mut i = 0;
        while i < bytes.len() {
            match bytes[i] {
                b'\r' => {
                    i += if bytes.get(i + 1) == Some(&b'\n') {
                        2
                    } else {
                        1
                    };
                    starts.push(i);
                }
                b'\n' => {
                    i += 1;
                    starts.push(i);
                }
                // U+2028 / U+2029: E2 80 A8 / E2 80 A9.
                0xE2 if bytes.get(i + 1) == Some(&0x80)
                    && matches!(bytes.get(i + 2), Some(0xA8 | 0xA9)) =>
                {
                    i += 3;
                    starts.push(i);
                }
                _ => i += 1,
            }
        }
        Lines { starts }
    }

    /// The 1-based line holding byte `pos`.
    pub fn line_of(&self, pos: u32) -> u32 {
        let pos = pos as usize;
        (self.starts.partition_point(|&s| s <= pos)) as u32
    }
}

pub struct Converter<'t> {
    text: &'t str,
    lines: Lines,
}

fn b(n: Node) -> P {
    Box::new(n)
}

impl<'t> Converter<'t> {
    pub fn new(text: &'t str) -> Converter<'t> {
        Converter {
            text,
            lines: Lines::new(text),
        }
    }

    fn loc(&self, span: Span) -> Option<Loc> {
        Some(Loc {
            start: self.lines.line_of(span.start),
            end: self.lines.line_of(span.end),
        })
    }

    fn node(&self, kind: Kind, span: Span) -> Node {
        Node::new(kind, self.loc(span))
    }

    fn slice(&self, span: Span) -> String {
        self.text[span.start as usize..span.end as usize].to_string()
    }

    fn ident(&self, name: &str, span: Span) -> Node {
        self.node(
            Kind::Identifier {
                name: name.to_string(),
            },
            span,
        )
    }

    // -- program + statements ------------------------------------------------

    pub fn program(&self, program: &Program<'_>) -> R<Node> {
        if program.source_type.is_module() {
            return Err("module source (not a desugar input)".into());
        }
        let directives = program
            .directives
            .iter()
            .map(|d| self.directive(d))
            .collect();
        let body = self.statements(&program.body)?;
        Ok(Node::new(
            Kind::Program {
                interpreter: program.hashbang.as_ref().map(|h| h.value.to_string()),
                directives,
                body,
            },
            None,
        ))
    }

    fn directive(&self, d: &oxc_ast::ast::Directive<'_>) -> Node {
        let lit = self.node(
            Kind::DirectiveLiteral {
                raw: self.slice(d.expression.span),
            },
            d.expression.span,
        );
        self.node(Kind::Directive { value: b(lit) }, d.span)
    }

    fn statements(&self, stmts: &[Statement<'_>]) -> R<Vec<Node>> {
        stmts.iter().map(|s| self.statement(s)).collect()
    }

    fn function_body(&self, body: &FunctionBody<'_>) -> R<Node> {
        Ok(self.node(
            Kind::BlockStatement {
                directives: body.directives.iter().map(|d| self.directive(d)).collect(),
                body: self.statements(&body.statements)?,
            },
            body.span,
        ))
    }

    fn block(&self, block: &oxc_ast::ast::BlockStatement<'_>) -> R<Node> {
        Ok(self.node(
            Kind::BlockStatement {
                directives: Vec::new(),
                body: self.statements(&block.body)?,
            },
            block.span,
        ))
    }

    fn opt_expr(&self, e: Option<&Expression<'_>>) -> R<Option<P>> {
        e.map(|e| self.expr(e).map(b)).transpose()
    }

    fn label(&self, l: Option<&oxc_ast::ast::LabelIdentifier<'_>>) -> Option<P> {
        l.map(|l| b(self.ident(&l.name, l.span)))
    }

    fn statement(&self, s: &Statement<'_>) -> R<Node> {
        let span = s.span();
        match s {
            Statement::BlockStatement(bl) => self.block(bl),
            Statement::VariableDeclaration(d) => self.var_decl(d),
            Statement::FunctionDeclaration(f) => {
                Ok(self.node(Kind::FunctionDeclaration(self.func(f)?), f.span))
            }
            Statement::ClassDeclaration(c) => {
                Ok(self.node(Kind::ClassDeclaration(self.class(c)?), c.span))
            }
            Statement::DoWhileStatement(_)
            | Statement::ForInStatement(_)
            | Statement::ForOfStatement(_)
            | Statement::ForStatement(_)
            | Statement::WhileStatement(_) => Ok(self.node(self.loop_statement(s)?, span)),
            Statement::SwitchStatement(st) => Ok(self.node(self.switch_statement(st)?, span)),
            Statement::TryStatement(st) => Ok(self.node(self.try_statement(st)?, span)),
            _ => Ok(self.node(self.simple_statement(s)?, span)),
        }
    }

    fn simple_statement(&self, s: &Statement<'_>) -> R<Kind> {
        Ok(match s {
            Statement::BreakStatement(st) => Kind::BreakStatement {
                label: self.label(st.label.as_ref()),
            },
            Statement::ContinueStatement(st) => Kind::ContinueStatement {
                label: self.label(st.label.as_ref()),
            },
            Statement::DebuggerStatement(_) => Kind::DebuggerStatement,
            Statement::EmptyStatement(_) => Kind::EmptyStatement,
            Statement::ExpressionStatement(st) => Kind::ExpressionStatement {
                expression: b(self.expr(&st.expression)?),
            },
            Statement::IfStatement(st) => Kind::IfStatement {
                test: b(self.expr(&st.test)?),
                consequent: b(self.statement(&st.consequent)?),
                alternate: self.opt_statement(st.alternate.as_ref())?,
            },
            Statement::LabeledStatement(st) => Kind::LabeledStatement {
                label: b(self.ident(&st.label.name, st.label.span)),
                body: b(self.statement(&st.body)?),
            },
            Statement::ReturnStatement(st) => Kind::ReturnStatement {
                argument: self.opt_expr(st.argument.as_ref())?,
            },
            Statement::ThrowStatement(st) => Kind::ThrowStatement {
                argument: b(self.expr(&st.argument)?),
            },
            Statement::WithStatement(st) => Kind::WithStatement {
                object: b(self.expr(&st.object)?),
                body: b(self.statement(&st.body)?),
            },
            other => {
                return Err(format!(
                    "unsupported statement for the desugar at {}..{}",
                    other.span().start,
                    other.span().end
                ));
            }
        })
    }

    fn opt_statement(&self, s: Option<&Statement<'_>>) -> R<Option<P>> {
        s.map(|s| self.statement(s).map(b)).transpose()
    }

    fn loop_statement(&self, s: &Statement<'_>) -> R<Kind> {
        Ok(match s {
            Statement::DoWhileStatement(st) => Kind::DoWhileStatement {
                body: b(self.statement(&st.body)?),
                test: b(self.expr(&st.test)?),
            },
            Statement::WhileStatement(st) => Kind::WhileStatement {
                test: b(self.expr(&st.test)?),
                body: b(self.statement(&st.body)?),
            },
            Statement::ForInStatement(st) => Kind::ForInStatement {
                left: b(self.for_left(&st.left)?),
                right: b(self.expr(&st.right)?),
                body: b(self.statement(&st.body)?),
            },
            Statement::ForOfStatement(st) => Kind::ForOfStatement {
                is_await: st.r#await,
                left: b(self.for_left(&st.left)?),
                right: b(self.expr(&st.right)?),
                body: b(self.statement(&st.body)?),
            },
            Statement::ForStatement(st) => Kind::ForStatement {
                init: self.for_init(st.init.as_ref())?,
                test: self.opt_expr(st.test.as_ref())?,
                update: self.opt_expr(st.update.as_ref())?,
                body: b(self.statement(&st.body)?),
            },
            _ => unreachable!("dispatched on the loop kinds"),
        })
    }

    fn for_init(&self, init: Option<&ForStatementInit<'_>>) -> R<Option<P>> {
        Ok(match init {
            None => None,
            Some(ForStatementInit::VariableDeclaration(d)) => Some(b(self.var_decl(d)?)),
            Some(other) => Some(b(self.expr(other.to_expression())?)),
        })
    }

    fn switch_statement(&self, st: &oxc_ast::ast::SwitchStatement<'_>) -> R<Kind> {
        let mut cases = Vec::with_capacity(st.cases.len());
        for c in &st.cases {
            cases.push(self.node(
                Kind::SwitchCase {
                    test: self.opt_expr(c.test.as_ref())?,
                    consequent: self.statements(&c.consequent)?,
                },
                c.span,
            ));
        }
        Ok(Kind::SwitchStatement {
            discriminant: b(self.expr(&st.discriminant)?),
            cases,
        })
    }

    fn catch_clause(&self, h: &oxc_ast::ast::CatchClause<'_>) -> R<P> {
        let param = match &h.param {
            Some(p) => Some(b(self.binding(&p.pattern)?)),
            None => None,
        };
        Ok(b(self.node(
            Kind::CatchClause {
                param,
                body: b(self.block(&h.body)?),
            },
            h.span,
        )))
    }

    fn try_statement(&self, st: &oxc_ast::ast::TryStatement<'_>) -> R<Kind> {
        let handler = match &st.handler {
            Some(h) => Some(self.catch_clause(h)?),
            None => None,
        };
        let finalizer = match &st.finalizer {
            Some(f) => Some(b(self.block(f)?)),
            None => None,
        };
        Ok(Kind::TryStatement {
            block: b(self.block(&st.block)?),
            handler,
            finalizer,
        })
    }

    fn var_decl(&self, d: &VariableDeclaration<'_>) -> R<Node> {
        let kind = match d.kind {
            VariableDeclarationKind::Var => "var",
            VariableDeclarationKind::Let => "let",
            VariableDeclarationKind::Const => "const",
            VariableDeclarationKind::Using => "using",
            VariableDeclarationKind::AwaitUsing => "await using",
        };
        let declarations = d
            .declarations
            .iter()
            .map(|v| {
                Ok(self.node(
                    Kind::VariableDeclarator {
                        id: b(self.binding(&v.id)?),
                        init: self.opt_expr(v.init.as_ref())?,
                    },
                    v.span,
                ))
            })
            .collect::<R<Vec<_>>>()?;
        Ok(self.node(Kind::VariableDeclaration { kind, declarations }, d.span))
    }

    fn for_left(&self, left: &ForStatementLeft<'_>) -> R<Node> {
        match left {
            ForStatementLeft::VariableDeclaration(d) => self.var_decl(d),
            other => self.target(other.to_assignment_target()),
        }
    }

    // -- functions + classes ---------------------------------------------------

    fn params(&self, params: &FormalParameters<'_>) -> R<Vec<Node>> {
        let mut out = Vec::new();
        for p in &params.items {
            if !p.decorators.is_empty() {
                return Err("decorators".into());
            }
            let pattern = self.binding(&p.pattern)?;
            out.push(match &p.initializer {
                None => pattern,
                Some(init) => self.node(
                    Kind::AssignmentPattern {
                        left: b(pattern),
                        right: b(self.expr(init)?),
                    },
                    p.span,
                ),
            });
        }
        if let Some(rest) = &params.rest {
            out.push(self.node(
                Kind::RestElement {
                    argument: b(self.binding(&rest.rest.argument)?),
                },
                rest.rest.span,
            ));
        }
        Ok(out)
    }

    fn func(&self, f: &Function<'_>) -> R<Func> {
        let body = f.body.as_ref().ok_or("a function without a body")?;
        Ok(Func {
            id: f.id.as_ref().map(|id| b(self.ident(&id.name, id.span))),
            params: self.params(&f.params)?,
            body: b(self.function_body(body)?),
            is_async: f.r#async,
            generator: f.generator,
        })
    }

    fn key(&self, key: &PropertyKey<'_>) -> R<Node> {
        match key {
            PropertyKey::StaticIdentifier(id) => Ok(self.ident(&id.name, id.span)),
            PropertyKey::PrivateIdentifier(id) => Ok(self.private_name(&id.name, id.span)),
            other => self.expr(other.to_expression()),
        }
    }

    fn private_name(&self, name: &str, span: Span) -> Node {
        let id = self.ident(name, Span::new(span.start + 1, span.end));
        self.node(Kind::PrivateName { id: b(id) }, span)
    }

    fn class(&self, c: &oxc_ast::ast::Class<'_>) -> R<Class> {
        if !c.decorators.is_empty() {
            return Err("decorators".into());
        }
        let mut body = Vec::new();
        for el in &c.body.body {
            body.push(self.class_element(el)?);
        }
        Ok(Class {
            id: c.id.as_ref().map(|id| b(self.ident(&id.name, id.span))),
            super_class: c
                .heritage
                .as_ref()
                .map(|h| self.expr(&h.expression).map(b))
                .transpose()?,
            body: b(self.node(Kind::ClassBody { body }, c.body.span)),
        })
    }

    fn class_element(&self, el: &ClassElement<'_>) -> R<Node> {
        match el {
            ClassElement::StaticBlock(sb) => Ok(self.node(
                Kind::StaticBlock {
                    body: self.statements(&sb.body)?,
                },
                sb.span,
            )),
            ClassElement::MethodDefinition(m) => {
                if !m.decorators.is_empty() {
                    return Err("decorators".into());
                }
                let private = matches!(m.key, PropertyKey::PrivateIdentifier(_));
                let method = Method {
                    kind: match m.kind {
                        MethodDefinitionKind::Constructor => "constructor",
                        MethodDefinitionKind::Method => "method",
                        MethodDefinitionKind::Get => "get",
                        MethodDefinitionKind::Set => "set",
                    },
                    key: b(self.key(&m.key)?),
                    computed: m.computed,
                    is_static: m.r#static,
                    func: self.func(&m.value)?,
                };
                Ok(self.node(
                    if private {
                        Kind::ClassPrivateMethod(method)
                    } else {
                        Kind::ClassMethod(method)
                    },
                    m.span,
                ))
            }
            ClassElement::PropertyDefinition(p) => {
                if !p.decorators.is_empty() {
                    return Err("decorators".into());
                }
                let private = matches!(p.key, PropertyKey::PrivateIdentifier(_));
                let field = Field {
                    key: b(self.key(&p.key)?),
                    value: self.opt_expr(p.value.as_ref())?,
                    computed: p.computed,
                    is_static: p.r#static,
                };
                Ok(self.node(
                    if private {
                        Kind::ClassPrivateProperty(field)
                    } else {
                        Kind::ClassProperty(field)
                    },
                    p.span,
                ))
            }
            ClassElement::AccessorProperty(p) => {
                if !p.decorators.is_empty() {
                    return Err("decorators".into());
                }
                let field = Field {
                    key: b(self.key(&p.key)?),
                    value: self.opt_expr(p.value.as_ref())?,
                    computed: p.computed,
                    is_static: p.r#static,
                };
                Ok(self.node(Kind::ClassAccessorProperty(field), p.span))
            }
            ClassElement::TSIndexSignature(_) => Err("TypeScript index signature".into()),
        }
    }

    // -- patterns ----------------------------------------------------------------

    fn binding(&self, p: &BindingPattern<'_>) -> R<Node> {
        match p {
            BindingPattern::BindingIdentifier(id) => Ok(self.ident(&id.name, id.span)),
            BindingPattern::AssignmentPattern(ap) => Ok(self.node(
                Kind::AssignmentPattern {
                    left: b(self.binding(&ap.left)?),
                    right: b(self.expr(&ap.right)?),
                },
                ap.span,
            )),
            BindingPattern::ObjectPattern(op) => {
                let mut properties = Vec::new();
                for prop in &op.properties {
                    properties.push(self.node(
                        Kind::ObjectProperty {
                            key: b(self.key(&prop.key)?),
                            value: b(self.binding(&prop.value)?),
                            computed: prop.computed,
                            shorthand: prop.shorthand,
                        },
                        prop.span,
                    ));
                }
                if let Some(rest) = &op.rest {
                    properties.push(self.node(
                        Kind::RestElement {
                            argument: b(self.binding(&rest.argument)?),
                        },
                        rest.span,
                    ));
                }
                Ok(self.node(Kind::ObjectPattern { properties }, op.span))
            }
            BindingPattern::ArrayPattern(ap) => {
                let mut elements = Vec::new();
                for el in &ap.elements {
                    elements.push(el.as_ref().map(|e| self.binding(e)).transpose()?);
                }
                if let Some(rest) = &ap.rest {
                    elements.push(Some(self.node(
                        Kind::RestElement {
                            argument: b(self.binding(&rest.argument)?),
                        },
                        rest.span,
                    )));
                }
                Ok(self.node(Kind::ArrayPattern { elements }, ap.span))
            }
        }
    }

    fn target(&self, t: &AssignmentTarget<'_>) -> R<Node> {
        match t {
            AssignmentTarget::ArrayAssignmentTarget(at) => {
                let mut elements = Vec::new();
                for el in &at.elements {
                    elements.push(
                        el.as_ref()
                            .map(|e| self.target_maybe_default(e))
                            .transpose()?,
                    );
                }
                if let Some(rest) = &at.rest {
                    elements.push(Some(self.node(
                        Kind::RestElement {
                            argument: b(self.target(&rest.target)?),
                        },
                        rest.span,
                    )));
                }
                Ok(self.node(Kind::ArrayPattern { elements }, at.span))
            }
            AssignmentTarget::ObjectAssignmentTarget(ot) => {
                let mut properties = Vec::new();
                for prop in &ot.properties {
                    properties.push(match prop {
                        AssignmentTargetProperty::AssignmentTargetPropertyIdentifier(p) => {
                            let key = self.ident(&p.binding.name, p.binding.span);
                            let value = match &p.init {
                                None => self.ident(&p.binding.name, p.binding.span),
                                Some(init) => self.node(
                                    Kind::AssignmentPattern {
                                        left: b(self.ident(&p.binding.name, p.binding.span)),
                                        right: b(self.expr(init)?),
                                    },
                                    p.span,
                                ),
                            };
                            self.node(
                                Kind::ObjectProperty {
                                    key: b(key),
                                    value: b(value),
                                    computed: false,
                                    shorthand: true,
                                },
                                p.span,
                            )
                        }
                        AssignmentTargetProperty::AssignmentTargetPropertyProperty(p) => self.node(
                            Kind::ObjectProperty {
                                key: b(self.key(&p.name)?),
                                value: b(self.target_maybe_default(&p.binding)?),
                                computed: p.computed,
                                shorthand: false,
                            },
                            p.span,
                        ),
                    });
                }
                if let Some(rest) = &ot.rest {
                    properties.push(self.node(
                        Kind::RestElement {
                            argument: b(self.target(&rest.target)?),
                        },
                        rest.span,
                    ));
                }
                Ok(self.node(Kind::ObjectPattern { properties }, ot.span))
            }
            other => self.simple_target(other.to_simple_assignment_target()),
        }
    }

    fn target_maybe_default(&self, t: &AssignmentTargetMaybeDefault<'_>) -> R<Node> {
        match t {
            AssignmentTargetMaybeDefault::AssignmentTargetWithDefault(d) => Ok(self.node(
                Kind::AssignmentPattern {
                    left: b(self.target(&d.binding)?),
                    right: b(self.expr(&d.init)?),
                },
                d.span,
            )),
            other => self.target(other.to_assignment_target()),
        }
    }

    fn simple_target(&self, t: &SimpleAssignmentTarget<'_>) -> R<Node> {
        match t {
            SimpleAssignmentTarget::AssignmentTargetIdentifier(id) => {
                Ok(self.ident(&id.name, id.span))
            }
            other => match other.as_member_expression() {
                Some(m) => self.member(m),
                None => Err("TypeScript assignment target".into()),
            },
        }
    }

    // -- expressions -------------------------------------------------------------

    fn exprs(&self, es: &[Expression<'_>]) -> R<Vec<Node>> {
        es.iter().map(|e| self.expr(e)).collect()
    }

    fn arguments(&self, args: &[Argument<'_>]) -> R<Vec<Node>> {
        args.iter()
            .map(|a| match a {
                Argument::SpreadElement(s) => Ok(self.node(
                    Kind::SpreadElement {
                        argument: b(self.expr(&s.argument)?),
                    },
                    s.span,
                )),
                other => self.expr(other.to_expression()),
            })
            .collect()
    }

    /// Whether an object/callee spine reaches an optional link (Babel types
    /// every link of an optional chain `Optional*`), stopping at anything
    /// that is not a member or call — parentheses included.
    fn spine_optional(e: &Expression<'_>) -> bool {
        match e {
            Expression::CallExpression(c) => c.optional || Self::spine_optional(&c.callee),
            Expression::StaticMemberExpression(m) => m.optional || Self::spine_optional(&m.object),
            Expression::ComputedMemberExpression(m) => {
                m.optional || Self::spine_optional(&m.object)
            }
            Expression::PrivateFieldExpression(m) => m.optional || Self::spine_optional(&m.object),
            _ => false,
        }
    }

    fn member(&self, m: &MemberExpression<'_>) -> R<Node> {
        let (object, property, computed, optional) = match m {
            MemberExpression::StaticMemberExpression(s) => (
                &s.object,
                self.ident(&s.property.name, s.property.span),
                false,
                s.optional,
            ),
            MemberExpression::ComputedMemberExpression(c) => {
                (&c.object, self.expr(&c.expression)?, true, c.optional)
            }
            MemberExpression::PrivateFieldExpression(p) => (
                &p.object,
                self.private_name(&p.field.name, p.field.span),
                false,
                p.optional,
            ),
        };
        let data = Member {
            object: b(self.expr(object)?),
            property: b(property),
            computed,
            optional,
        };
        let kind = if optional || Self::spine_optional(object) {
            Kind::OptionalMemberExpression(data)
        } else {
            Kind::MemberExpression(data)
        };
        Ok(self.node(kind, m.span()))
    }

    fn call(&self, c: &oxc_ast::ast::CallExpression<'_>) -> R<Node> {
        let data = Call {
            callee: b(self.expr(&c.callee)?),
            arguments: self.arguments(&c.arguments)?,
            optional: c.optional,
        };
        let kind = if c.optional || Self::spine_optional(&c.callee) {
            Kind::OptionalCallExpression(data)
        } else {
            Kind::CallExpression(data)
        };
        Ok(self.node(kind, c.span))
    }

    fn template(&self, t: &oxc_ast::ast::TemplateLiteral<'_>) -> R<Node> {
        let quasis = t
            .quasis
            .iter()
            .map(|q| {
                q.value
                    .raw
                    .as_str()
                    .replace("\r\n", "\n")
                    .replace('\r', "\n")
            })
            .collect();
        Ok(self.node(
            Kind::TemplateLiteral {
                quasis,
                expressions: self.exprs(&t.expressions)?,
            },
            t.span,
        ))
    }

    fn arrow(&self, a: &oxc_ast::ast::ArrowFunctionExpression<'_>) -> R<Node> {
        let body = match &a.body {
            ArrowFunctionBody::FunctionBody(fb) => self.function_body(fb)?,
            other => self.expr(other.to_expression())?,
        };
        Ok(self.node(
            Kind::ArrowFunctionExpression(Func {
                id: None,
                params: self.params(&a.params)?,
                body: b(body),
                is_async: a.r#async,
                generator: false,
            }),
            a.span,
        ))
    }

    fn object(&self, o: &oxc_ast::ast::ObjectExpression<'_>) -> R<Node> {
        let mut properties = Vec::new();
        for p in &o.properties {
            properties.push(match p {
                ObjectPropertyKind::SpreadProperty(s) => self.node(
                    Kind::SpreadElement {
                        argument: b(self.expr(&s.argument)?),
                    },
                    s.span,
                ),
                ObjectPropertyKind::ObjectProperty(p) => {
                    let method_kind = match p.kind {
                        PropertyKind::Get => Some("get"),
                        PropertyKind::Set => Some("set"),
                        PropertyKind::Init if p.method => Some("method"),
                        PropertyKind::Init => None,
                    };
                    match method_kind {
                        Some(kind) => {
                            let Expression::FunctionExpression(f) = &p.value else {
                                return Err("a method without a function value".into());
                            };
                            self.node(
                                Kind::ObjectMethod(Method {
                                    kind,
                                    key: b(self.key(&p.key)?),
                                    computed: p.computed,
                                    is_static: false,
                                    func: self.func(f)?,
                                }),
                                p.span,
                            )
                        }
                        None => self.node(
                            Kind::ObjectProperty {
                                key: b(self.key(&p.key)?),
                                value: b(self.expr(&p.value)?),
                                computed: p.computed,
                                shorthand: p.shorthand,
                            },
                            p.span,
                        ),
                    }
                }
            });
        }
        Ok(self.node(Kind::ObjectExpression { properties }, o.span))
    }

    fn binary(&self, operator: &'static str, left: Node, right: &Expression<'_>) -> R<Binary> {
        Ok(Binary {
            operator,
            left: b(left),
            right: b(self.expr(right)?),
        })
    }

    fn raw_or_slice(&self, raw: Option<impl ToString>, span: Span) -> String {
        raw.map(|r| r.to_string())
            .unwrap_or_else(|| self.slice(span))
    }

    pub fn expr(&self, e: &Expression<'_>) -> R<Node> {
        let span = e.span();
        match e {
            Expression::ParenthesizedExpression(p) => {
                let mut inner = self.expr(&p.expression)?;
                inner.parenthesized = true;
                Ok(inner)
            }
            Expression::TemplateLiteral(t) => self.template(t),
            Expression::ArrowFunctionExpression(a) => self.arrow(a),
            Expression::CallExpression(c) => self.call(c),
            Expression::ChainExpression(c) => match &c.expression {
                ChainElement::CallExpression(call) => self.call(call),
                ChainElement::TSNonNullExpression(_) => Err("TypeScript non-null".into()),
                other => self.member(other.to_member_expression()),
            },
            Expression::ObjectExpression(o) => self.object(o),
            _ => {
                if let Some(m) = e.as_member_expression() {
                    return self.member(m);
                }
                let kind = match self.literal_kind(e) {
                    Some(k) => k,
                    None => match self.operator_kind(e)? {
                        Some(k) => k,
                        None => self.other_kind(e)?,
                    },
                };
                Ok(self.node(kind, span))
            }
        }
    }

    fn literal_kind(&self, e: &Expression<'_>) -> Option<Kind> {
        Some(match e {
            Expression::BooleanLiteral(l) => Kind::BooleanLiteral { value: l.value },
            Expression::NullLiteral(_) => Kind::NullLiteral,
            Expression::NumericLiteral(l) => Kind::NumericLiteral {
                value: l.value,
                raw: Some(self.raw_or_slice(l.raw, l.span)),
            },
            Expression::BigIntLiteral(l) => Kind::BigIntLiteral {
                raw: self.raw_or_slice(l.raw, l.span),
            },
            Expression::RegExpLiteral(l) => Kind::RegExpLiteral {
                text: self.slice(l.span),
            },
            Expression::StringLiteral(l) => Kind::StringLiteral {
                value: l.value.to_string(),
                raw: Some(self.raw_or_slice(l.raw, l.span)),
            },
            Expression::Identifier(id) => Kind::Identifier {
                name: id.name.to_string(),
            },
            Expression::Super(_) => Kind::Super,
            Expression::ThisExpression(_) => Kind::ThisExpression,
            _ => return None,
        })
    }

    fn operator_kind(&self, e: &Expression<'_>) -> R<Option<Kind>> {
        Ok(Some(match e {
            Expression::AssignmentExpression(a) => Kind::AssignmentExpression(Binary {
                operator: a.operator.as_str(),
                left: b(self.target(&a.left)?),
                right: b(self.expr(&a.right)?),
            }),
            Expression::AwaitExpression(a) => Kind::AwaitExpression {
                argument: b(self.expr(&a.argument)?),
            },
            Expression::BinaryExpression(x) => Kind::BinaryExpression(self.binary(
                x.operator.as_str(),
                self.expr(&x.left)?,
                &x.right,
            )?),
            Expression::PrivateInExpression(x) => Kind::BinaryExpression(self.binary(
                "in",
                self.private_name(&x.left.name, x.left.span),
                &x.right,
            )?),
            Expression::LogicalExpression(x) => Kind::LogicalExpression(self.binary(
                x.operator.as_str(),
                self.expr(&x.left)?,
                &x.right,
            )?),
            Expression::ConditionalExpression(c) => Kind::ConditionalExpression {
                test: b(self.expr(&c.test)?),
                consequent: b(self.expr(&c.consequent)?),
                alternate: b(self.expr(&c.alternate)?),
            },
            Expression::SequenceExpression(s) => Kind::SequenceExpression {
                expressions: self.exprs(&s.expressions)?,
            },
            Expression::UnaryExpression(u) => Kind::UnaryExpression {
                operator: u.operator.as_str(),
                argument: b(self.expr(&u.argument)?),
            },
            Expression::UpdateExpression(u) => Kind::UpdateExpression {
                operator: u.operator.as_str(),
                prefix: u.prefix,
                argument: b(self.simple_target(&u.argument)?),
            },
            Expression::YieldExpression(y) => Kind::YieldExpression {
                argument: self.opt_expr(y.argument.as_ref())?,
                delegate: y.delegate,
            },
            _ => return Ok(None),
        }))
    }

    fn array(&self, a: &oxc_ast::ast::ArrayExpression<'_>) -> R<Kind> {
        let mut elements = Vec::with_capacity(a.elements.len());
        for el in &a.elements {
            elements.push(match el {
                ArrayExpressionElement::Elision(_) => None,
                ArrayExpressionElement::SpreadElement(s) => Some(self.node(
                    Kind::SpreadElement {
                        argument: b(self.expr(&s.argument)?),
                    },
                    s.span,
                )),
                other => Some(self.expr(other.to_expression())?),
            });
        }
        Ok(Kind::ArrayExpression { elements })
    }

    fn import_call(&self, i: &oxc_ast::ast::ImportExpression<'_>) -> R<Kind> {
        let callee = self.node(Kind::Import, Span::new(i.span.start, i.span.start + 6));
        let mut arguments = vec![self.expr(&i.source)?];
        if let Some(opts) = &i.options {
            arguments.push(self.expr(opts)?);
        }
        Ok(Kind::CallExpression(Call {
            callee: b(callee),
            arguments,
            optional: false,
        }))
    }

    fn meta(&self, meta: &str, property: &str, span: Span) -> Kind {
        let m = meta.len() as u32;
        let p = property.len() as u32;
        Kind::MetaProperty {
            meta: b(self.ident(meta, Span::new(span.start, span.start + m))),
            property: b(self.ident(property, Span::new(span.end - p, span.end))),
        }
    }

    fn other_kind(&self, e: &Expression<'_>) -> R<Kind> {
        Ok(match e {
            Expression::ArrayExpression(a) => self.array(a)?,
            Expression::ClassExpression(c) => Kind::ClassExpression(self.class(c)?),
            Expression::FunctionExpression(f) => Kind::FunctionExpression(self.func(f)?),
            Expression::ImportExpression(i) => self.import_call(i)?,
            Expression::NewExpression(n) => Kind::NewExpression(Call {
                callee: b(self.expr(&n.callee)?),
                arguments: self.arguments(&n.arguments)?,
                optional: false,
            }),
            Expression::TaggedTemplateExpression(t) => Kind::TaggedTemplateExpression {
                tag: b(self.expr(&t.tag)?),
                quasi: b(self.template(&t.quasi)?),
            },
            Expression::ImportMeta(m) => self.meta("import", "meta", m.span),
            Expression::NewTarget(m) => self.meta("new", "target", m.span),
            other => {
                let span = other.span();
                return Err(format!(
                    "unsupported expression for the desugar at {}..{}",
                    span.start, span.end
                ));
            }
        })
    }
}
