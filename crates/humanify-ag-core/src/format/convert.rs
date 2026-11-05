//! oxc's AST → the Babel-shaped arena ([`super::ast::Tree`]) the generator
//! prints and the transforms mutate.
//!
//! The translations are the ones the port has already pinned (lesson 2,
//! 14): parentheses become `extra.parenthesized`; an optional chain's links
//! are `Optional*` iff their own `optional` or their object/callee spine
//! reaches an optional link (never through parentheses); object/class
//! methods are single method nodes; `import(…)` is a call on `Import`;
//! `#x in o` is a BinaryExpression over a PrivateName. Module declarations
//! are Babel's: oxc's three export kinds (`export const …`, `export {…}`,
//! `export {…} from`) are ONE ExportNamedDeclaration, and `export * as ns
//! from` is an ExportNamedDeclaration over an ExportNamespaceSpecifier
//! (lesson 23). Lines are Babel's: `\r\n`, `\r`, `\n`, U+2028 and U+2029
//! each end a line.
//!
//! Anything a Babel-parsed JavaScript file cannot contain (TypeScript,
//! JSX, decorators) is an `Err`, never a guess.

use oxc_ast::ast::{
    Argument, ArrayExpressionElement, ArrowFunctionBody, AssignmentTarget,
    AssignmentTargetMaybeDefault, AssignmentTargetProperty, BindingPattern, ChainElement,
    ClassElement, Declaration, ExportDefaultDeclarationKind, Expression, ForStatementInit,
    ForStatementLeft, FormalParameters, Function, FunctionBody, ImportAttributeKey,
    ImportDeclarationSpecifier, ImportPhase, MemberExpression, MethodDefinitionKind,
    ModuleExportName, ObjectPropertyKind, Program, PropertyKey, PropertyKind,
    SimpleAssignmentTarget, Statement, VariableDeclaration, VariableDeclarationKind, WithClause,
    WithClauseKeyword,
};
use oxc_span::{GetSpan, Span};

use crate::babel_view::BabelLines;

use super::ast::{
    Binary, Call, Class, Func, Kind, Loc, Member, Method, Node, NodeId, Prop, Quasi, Tree,
};

type R<T> = Result<T, String>;

pub struct Converter<'t> {
    text: &'t str,
    lines: BabelLines<'t>,
    pub tree: Tree,
}

