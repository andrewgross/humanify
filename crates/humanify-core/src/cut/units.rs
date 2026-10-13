//! The statement-level cut (docs/matching-architecture.md §3.1): a
//! container's statements as UNITS — the one unit model the statement
//! reference graph ([`crate::graph::references`]) and the library index's
//! unit fingerprints ([`crate::libindex::units`]) share, so the index side
//! and the bundle side can never cut differently.
//!
//! **Units** (docs/design/library-discovery.md §2): every statement of the
//! container, except that a multi-declarator `var a, b;` gives each
//! declarator its own unit (a Bun lazy module hoists its bindings that
//! way); a bundled CommonJS module is ONE unit (or, on the index side, a
//! container whose factory-body statements are units of their own); a lazy
//! ES module's wrapper is a unit owning only its overhead, and each
//! statement of its init body is a unit of its own.
//!
//! Which helpers start a lazy module, fill a namespace or belong to the
//! runtime is the toolchain's answer ([`BundlerFacts::recognise`]); the
//! cutter never names a bundler.

use std::collections::{HashMap, HashSet};

use oxc_ast::ast::{
    Argument, ArrowFunctionExpression, BindingPattern, Expression, Statement, VariableDeclarator,
};
use oxc_span::{GetSpan, Span};

use crate::babel_view::unparen;
use crate::toolchain::{InteropHelpers, ModuleWrapperGrammar};

/// What the cutter (and the reference graph) needs to know about the
/// bundler (docs/design/library-discovery.md §3a):
/// which top-level bindings are its runtime helpers (externals, never
/// candidates), which helpers start a lazy ES module, and which helpers
/// FILL a namespace object (a call defines its first argument). Read off
/// the run's toolchain pieces by [`BundlerFacts::recognise`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BundlerFacts {
    /// The grammar whose factory-argument forms a lazy wrapper's init takes.
    pub grammar: ModuleWrapperGrammar,
    pub lazy_init: HashSet<String>,
    pub namespace_fillers: HashSet<String>,
    pub runtime: HashSet<String>,
}

impl Default for BundlerFacts {
    fn default() -> Self {
        BundlerFacts {
            grammar: ModuleWrapperGrammar::BunAndEsbuild,
            lazy_init: HashSet::new(),
            namespace_fillers: HashSet::new(),
            runtime: HashSet::new(),
        }
    }
}

/// React Compiler's cache sentinels (`var z = Symbol.for("react.memo_cache_sentinel")`):
/// compiler output every compiled component reads, never a library's code.
const COMPILER_SENTINELS: [&str; 2] = ["react.memo_cache_sentinel", "react.early_return_sentinel"];

impl BundlerFacts {
    /// The bundler facts of a container's statements, from the run's
    /// pieces: the module grammar's factory helper and lazy-init helpers,
    /// the interop helpers' shapes and namespace fillers, and React
    /// Compiler's sentinels. Every one of them is a runtime helper.
    pub fn recognise(
        code: &str,
        statements: &[Statement<'_>],
        grammar: ModuleWrapperGrammar,
        interop: InteropHelpers,
    ) -> BundlerFacts {
        let lazy_init = grammar.lazy_init_helpers_in(statements);
        let namespace_fillers = interop.namespace_fillers(code, statements);
        let mut runtime: HashSet<String> = lazy_init.union(&namespace_fillers).cloned().collect();
        if let Some(helper) = grammar.identify_factory_helper(code) {
            runtime.insert(helper.name);
        }
        for d in top_declarators(statements) {
            let (BindingPattern::BindingIdentifier(id), Some(init)) = (&d.id, &d.init) else {
                continue;
            };
            if interop.recognise(code, init).is_some()
                || interop.runtime_shape(code, init).is_some()
                || is_compiler_sentinel(init)
            {
                runtime.insert(id.name.to_string());
            }
        }
        BundlerFacts {
            grammar,
            lazy_init,
            namespace_fillers,
            runtime,
        }
    }
}

fn top_declarators<'s, 'a>(
    statements: &'s [Statement<'a>],
) -> impl Iterator<Item = &'s VariableDeclarator<'a>> {
    statements.iter().flat_map(|s| match s {
        Statement::VariableDeclaration(d) => d.declarations.as_slice(),
        _ => &[],
    })
}

fn is_compiler_sentinel(init: &Expression<'_>) -> bool {
    let Expression::CallExpression(call) = unparen(init) else {
        return false;
    };
    let Some(m) = call.callee.as_member_expression() else {
        return false;
    };
    matches!(m.object(), Expression::Identifier(o) if o.name == "Symbol")
        && m.static_property_name() == Some("for")
        && matches!(
            call.arguments.first(),
            Some(Argument::StringLiteral(s)) if COMPILER_SENTINELS.contains(&s.value.as_str())
        )
}