impl<'t> Converter<'t> {
    pub fn new(text: &'t str) -> Converter<'t> {
        Converter::with_tree(text, Tree::new())
    }

    /// Convert into an existing tree (a Babel helper grafted into a file).
    pub fn with_tree(text: &'t str, tree: Tree) -> Converter<'t> {
        Converter {
            text,
            lines: BabelLines::new(text),
            tree,
        }
    }

    fn loc(&self, span: Span) -> Option<Loc> {
        Some(Loc {
            start: self.lines.line(span.start) as u32,
            end: self.lines.line(span.end) as u32,
        })
    }

    fn node(&mut self, kind: Kind, span: Span) -> NodeId {
        let mut node = Node::new(kind, self.loc(span));
        node.span = Some((span.start, span.end));
        self.tree.alloc(node)
    }

    fn slice(&self, span: Span) -> String {
        self.text[span.start as usize..span.end as usize].to_string()
    }

    fn ident(&mut self, name: &str, span: Span) -> NodeId {
        self.node(
            Kind::Identifier {
                name: name.to_string(),
            },
            span,
        )
    }

    // -- program + statements ------------------------------------------------

    /// The Program node (the tree's root is added by [`Converter::file`]).
    pub fn program(&mut self, program: &Program<'_>) -> R<NodeId> {
        let directives = program
            .directives
            .iter()
            .map(|d| self.directive(d))
            .collect();
        let body = self.statements(&program.body)?;
        let mut node = Node::new(
            Kind::Program {
                interpreter: program.hashbang.as_ref().map(|h| h.value.to_string()),
                directives,
                body,
            },
            None,
        );
        node.span = Some((program.span.start, program.span.end));
        Ok(self.tree.alloc(node))
    }

    /// `File { program }`: the traversal root.
    pub fn file(&mut self, program: &Program<'_>) -> R<NodeId> {
        let program = self.program(program)?;
        Ok(self.tree.synth(Kind::File { program }))
    }

    fn directive(&mut self, d: &oxc_ast::ast::Directive<'_>) -> NodeId {
        let raw = self.slice(d.expression.span);
        let lit = self.node(Kind::DirectiveLiteral { raw }, d.expression.span);
        self.node(Kind::Directive { value: lit }, d.span)
    }

    fn statements(&mut self, stmts: &[Statement<'_>]) -> R<Vec<NodeId>> {
        stmts.iter().map(|s| self.statement(s)).collect()
    }

    fn function_body(&mut self, body: &FunctionBody<'_>) -> R<NodeId> {
        let directives = body.directives.iter().map(|d| self.directive(d)).collect();
        let body_stmts = self.statements(&body.statements)?;
        Ok(self.node(
            Kind::BlockStatement {
                directives,
                body: body_stmts,
            },
            body.span,
        ))
    }

    fn block(&mut self, block: &oxc_ast::ast::BlockStatement<'_>) -> R<NodeId> {
        let body = self.statements(&block.body)?;
        Ok(self.node(
            Kind::BlockStatement {
                directives: Vec::new(),
                body,
            },
            block.span,
        ))
    }

    fn opt_expr(&mut self, e: Option<&Expression<'_>>) -> R<Option<NodeId>> {
        e.map(|e| self.expr(e)).transpose()
    }

    fn label(&mut self, l: Option<&oxc_ast::ast::LabelIdentifier<'_>>) -> Option<NodeId> {
        l.map(|l| self.ident(&l.name, l.span))
    }

    fn statement(&mut self, s: &Statement<'_>) -> R<NodeId> {
        let span = s.span();
        match s {
            Statement::BlockStatement(bl) => self.block(bl),
            Statement::VariableDeclaration(d) => self.var_decl(d),
            Statement::FunctionDeclaration(f) => {
                let func = self.func(f)?;
                Ok(self.node(Kind::FunctionDeclaration(func), f.span))
            }
            Statement::ClassDeclaration(c) => {
                let class = self.class(c)?;
                Ok(self.node(Kind::ClassDeclaration(class), c.span))
            }
            Statement::DoWhileStatement(_)
            | Statement::ForInStatement(_)
            | Statement::ForOfStatement(_)
            | Statement::ForStatement(_)
            | Statement::WhileStatement(_) => {
                let kind = self.loop_statement(s)?;
                Ok(self.node(kind, span))
            }
            Statement::SwitchStatement(st) => {
                let kind = self.switch_statement(st)?;
                Ok(self.node(kind, span))
            }
            Statement::TryStatement(st) => {
                let kind = self.try_statement(st)?;
                Ok(self.node(kind, span))
            }
            Statement::ImportDeclaration(_)
            | Statement::ExportAllDeclaration(_)
            | Statement::ExportDefaultDeclaration(_)
            | Statement::ExportDeclaration(_)
            | Statement::ExportNamedDeclaration(_)
            | Statement::ExportFromDeclaration(_) => {
                let kind = self.module_declaration(s)?;
                Ok(self.node(kind, span))
            }
            _ => {
                let kind = self.simple_statement(s)?;
                Ok(self.node(kind, span))
            }
        }
    }

    fn simple_statement(&mut self, s: &Statement<'_>) -> R<Kind> {
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
                expression: self.expr(&st.expression)?,
            },
            Statement::IfStatement(st) => Kind::IfStatement {
                test: self.expr(&st.test)?,
                consequent: self.statement(&st.consequent)?,
                alternate: self.opt_statement(st.alternate.as_ref())?,
            },
            Statement::LabeledStatement(st) => Kind::LabeledStatement {
                label: self.ident(&st.label.name, st.label.span),
                body: self.statement(&st.body)?,
            },
            Statement::ReturnStatement(st) => Kind::ReturnStatement {
                argument: self.opt_expr(st.argument.as_ref())?,
            },
            Statement::ThrowStatement(st) => Kind::ThrowStatement {
                argument: self.expr(&st.argument)?,
            },
            Statement::WithStatement(st) => Kind::WithStatement {
                object: self.expr(&st.object)?,
                body: self.statement(&st.body)?,
            },
            other => {
                return Err(format!(
                    "unsupported statement at {}..{}",
                    other.span().start,
                    other.span().end
                ));
            }
        })
    }

    fn opt_statement(&mut self, s: Option<&Statement<'_>>) -> R<Option<NodeId>> {
        s.map(|s| self.statement(s)).transpose()
    }