/// What one unit is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitKind {
    /// A top-level statement, or one declarator of a multi-declarator `var`.
    Statement,
    /// A lazy ES module's wrapper (`var I = init(() => { … })`): it owns
    /// only the wrapper overhead; its body statements are their own units.
    LazyModule,
    /// One statement of a lazy module's init body.
    LazyBody { module: u32 },
    /// One statement of a bundled module's factory body — the index side
    /// only ([`container_units`] with `factory_bodies`).
    FactoryBody { module: u32 },
    /// A bundled CommonJS module: one unit, external.
    Factory,
}

/// One statement-level piece: a node of the reference graph, a library
/// unit on the index side.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unit {
    pub span: Span,
    pub kind: UnitKind,
    /// The bytes this unit OWNS (a lazy wrapper: its span minus its body's).
    pub bytes: u32,
    /// A bundler runtime helper, or code only they reference.
    pub runtime: bool,
}

/// The AST node a unit stands for: a statement, one declarator of a split
/// `var`, or a container (a lazy wrapper or a bundled module — its own
/// statements are their own units, or it is external).
#[derive(Clone, Copy, Debug)]
pub enum UnitNode<'a> {
    Statement(&'a Statement<'a>),
    Declarator(&'a VariableDeclarator<'a>),
    Container,
}

/// The units of a container, their nodes (parallel to `units`), and which
/// unit declares each top-level name.
pub struct ContainerUnits<'a> {
    pub units: Vec<Unit>,
    pub nodes: Vec<UnitNode<'a>>,
    /// Per unit, whether it is one declarator of a split multi-declarator
    /// `var` (a container unit included) — the cutter's piece kind.
    pub split_declarator: Vec<bool>,
    pub(crate) declared: HashMap<String, u32>,
}

/// The units of a container's statements (module doc): the ONE unit model,
/// shared by the reference graph and the library index's unit fingerprints
/// (`libindex::units`). `factory_bodies`: also give each statement of a
/// bundled module's factory body its own unit (`UnitKind::FactoryBody`) —
/// the index side, where library code sits inside modules; the graph keeps
/// a module as one external unit.
pub fn container_units<'a>(
    statements: &'a [Statement<'a>],
    factories: &[Span],
    facts: &BundlerFacts,
    factory_bodies: bool,
) -> ContainerUnits<'a> {
    let mut builder = UnitBuilder {
        facts,
        factories: factories.iter().map(|s| (s.start, s.end)).collect(),
        factory_bodies,
        in_split: false,
        out: ContainerUnits {
            units: Vec::new(),
            nodes: Vec::new(),
            split_declarator: Vec::new(),
            declared: HashMap::new(),
        },
    };
    for stmt in statements {
        builder.statement(stmt);
    }
    builder.out
}

struct UnitBuilder<'f, 'a> {
    facts: &'f BundlerFacts,
    factories: HashSet<(u32, u32)>,
    factory_bodies: bool,
    /// Pushing the declarators of a split `var`.
    in_split: bool,
    out: ContainerUnits<'a>,
}

impl<'a> UnitBuilder<'_, 'a> {
    fn push(&mut self, span: Span, kind: UnitKind, node: UnitNode<'a>) -> u32 {
        self.out.units.push(Unit {
            span,
            kind,
            bytes: span.size(),
            runtime: false,
        });
        self.out.nodes.push(node);
        self.out.split_declarator.push(self.in_split);
        self.out.units.len() as u32 - 1
    }

    fn declare(&mut self, name: &str, u: u32) {
        self.out.declared.insert(name.to_string(), u);
    }

    fn statement(&mut self, stmt: &'a Statement<'a>) {
        match stmt {
            Statement::VariableDeclaration(decl) => {
                let split = decl.declarations.len() > 1;
                self.in_split = split;
                for d in &decl.declarations {
                    let (span, node) = if split {
                        (d.span, UnitNode::Declarator(d))
                    } else {
                        (decl.span, UnitNode::Statement(stmt))
                    };
                    let u = self.declarator(d, span, node);
                    if let BindingPattern::BindingIdentifier(id) = &d.id {
                        self.declare(&id.name, u);
                    }
                }
                self.in_split = false;
            }
            Statement::FunctionDeclaration(f) => {
                let u = self.push(f.span, UnitKind::Statement, UnitNode::Statement(stmt));
                if let Some(id) = &f.id {
                    self.declare(&id.name, u);
                }
            }
            Statement::ClassDeclaration(c) => {
                let u = self.push(c.span, UnitKind::Statement, UnitNode::Statement(stmt));
                if let Some(id) = &c.id {
                    self.declare(&id.name, u);
                }
            }
            s => {
                self.push(s.span(), UnitKind::Statement, UnitNode::Statement(stmt));
            }
        }
    }

    /// A container unit, then one unit per statement of its body.
    fn container(&mut self, span: Span, kind: UnitKind, body: &'a [Statement<'a>]) -> u32 {
        let module = self.push(span, kind, UnitNode::Container);
        let body_kind = match kind {
            UnitKind::Factory => UnitKind::FactoryBody { module },
            _ => UnitKind::LazyBody { module },
        };
        let mut owned = span.size();
        let in_split = std::mem::replace(&mut self.in_split, false);
        for s in body {
            self.push(s.span(), body_kind, UnitNode::Statement(s));
            owned -= s.span().size();
        }
        self.in_split = in_split;
        self.out.units[module as usize].bytes = owned;
        module
    }

    fn declarator(&mut self, d: &'a VariableDeclarator<'a>, span: Span, node: UnitNode<'a>) -> u32 {
        if self.factories.contains(&(d.span.start, d.span.end)) {
            return match factory_body(d, self.facts).filter(|_| self.factory_bodies) {
                Some(body) => self.container(span, UnitKind::Factory, body),
                None => self.push(span, UnitKind::Factory, UnitNode::Container),
            };
        }
        match lazy_init_body(d, self.facts) {
            Some(body) => self.container(span, UnitKind::LazyModule, body),
            None => self.push(span, UnitKind::Statement, node),
        }
    }
}