    fn loop_statement(&mut self, s: &Statement<'_>) -> R<Kind> {
        Ok(match s {
            Statement::DoWhileStatement(st) => Kind::DoWhileStatement {
                body: self.statement(&st.body)?,
                test: self.expr(&st.test)?,
            },
            Statement::WhileStatement(st) => Kind::WhileStatement {
                test: self.expr(&st.test)?,
                body: self.statement(&st.body)?,
            },
            Statement::ForInStatement(st) => Kind::ForInStatement {
                left: self.for_left(&st.left)?,
                right: self.expr(&st.right)?,
                body: self.statement(&st.body)?,
            },
            Statement::ForOfStatement(st) => Kind::ForOfStatement {
                is_await: st.r#await,
                left: self.for_left(&st.left)?,
                right: self.expr(&st.right)?,
                body: self.statement(&st.body)?,
            },
            Statement::ForStatement(st) => Kind::ForStatement {
                init: self.for_init(st.init.as_ref())?,
                test: self.opt_expr(st.test.as_ref())?,
                update: self.opt_expr(st.update.as_ref())?,
                body: self.statement(&st.body)?,
            },
            _ => unreachable!("dispatched on the loop kinds"),
        })
    }

    fn for_init(&mut self, init: Option<&ForStatementInit<'_>>) -> R<Option<NodeId>> {
        Ok(match init {
            None => None,
            Some(ForStatementInit::VariableDeclaration(d)) => Some(self.var_decl(d)?),
            Some(other) => Some(self.expr(other.to_expression())?),
        })
    }

    fn switch_statement(&mut self, st: &oxc_ast::ast::SwitchStatement<'_>) -> R<Kind> {
        let discriminant = self.expr(&st.discriminant)?;
        let mut cases = Vec::with_capacity(st.cases.len());
        for c in &st.cases {
            let kind = Kind::SwitchCase {
                test: self.opt_expr(c.test.as_ref())?,
                consequent: self.statements(&c.consequent)?,
            };
            cases.push(self.node(kind, c.span));
        }
        Ok(Kind::SwitchStatement {
            discriminant,
            cases,
        })
    }

    fn catch_clause(&mut self, h: &oxc_ast::ast::CatchClause<'_>) -> R<NodeId> {
        let param = match &h.param {
            Some(p) => Some(self.binding(&p.pattern)?),
            None => None,
        };
        let body = self.block(&h.body)?;
        Ok(self.node(Kind::CatchClause { param, body }, h.span))
    }

    fn try_statement(&mut self, st: &oxc_ast::ast::TryStatement<'_>) -> R<Kind> {
        let block = self.block(&st.block)?;
        let handler = match &st.handler {
            Some(h) => Some(self.catch_clause(h)?),
            None => None,
        };
        let finalizer = match &st.finalizer {
            Some(f) => Some(self.block(f)?),
            None => None,
        };
        Ok(Kind::TryStatement {
            block,
            handler,
            finalizer,
        })
    }

    fn var_decl(&mut self, d: &VariableDeclaration<'_>) -> R<NodeId> {
        let kind = match d.kind {
            VariableDeclarationKind::Var => "var",
            VariableDeclarationKind::Let => "let",
            VariableDeclarationKind::Const => "const",
            VariableDeclarationKind::Using => "using",
            VariableDeclarationKind::AwaitUsing => "await using",
        };
        let mut declarations = Vec::with_capacity(d.declarations.len());
        for v in &d.declarations {
            let k = Kind::VariableDeclarator {
                id: self.binding(&v.id)?,
                init: self.opt_expr(v.init.as_ref())?,
            };
            declarations.push(self.node(k, v.span));
        }
        Ok(self.node(Kind::VariableDeclaration { kind, declarations }, d.span))
    }

    fn for_left(&mut self, left: &ForStatementLeft<'_>) -> R<NodeId> {
        match left {
            ForStatementLeft::VariableDeclaration(d) => self.var_decl(d),
            other => self.target(other.to_assignment_target()),
        }
    }

    // -- modules -----------------------------------------------------------------

    fn module_export_name(&mut self, n: &ModuleExportName<'_>) -> NodeId {
        match n {
            ModuleExportName::IdentifierName(id) => self.ident(&id.name, id.span),
            ModuleExportName::IdentifierReference(id) => self.ident(&id.name, id.span),
            ModuleExportName::StringLiteral(s) => self.string_literal(s),
        }
    }

    fn string_literal(&mut self, s: &oxc_ast::ast::StringLiteral<'_>) -> NodeId {
        let raw = self.raw_or_slice(s.raw, s.span);
        self.node(
            Kind::StringLiteral {
                value: s.value.to_string(),
                raw: Some(raw),
            },
            s.span,
        )
    }

    fn attributes(&mut self, w: Option<&WithClause<'_>>) -> R<Vec<NodeId>> {
        let Some(w) = w else {
            return Ok(Vec::new());
        };
        if w.keyword == WithClauseKeyword::Assert {
            return Err("import assertions (`assert {…}`) are not ported".into());
        }
        let mut out = Vec::with_capacity(w.with_entries.len());
        for a in &w.with_entries {
            let key = match &a.key {
                ImportAttributeKey::Identifier(id) => self.ident(&id.name, id.span),
                ImportAttributeKey::StringLiteral(s) => self.string_literal(s),
            };
            let value = self.string_literal(&a.value);
            out.push(self.node(Kind::ImportAttribute { key, value }, a.span));
        }
        Ok(out)
    }

    fn export_specifiers(&mut self, specs: &[oxc_ast::ast::ExportSpecifier<'_>]) -> Vec<NodeId> {
        let mut out = Vec::with_capacity(specs.len());
        for s in specs {
            let local = self.module_export_name(&s.local);
            let exported = self.module_export_name(&s.exported);
            out.push(self.node(Kind::ExportSpecifier { local, exported }, s.span));
        }
        out
    }

    fn declaration(&mut self, d: &Declaration<'_>) -> R<NodeId> {
        match d {
            Declaration::VariableDeclaration(v) => self.var_decl(v),
            Declaration::FunctionDeclaration(f) => {
                let func = self.func(f)?;
                Ok(self.node(Kind::FunctionDeclaration(func), f.span))
            }
            Declaration::ClassDeclaration(c) => {
                let class = self.class(c)?;
                Ok(self.node(Kind::ClassDeclaration(class), c.span))
            }
            _ => Err("TypeScript declaration".into()),
        }
    }

    fn import_declaration(&mut self, d: &oxc_ast::ast::ImportDeclaration<'_>) -> R<Kind> {
        let mut specifiers = Vec::new();
        for s in d.specifiers.iter().flatten() {
            specifiers.push(match s {
                ImportDeclarationSpecifier::ImportSpecifier(s) => {
                    let imported = self.module_export_name(&s.imported);
                    let local = self.ident(&s.local.name, s.local.span);
                    self.node(Kind::ImportSpecifier { imported, local }, s.span)
                }
                ImportDeclarationSpecifier::ImportDefaultSpecifier(s) => {
                    let local = self.ident(&s.local.name, s.local.span);
                    self.node(Kind::ImportDefaultSpecifier { local }, s.span)
                }
                ImportDeclarationSpecifier::ImportNamespaceSpecifier(s) => {
                    let local = self.ident(&s.local.name, s.local.span);
                    self.node(Kind::ImportNamespaceSpecifier { local }, s.span)
                }
            });
        }
        let source = self.string_literal(&d.source);
        let attributes = self.attributes(d.with_clause.as_deref())?;
        Ok(Kind::ImportDeclaration {
            specifiers,
            source,
            attributes,
            phase: d.phase.map(|p| match p {
                ImportPhase::Source => "source",
                ImportPhase::Defer => "defer",
            }),
        })
    }

    fn module_declaration(&mut self, s: &Statement<'_>) -> R<Kind> {
        Ok(match s {
            Statement::ImportDeclaration(d) => self.import_declaration(d)?,
            Statement::ExportDeclaration(d) => Kind::ExportNamedDeclaration {
                declaration: Some(self.declaration(&d.declaration)?),
                specifiers: Vec::new(),
                source: None,
                attributes: Vec::new(),
            },
            Statement::ExportNamedDeclaration(d) => Kind::ExportNamedDeclaration {
                declaration: None,
                specifiers: self.export_specifiers(&d.specifiers),
                source: None,
                attributes: Vec::new(),
            },
            Statement::ExportFromDeclaration(d) => {
                let specifiers = self.export_specifiers(&d.specifiers);
                let source = self.string_literal(&d.source);
                Kind::ExportNamedDeclaration {
                    declaration: None,
                    specifiers,
                    source: Some(source),
                    attributes: self.attributes(d.with_clause.as_deref())?,
                }
            }
            Statement::ExportAllDeclaration(d) => match &d.exported {
                None => {
                    let source = self.string_literal(&d.source);
                    Kind::ExportAllDeclaration {
                        source,
                        attributes: self.attributes(d.with_clause.as_deref())?,
                    }
                }
                Some(name) => {
                    // `export * as ns from "m"`: Babel's namespace specifier.
                    let exported = self.module_export_name(name);
                    let (start, end) = (d.span.start + 7, name.span().end);
                    let spec = self.node(
                        Kind::ExportNamespaceSpecifier { exported },
                        Span::new(start, end),
                    );
                    let source = self.string_literal(&d.source);
                    Kind::ExportNamedDeclaration {
                        declaration: None,
                        specifiers: vec![spec],
                        source: Some(source),
                        attributes: self.attributes(d.with_clause.as_deref())?,
                    }
                }
            },
            Statement::ExportDefaultDeclaration(d) => Kind::ExportDefaultDeclaration {
                declaration: match &d.declaration {
                    ExportDefaultDeclarationKind::FunctionDeclaration(f) => {
                        let func = self.func(f)?;
                        self.node(Kind::FunctionDeclaration(func), f.span)
                    }
                    ExportDefaultDeclarationKind::ClassDeclaration(c) => {
                        let class = self.class(c)?;
                        self.node(Kind::ClassDeclaration(class), c.span)
                    }
                    ExportDefaultDeclarationKind::TSInterfaceDeclaration(_) => {
                        return Err("TypeScript interface".into());
                    }
                    other => self.expr(other.to_expression())?,
                },
            },
            _ => unreachable!("dispatched on the module declaration kinds"),
        })
    }

    // -- functions + classes ---------------------------------------------------

    fn params(&mut self, params: &FormalParameters<'_>) -> R<Vec<NodeId>> {
        let mut out = Vec::new();
        for p in &params.items {
            if !p.decorators.is_empty() {
                return Err("decorators".into());
            }
            let pattern = self.binding(&p.pattern)?;
            out.push(match &p.initializer {
                None => pattern,
                Some(init) => {
                    let right = self.expr(init)?;
                    self.node(
                        Kind::AssignmentPattern {
                            left: pattern,
                            right,
                        },
                        p.span,
                    )
                }
            });
        }
        if let Some(rest) = &params.rest {
            let argument = self.binding(&rest.rest.argument)?;
            out.push(self.node(Kind::RestElement { argument }, rest.rest.span));
        }
        Ok(out)
    }

    fn func(&mut self, f: &Function<'_>) -> R<Func> {
        let body = f.body.as_ref().ok_or("a function without a body")?;
        let id = f.id.as_ref().map(|id| self.ident(&id.name, id.span));
        let params = self.params(&f.params)?;
        let body = self.function_body(body)?;
        Ok(Func {
            id,
            params,
            body,
            is_async: f.r#async,
            generator: f.generator,
        })
    }

    fn key(&mut self, key: &PropertyKey<'_>) -> R<NodeId> {
        match key {
            PropertyKey::StaticIdentifier(id) => Ok(self.ident(&id.name, id.span)),
            PropertyKey::PrivateIdentifier(id) => Ok(self.private_name(&id.name, id.span)),
            other => self.expr(other.to_expression()),
        }
    }

    fn private_name(&mut self, name: &str, span: Span) -> NodeId {
        let id = self.ident(name, Span::new(span.start + 1, span.end));
        self.node(Kind::PrivateName { id }, span)
    }

    fn class(&mut self, c: &oxc_ast::ast::Class<'_>) -> R<Class> {
        if !c.decorators.is_empty() {
            return Err("decorators".into());
        }
        let id = c.id.as_ref().map(|id| self.ident(&id.name, id.span));
        let super_class = c
            .heritage
            .as_ref()
            .map(|h| self.expr(&h.expression))
            .transpose()?;
        let mut body = Vec::new();
        for el in &c.body.body {
            body.push(self.class_element(el)?);
        }
        let body = self.node(Kind::ClassBody { body }, c.body.span);
        Ok(Class {
            id,
            super_class,
            body,
        })
    }

    fn class_element(&mut self, el: &ClassElement<'_>) -> R<NodeId> {
        match el {
            ClassElement::StaticBlock(sb) => {
                let body = self.statements(&sb.body)?;
                Ok(self.node(Kind::StaticBlock { body }, sb.span))
            }
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
                    key: self.key(&m.key)?,
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
                let prop = Prop {
                    key: self.key(&p.key)?,
                    value: self.opt_expr(p.value.as_ref())?,
                    computed: p.computed,
                    is_static: p.r#static,
                };
                Ok(self.node(
                    if private {
                        Kind::ClassPrivateProperty(prop)
                    } else {
                        Kind::ClassProperty(prop)
                    },
                    p.span,
                ))
            }
            // Babel's parser needs the `decoratorAutoAccessors` plugin for
            // `accessor x`; without it the parse throws (and so does the TS
            // stage).
            ClassElement::AccessorProperty(p) => Err(format!(
                "Support for the experimental syntax 'decoratorAutoAccessors' isn't currently enabled at {}",
                p.span.start
            )),
            ClassElement::TSIndexSignature(_) => Err("TypeScript index signature".into()),
        }
    }