/// A bundled module's factory body: the single argument of its
/// `HELPER(…)` call, in one of the grammar's factory forms.
fn factory_body<'a>(
    d: &'a VariableDeclarator<'a>,
    facts: &BundlerFacts,
) -> Option<&'a [Statement<'a>]> {
    let Expression::CallExpression(call) = unparen(d.init.as_ref()?) else {
        return None;
    };
    let arg = call.arguments.first()?.as_expression()?;
    function_block(facts.grammar.factory_function(arg)?.function)
}

fn function_block<'a>(function: &'a Expression<'a>) -> Option<&'a [Statement<'a>]> {
    match function {
        Expression::ArrowFunctionExpression(a) => arrow_block(a),
        Expression::FunctionExpression(f) => Some(f.body.as_ref()?.statements.as_slice()),
        _ => None,
    }
}

/// A lazy wrapper's init body: `var I = init(<function>)` where `init` is
/// one of the bundle's lazy-init helpers and the argument takes one of the
/// grammar's factory forms with a block body.
fn lazy_init_body<'a>(
    d: &'a VariableDeclarator<'a>,
    facts: &BundlerFacts,
) -> Option<&'a [Statement<'a>]> {
    let Expression::CallExpression(call) = unparen(d.init.as_ref()?) else {
        return None;
    };
    let Expression::Identifier(callee) = unparen(&call.callee) else {
        return None;
    };
    if !facts.lazy_init.contains(callee.name.as_str()) || call.arguments.len() != 1 {
        return None;
    }
    function_block(
        facts
            .grammar
            .factory_function(call.arguments[0].as_expression()?)?
            .function,
    )
}

fn arrow_block<'a>(a: &'a ArrowFunctionExpression<'a>) -> Option<&'a [Statement<'a>]> {
    match &a.body {
        oxc_ast::ast::ArrowFunctionBody::FunctionBody(b) => Some(b.statements.as_slice()),
        _ => None,
    }
}

/// Which unit holds a byte position — the innermost one: a body unit
/// (lazy-init or factory body) before its container. The ONE owner of the
/// position → unit question (the reference graph's edges and discovery's
/// seeds both ask it).
#[derive(Clone, Debug, Default)]
pub struct UnitLocator {
    /// Top-level units, then body units, each in source order: the
    /// lookup's two levels.
    top: Vec<u32>,
    inner: Vec<u32>,
}

impl UnitLocator {
    pub fn new(units: &[Unit]) -> UnitLocator {
        let (inner, top): (Vec<u32>, Vec<u32>) = (0..units.len() as u32).partition(|&u| {
            matches!(
                units[u as usize].kind,
                UnitKind::LazyBody { .. } | UnitKind::FactoryBody { .. }
            )
        });
        UnitLocator { top, inner }
    }

    /// The innermost unit of `units` (the list this locator was built
    /// over) holding byte `pos`.
    pub fn unit_at(&self, units: &[Unit], pos: u32) -> Option<u32> {
        find(units, &self.inner, pos).or_else(|| find(units, &self.top, pos))
    }
}

fn find(units: &[Unit], level: &[u32], pos: u32) -> Option<u32> {
    let i = level.partition_point(|&u| units[u as usize].span.start <= pos);
    let u = *level.get(i.checked_sub(1)?)?;
    (pos < units[u as usize].span.end).then_some(u)
}