    // -- patterns ----------------------------------------------------------------

    fn binding(&mut self, p: &BindingPattern<'_>) -> R<NodeId> {
        match p {
            BindingPattern::BindingIdentifier(id) => Ok(self.ident(&id.name, id.span)),
            BindingPattern::AssignmentPattern(ap) => {
                let left = self.binding(&ap.left)?;
                let right = self.expr(&ap.right)?;
                Ok(self.node(Kind::AssignmentPattern { left, right }, ap.span))
            }
            BindingPattern::ObjectPattern(op) => {
                let mut properties = Vec::new();
                for prop in &op.properties {
                    let kind = Kind::ObjectProperty {
                        key: self.key(&prop.key)?,
                        value: self.binding(&prop.value)?,
                        computed: prop.computed,
                        shorthand: prop.shorthand,
                    };
                    properties.push(self.node(kind, prop.span));
                }
                if let Some(rest) = &op.rest {
                    let argument = self.binding(&rest.argument)?;
                    properties.push(self.node(Kind::RestElement { argument }, rest.span));
                }
                Ok(self.node(Kind::ObjectPattern { properties }, op.span))
            }
            BindingPattern::ArrayPattern(ap) => {
                let mut elements = Vec::new();
                for el in &ap.elements {
                    elements.push(match el {
                        Some(e) => self.binding(e)?,
                        None => NodeId::NONE,
                    });
                }
                if let Some(rest) = &ap.rest {
                    let argument = self.binding(&rest.argument)?;
                    elements.push(self.node(Kind::RestElement { argument }, rest.span));
                }
                Ok(self.node(Kind::ArrayPattern { elements }, ap.span))
            }
        }
    }

    fn target(&mut self, t: &AssignmentTarget<'_>) -> R<NodeId> {
        match t {
            AssignmentTarget::ArrayAssignmentTarget(at) => {
                let mut elements = Vec::new();
                for el in &at.elements {
                    elements.push(match el {
                        Some(e) => self.target_maybe_default(e)?,
                        None => NodeId::NONE,
                    });
                }
                if let Some(rest) = &at.rest {
                    let argument = self.target(&rest.target)?;
                    elements.push(self.node(Kind::RestElement { argument }, rest.span));
                }
                Ok(self.node(Kind::ArrayPattern { elements }, at.span))
            }
            AssignmentTarget::ObjectAssignmentTarget(ot) => {
                let mut properties = Vec::new();
                for prop in &ot.properties {
                    let id = match prop {
                        AssignmentTargetProperty::AssignmentTargetPropertyIdentifier(p) => {
                            let key = self.ident(&p.binding.name, p.binding.span);
                            let value = match &p.init {
                                None => self.ident(&p.binding.name, p.binding.span),
                                Some(init) => {
                                    let left = self.ident(&p.binding.name, p.binding.span);
                                    let right = self.expr(init)?;
                                    self.node(Kind::AssignmentPattern { left, right }, p.span)
                                }
                            };
                            self.node(
                                Kind::ObjectProperty {
                                    key,
                                    value,
                                    computed: false,
                                    shorthand: true,
                                },
                                p.span,
                            )
                        }
                        AssignmentTargetProperty::AssignmentTargetPropertyProperty(p) => {
                            let kind = Kind::ObjectProperty {
                                key: self.key(&p.name)?,
                                value: self.target_maybe_default(&p.binding)?,
                                computed: p.computed,
                                shorthand: false,
                            };
                            self.node(kind, p.span)
                        }
                    };
                    properties.push(id);
                }
                if let Some(rest) = &ot.rest {
                    let argument = self.target(&rest.target)?;
                    properties.push(self.node(Kind::RestElement { argument }, rest.span));
                }
                Ok(self.node(Kind::ObjectPattern { properties }, ot.span))
            }
            other => self.simple_target(other.to_simple_assignment_target()),
        }
    }

    fn target_maybe_default(&mut self, t: &AssignmentTargetMaybeDefault<'_>) -> R<NodeId> {
        match t {
            AssignmentTargetMaybeDefault::AssignmentTargetWithDefault(d) => {
                let left = self.target(&d.binding)?;
                let right = self.expr(&d.init)?;
                Ok(self.node(Kind::AssignmentPattern { left, right }, d.span))
            }
            other => self.target(other.to_assignment_target()),
        }
    }

    fn simple_target(&mut self, t: &SimpleAssignmentTarget<'_>) -> R<NodeId> {
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

    fn exprs(&mut self, es: &[Expression<'_>]) -> R<Vec<NodeId>> {
        es.iter().map(|e| self.expr(e)).collect()
    }

    fn arguments(&mut self, args: &[Argument<'_>]) -> R<Vec<NodeId>> {
        let mut out = Vec::with_capacity(args.len());
        for a in args {
            out.push(match a {
                Argument::SpreadElement(s) => {
                    let argument = self.expr(&s.argument)?;
                    self.node(Kind::SpreadElement { argument }, s.span)
                }
                other => self.expr(other.to_expression())?,
            });
        }
        Ok(out)
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

    fn member(&mut self, m: &MemberExpression<'_>) -> R<NodeId> {
        let (object, optional) = match m {
            MemberExpression::StaticMemberExpression(s) => (&s.object, s.optional),
            MemberExpression::ComputedMemberExpression(c) => (&c.object, c.optional),
            MemberExpression::PrivateFieldExpression(p) => (&p.object, p.optional),
        };
        let object_id = self.expr(object)?;
        let (property, computed) = match m {
            MemberExpression::StaticMemberExpression(s) => {
                (self.ident(&s.property.name, s.property.span), false)
            }
            MemberExpression::ComputedMemberExpression(c) => (self.expr(&c.expression)?, true),
            MemberExpression::PrivateFieldExpression(p) => {
                (self.private_name(&p.field.name, p.field.span), false)
            }
        };
        let data = Member {
            object: object_id,
            property,
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

    fn call(&mut self, c: &oxc_ast::ast::CallExpression<'_>) -> R<NodeId> {
        let data = Call {
            callee: self.expr(&c.callee)?,
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

    fn template(&mut self, t: &oxc_ast::ast::TemplateLiteral<'_>) -> R<NodeId> {
        let quasis = t
            .quasis
            .iter()
            .map(|q| Quasi {
                raw: q
                    .value
                    .raw
                    .as_str()
                    .replace("\r\n", "\n")
                    .replace('\r', "\n"),
                cooked: q.value.cooked.as_ref().map(|c| c.as_str().to_string()),
            })
            .collect();
        let expressions = self.exprs(&t.expressions)?;
        Ok(self.node(
            Kind::TemplateLiteral {
                quasis,
                expressions,
            },
            t.span,
        ))
    }

    fn arrow(&mut self, a: &oxc_ast::ast::ArrowFunctionExpression<'_>) -> R<NodeId> {
        let params = self.params(&a.params)?;
        let body = match &a.body {
            ArrowFunctionBody::FunctionBody(fb) => self.function_body(fb)?,
            other => self.expr(other.to_expression())?,
        };
        Ok(self.node(
            Kind::ArrowFunctionExpression(Func {
                id: None,
                params,
                body,
                is_async: a.r#async,
                generator: false,
            }),
            a.span,
        ))
    }

    fn object(&mut self, o: &oxc_ast::ast::ObjectExpression<'_>) -> R<NodeId> {
        let mut properties = Vec::new();
        for p in &o.properties {
            properties.push(match p {
                ObjectPropertyKind::SpreadProperty(s) => {
                    let argument = self.expr(&s.argument)?;
                    self.node(Kind::SpreadElement { argument }, s.span)
                }
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
                            let method = Method {
                                kind,
                                key: self.key(&p.key)?,
                                computed: p.computed,
                                is_static: false,
                                func: self.func(f)?,
                            };
                            self.node(Kind::ObjectMethod(method), p.span)
                        }
                        None => {
                            let kind = Kind::ObjectProperty {
                                key: self.key(&p.key)?,
                                value: self.expr(&p.value)?,
                                computed: p.computed,
                                shorthand: p.shorthand,
                            };
                            self.node(kind, p.span)
                        }
                    }
                }
            });
        }
        Ok(self.node(Kind::ObjectExpression { properties }, o.span))
    }

    fn binary(
        &mut self,
        operator: &'static str,
        left: NodeId,
        right: &Expression<'_>,
    ) -> R<Binary> {
        Ok(Binary {
            operator,
            left,
            right: self.expr(right)?,
        })
    }

    fn raw_or_slice(&self, raw: Option<impl ToString>, span: Span) -> String {
        raw.map(|r| r.to_string())
            .unwrap_or_else(|| self.slice(span))
    }

    pub fn expr(&mut self, e: &Expression<'_>) -> R<NodeId> {
        let span = e.span();
        match e {
            Expression::ParenthesizedExpression(p) => {
                let inner = self.expr(&p.expression)?;
                self.tree.node_mut(inner).parenthesized = true;
                self.tree.parens.push((inner, p.span.start, p.span.end));
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
            Expression::StringLiteral(s) => Ok(self.string_literal(s)),
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
            Expression::Identifier(id) => Kind::Identifier {
                name: id.name.to_string(),
            },
            Expression::Super(_) => Kind::Super,
            Expression::ThisExpression(_) => Kind::ThisExpression,
            _ => return None,
        })
    }

    fn operator_kind(&mut self, e: &Expression<'_>) -> R<Option<Kind>> {
        Ok(Some(match e {
            Expression::AssignmentExpression(a) => {
                let left = self.target(&a.left)?;
                let right = self.expr(&a.right)?;
                Kind::AssignmentExpression(Binary {
                    operator: a.operator.as_str(),
                    left,
                    right,
                })
            }
            Expression::AwaitExpression(a) => Kind::AwaitExpression {
                argument: self.expr(&a.argument)?,
            },
            Expression::BinaryExpression(x) => {
                let left = self.expr(&x.left)?;
                Kind::BinaryExpression(self.binary(x.operator.as_str(), left, &x.right)?)
            }
            Expression::PrivateInExpression(x) => {
                let left = self.private_name(&x.left.name, x.left.span);
                Kind::BinaryExpression(self.binary("in", left, &x.right)?)
            }
            Expression::LogicalExpression(x) => {
                let left = self.expr(&x.left)?;
                Kind::LogicalExpression(self.binary(x.operator.as_str(), left, &x.right)?)
            }
            Expression::ConditionalExpression(c) => Kind::ConditionalExpression {
                test: self.expr(&c.test)?,
                consequent: self.expr(&c.consequent)?,
                alternate: self.expr(&c.alternate)?,
            },
            Expression::SequenceExpression(s) => Kind::SequenceExpression {
                expressions: self.exprs(&s.expressions)?,
            },
            Expression::UnaryExpression(u) => Kind::UnaryExpression {
                operator: u.operator.as_str(),
                argument: self.expr(&u.argument)?,
            },
            Expression::UpdateExpression(u) => Kind::UpdateExpression {
                operator: u.operator.as_str(),
                prefix: u.prefix,
                argument: self.simple_target(&u.argument)?,
            },
            Expression::YieldExpression(y) => Kind::YieldExpression {
                argument: self.opt_expr(y.argument.as_ref())?,
                delegate: y.delegate,
            },
            _ => return Ok(None),
        }))
    }

    fn array(&mut self, a: &oxc_ast::ast::ArrayExpression<'_>) -> R<Kind> {
        let mut elements = Vec::with_capacity(a.elements.len());
        for el in &a.elements {
            elements.push(match el {
                ArrayExpressionElement::Elision(_) => NodeId::NONE,
                ArrayExpressionElement::SpreadElement(s) => {
                    let argument = self.expr(&s.argument)?;
                    self.node(Kind::SpreadElement { argument }, s.span)
                }
                other => self.expr(other.to_expression())?,
            });
        }
        Ok(Kind::ArrayExpression { elements })
    }

    fn import_call(&mut self, i: &oxc_ast::ast::ImportExpression<'_>) -> R<Kind> {
        if i.phase.is_some() {
            return Err("import.source()/import.defer() are not ported".into());
        }
        let callee = self.node(Kind::Import, Span::new(i.span.start, i.span.start + 6));
        let mut arguments = vec![self.expr(&i.source)?];
        if let Some(opts) = &i.options {
            arguments.push(self.expr(opts)?);
        }
        Ok(Kind::CallExpression(Call {
            callee,
            arguments,
            optional: false,
        }))
    }

    fn meta(&mut self, meta: &str, property: &str, span: Span) -> Kind {
        let m = meta.len() as u32;
        let p = property.len() as u32;
        Kind::MetaProperty {
            meta: self.ident(meta, Span::new(span.start, span.start + m)),
            property: self.ident(property, Span::new(span.end - p, span.end)),
        }
    }

    fn other_kind(&mut self, e: &Expression<'_>) -> R<Kind> {
        Ok(match e {
            Expression::ArrayExpression(a) => self.array(a)?,
            Expression::ClassExpression(c) => Kind::ClassExpression(self.class(c)?),
            Expression::FunctionExpression(f) => Kind::FunctionExpression(self.func(f)?),
            Expression::ImportExpression(i) => self.import_call(i)?,
            Expression::NewExpression(n) => Kind::NewExpression(Call {
                callee: self.expr(&n.callee)?,
                arguments: self.arguments(&n.arguments)?,
                optional: false,
            }),
            Expression::TaggedTemplateExpression(t) => Kind::TaggedTemplateExpression {
                tag: self.expr(&t.tag)?,
                quasi: self.template(&t.quasi)?,
            },
            Expression::ImportMeta(m) => self.meta("import", "meta", m.span),
            Expression::NewTarget(m) => self.meta("new", "target", m.span),
            other => {
                let span = other.span();
                return Err(format!(
                    "unsupported expression at {}..{}",
                    span.start, span.end
                ));
            }
        })
    }
}
