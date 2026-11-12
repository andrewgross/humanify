//! One chunk, read once: its imports, exports, lazy loads, top-level
//! statements and top-level bindings (with every site that names them),
//! as owned facts — the parse is dropped before the next chunk is read.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use oxc_allocator::Allocator;
use oxc_ast::AstKind;
use oxc_ast::ast::{
    BindingPattern, Class, ClassElement, Declaration, ExportDefaultDeclarationKind, Expression,
    ImportDeclarationSpecifier, ModuleExportName, ObjectPropertyKind, Statement,
};
use oxc_parser::Parser;
use oxc_semantic::{AstNodes, Scoping, SemanticBuilder, SymbolId};
use oxc_span::{GetSpan, SourceType, Span};
use oxc_syntax::scope::ScopeId;
use oxc_syntax::symbol::SymbolFlags;

use super::ROOT;

/// A text edit: replace `[start, end)` with `text`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Edit {
    pub start: usize,
    pub end: usize,
    pub text: String,
}

/// Where an import or a lazy load points.
#[derive(Clone, Debug)]
pub(super) enum Spec {
    /// A sibling chunk, by its index in the load order.
    Chunk(usize),
    /// Anything else (a Node built-in, an external package).
    External(String),
}

#[derive(Clone, Debug)]
pub(super) enum Imported {
    Name(String),
    Default,
    Namespace,
}

pub(super) struct ImportDecl {
    pub spec: Spec,
    pub specifiers: Vec<(Imported, SymbolId)>,
}

#[derive(Clone, Debug)]
pub(super) enum ExportTarget {
    Local(SymbolId),
    From(Spec, Imported),
    /// `export default <expression>`: a binding the linker makes.
    DefaultExpr,
}

/// A load that happens when the code runs, not when the chunk loads.
pub(super) struct LazyEdge {
    /// The whole `import(…)` / `import.meta.require(…)` expression.
    pub span: Span,
    pub target: usize,
    pub sync: bool,
}

/// `None`: the value has effects or reads state. `Some(helpers)`: free
/// of both, provided every listed binding is a thunk factory.
pub(super) type Purity = Option<Vec<SymbolId>>;

pub(super) struct Declarator {
    pub pattern: Span,
    pub init: Option<Span>,
    pub simple: bool,
    pub names: Vec<SymbolId>,
    pub purity: Purity,
}

pub(super) enum StmtKind {
    /// An import or `export {…}` / `export … from`: removed.
    Removed,
    /// `export <declaration>`: the declaration stays, `[start, decl)` goes.
    ExportPrefix(u32),
    Function,
    Class {
        pure: bool,
        name: SymbolId,
    },
    Var(Vec<Declarator>),
    DefaultExpr {
        expr: Span,
        purity: Purity,
    },
    /// `nested_decl`: declares a top-level `var` from inside a block.
    Other {
        nested_decl: bool,
    },
}

pub(super) struct Stmt {
    pub span: Span,
    pub kind: StmtKind,
}

/// One place a binding's name is written.
#[derive(Clone, Copy)]
pub(super) struct Site {
    pub span: Span,
    /// `{a}` (or `{a} = …`, `var {a} = …`): renaming needs `a: new`.
    pub shorthand: bool,
}

pub(super) struct Sym {
    pub id: SymbolId,
    pub name: String,
    pub is_import: bool,
    pub decls: Vec<Site>,
    pub refs: Vec<Site>,
}

pub(super) struct Chunk {
    pub path: PathBuf,
    pub text: String,
    pub imports: Vec<ImportDecl>,
    pub exports: Vec<(String, ExportTarget)>,
    pub lazy: Vec<LazyEdge>,
    pub stmts: Vec<Stmt>,
    /// Top-level bindings, in declaration order.
    pub syms: Vec<Sym>,
    pub sym_index: BTreeMap<SymbolId, usize>,
    /// Names declared in any nested scope.
    pub nested_names: BTreeSet<String>,
    /// Names read but bound nowhere in the chunk (globals).
    pub unresolved: BTreeSet<String>,
    /// Top-level `var h = (…) => () => …` bindings: calling one has no
    /// effect (Bun's `__esm` / `__commonJS` shapes).
    pub thunk_factories: BTreeSet<SymbolId>,
    /// The chunk's Bun `__toESM` helper, recognised by shape (the
    /// toolchain's interop piece), when it declares one.
    pub to_esm: Option<SymbolId>,
    /// Block comments before the first statement (`/*!` licences).
    pub leading_comments: Vec<String>,
    /// `import.meta.*` rewritten to the CommonJS bindings.
    pub meta_edits: Vec<Edit>,
    pub static_deps: Vec<usize>,
}

/// The load queue: every chunk path gets an index the first time any
/// chunk names it.
pub(super) struct Loader {
    pub dir: PathBuf,
    pub index: BTreeMap<PathBuf, usize>,
    pub queue: Vec<PathBuf>,
}

impl Loader {
    pub fn new(entry: &Path) -> Loader {
        let dir = entry.parent().unwrap_or(Path::new(".")).to_path_buf();
        Loader {
            dir,
            index: BTreeMap::from([(entry.to_path_buf(), 0)]),
            queue: vec![entry.to_path_buf()],
        }
    }

    /// A `/$bunfs/root/<rest>` specifier → its sibling file, queued;
    /// anything else is external. A bunfs specifier whose file is
    /// missing fails: the layout would be linked with a hole.
    pub fn spec(&mut self, s: &str) -> Result<Spec, String> {
        let Some(rest) = s.strip_prefix(ROOT) else {
            return Ok(Spec::External(s.to_string()));
        };
        let path = self.dir.join(rest);
        if !path.is_file() {
            return Err(format!(
                "{s} is imported but {} does not exist",
                path.display()
            ));
        }
        let next = self.index.len();
        let id = *self.index.entry(path.clone()).or_insert(next);
        if id == next {
            self.queue.push(path);
        }
        Ok(Spec::Chunk(id))
    }
}

/// Read and analyse one chunk.
pub(super) fn analyse(path: &Path, loader: &mut Loader) -> Result<Chunk, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let alloc = Allocator::default();
    let ret = Parser::new(&alloc, &text, SourceType::mjs()).parse();
    if let Some(e) = ret.diagnostics.first() {
        return Err(format!(
            "{} does not parse as a module: {e}",
            path.display()
        ));
    }
    let program = alloc.alloc(ret.program);
    let semantic = SemanticBuilder::new()
        .with_build_nodes(true)
        .build(program)
        .semantic;
    let ctx = Ctx {
        scoping: semantic.scoping(),
        nodes: semantic.nodes(),
        root: semantic.scoping().root_scope_id(),
        path,
    };
    let mut chunk = Chunk {
        path: path.to_path_buf(),
        text: String::new(),
        imports: Vec::new(),
        exports: Vec::new(),
        lazy: Vec::new(),
        stmts: Vec::new(),
        syms: Vec::new(),
        sym_index: BTreeMap::new(),
        nested_names: BTreeSet::new(),
        unresolved: BTreeSet::new(),
        thunk_factories: BTreeSet::new(),
        to_esm: None,
        leading_comments: Vec::new(),
        meta_edits: Vec::new(),
        static_deps: Vec::new(),
    };
    for st in &program.body {
        let kind = ctx.statement(st, loader, &mut chunk)?;
        chunk.stmts.push(Stmt {
            span: st.span(),
            kind,
        });
    }
    chunk.thunk_factories = thunk_factories(&program.body);
    chunk.to_esm = to_esm_helper(&text, &program.body, ctx.scoping);
    let shorthand = ctx.walk_nodes(loader, &mut chunk)?;
    ctx.bindings(&mut chunk, &shorthand);
    mark_nested_decls(&mut chunk);
    let first = program
        .body
        .first()
        .map_or(text.len(), |s| s.span().start as usize);
    chunk.leading_comments = program
        .comments
        .iter()
        .filter(|c| c.is_block() && (c.span.end as usize) <= first)
        .map(|c| text[c.span.start as usize..c.span.end as usize].to_string())
        .collect();
    drop(semantic);
    chunk.text = text;
    Ok(chunk)
}

struct Ctx<'s, 'a> {
    scoping: &'s Scoping,
    nodes: &'s AstNodes<'a>,
    root: ScopeId,
    path: &'s Path,
}

fn export_name(n: &ModuleExportName<'_>) -> String {
    match n {
        ModuleExportName::IdentifierName(i) => i.name.to_string(),
        ModuleExportName::IdentifierReference(i) => i.name.to_string(),
        ModuleExportName::StringLiteral(s) => s.value.to_string(),
    }
}

fn unparen<'b, 'a>(e: &'b Expression<'a>) -> &'b Expression<'a> {
    match e {
        Expression::ParenthesizedExpression(p) => unparen(&p.expression),
        _ => e,
    }
}

fn binding_names(p: &BindingPattern<'_>, out: &mut Vec<SymbolId>) {
    match p {
        BindingPattern::BindingIdentifier(b) => out.push(b.symbol_id()),
        BindingPattern::ObjectPattern(o) => {
            o.properties
                .iter()
                .for_each(|pr| binding_names(&pr.value, out));
            if let Some(r) = &o.rest {
                binding_names(&r.argument, out);
            }
        }
        BindingPattern::ArrayPattern(a) => {
            a.elements
                .iter()
                .flatten()
                .for_each(|el| binding_names(el, out));
            if let Some(r) = &a.rest {
                binding_names(&r.argument, out);
            }
        }
        BindingPattern::AssignmentPattern(a) => binding_names(&a.left, out),
    }
}

/// A class whose evaluation runs nothing: no heritage, decorators,
/// computed keys, static fields or static blocks.
fn class_is_pure(c: &Class<'_>) -> bool {
    c.heritage.is_none()
        && c.decorators.is_empty()
        && c.body.body.iter().all(|el| match el {
            ClassElement::MethodDefinition(m) => !m.computed,
            ClassElement::PropertyDefinition(p) => !p.computed && !p.r#static,
            ClassElement::AccessorProperty(a) => !a.computed && !a.r#static,
            _ => false,
        })
}

fn is_function_value(e: &Expression<'_>) -> bool {
    matches!(
        unparen(e),
        Expression::FunctionExpression(_) | Expression::ArrowFunctionExpression(_)
    )
}

/// `(…) => () => …` / `(…) => function …`: Bun's `__esm` and
/// `__commonJS` helper shapes, whose call only builds a thunk.
fn is_thunk_factory(e: &Expression<'_>) -> bool {
    match unparen(e) {
        Expression::ArrowFunctionExpression(a) => {
            a.body.as_expression().is_some_and(is_function_value)
        }
        _ => false,
    }
}

/// The first never-written top-level binding whose value has Bun's
/// `__toESM` shape.
fn to_esm_helper(text: &str, body: &[Statement<'_>], scoping: &Scoping) -> Option<SymbolId> {
    body.iter()
        .filter_map(|st| match st {
            Statement::VariableDeclaration(v) => Some(v),
            _ => None,
        })
        .flat_map(|v| v.declarations.iter())
        .find_map(|d| match (&d.id, &d.init) {
            (BindingPattern::BindingIdentifier(b), Some(init))
                if !scoping.symbol_is_mutated(b.symbol_id())
                    && crate::toolchain::InteropHelpers::Bun.recognise(text, init)
                        == Some("__toESM") =>
            {
                Some(b.symbol_id())
            }
            _ => None,
        })
}

fn thunk_factories(body: &[Statement<'_>]) -> BTreeSet<SymbolId> {
    let mut out = BTreeSet::new();
    for st in body {
        let Statement::VariableDeclaration(v) = st else {
            continue;
        };
        for d in &v.declarations {
            if let (BindingPattern::BindingIdentifier(b), Some(init)) = (&d.id, &d.init)
                && is_thunk_factory(init)
            {
                out.insert(b.symbol_id());
            }
        }
    }
    out
}

impl Ctx<'_, '_> {
    fn symbol_of_ref(&self, r: &oxc_ast::ast::IdentifierReference<'_>) -> Option<SymbolId> {
        self.scoping.get_reference(r.reference_id()).symbol_id()
    }

    /// Is evaluating `e` free of effects and of reads of other bindings?
    /// A call counts only when its callee is a top-level binding (checked
    /// to be a thunk factory once every chunk is known) and every argument
    /// is a function.
    fn purity(&self, e: &Expression<'_>, helpers: &mut Vec<SymbolId>) -> bool {
        use oxc_syntax::operator::UnaryOperator as U;
        match unparen(e) {
            Expression::BooleanLiteral(_)
            | Expression::NullLiteral(_)
            | Expression::NumericLiteral(_)
            | Expression::BigIntLiteral(_)
            | Expression::StringLiteral(_)
            | Expression::RegExpLiteral(_)
            | Expression::FunctionExpression(_)
            | Expression::ArrowFunctionExpression(_) => true,
            Expression::TemplateLiteral(t) => t.expressions.is_empty(),
            Expression::UnaryExpression(u) => {
                matches!(u.operator, U::Void | U::LogicalNot | U::UnaryNegation)
                    && self.purity(&u.argument, helpers)
            }
            Expression::ArrayExpression(a) => a.elements.iter().all(|el| {
                el.is_elision() || el.as_expression().is_some_and(|x| self.purity(x, helpers))
            }),
            Expression::ObjectExpression(o) => o.properties.iter().all(|p| match p {
                ObjectPropertyKind::SpreadProperty(_) => false,
                ObjectPropertyKind::ObjectProperty(p) => {
                    !p.computed && self.purity(&p.value, helpers)
                }
            }),
            Expression::CallExpression(c) => self.helper_call(c, helpers),
            _ => false,
        }
    }

    fn helper_call(
        &self,
        c: &oxc_ast::ast::CallExpression<'_>,
        helpers: &mut Vec<SymbolId>,
    ) -> bool {
        let Expression::Identifier(callee) = unparen(&c.callee) else {
            return false;
        };
        let Some(sym) = self.symbol_of_ref(callee) else {
            return false;
        };
        let args_are_functions = c
            .arguments
            .iter()
            .all(|a| a.as_expression().is_some_and(is_function_value));
        if self.scoping.symbol_scope_id(sym) != self.root || !args_are_functions {
            return false;
        }
        helpers.push(sym);
        true
    }

    fn purity_of(&self, e: Option<&Expression<'_>>) -> Purity {
        let mut helpers = Vec::new();
        match e {
            None => Some(helpers),
            Some(e) => self.purity(e, &mut helpers).then_some(helpers),
        }
    }

    fn statement(
        &self,
        st: &Statement<'_>,
        loader: &mut Loader,
        chunk: &mut Chunk,
    ) -> Result<StmtKind, String> {
        Ok(match st {
            Statement::ImportDeclaration(d) => {
                let spec = loader.spec(&d.source.value)?;
                if let Spec::Chunk(c) = spec {
                    chunk.static_deps.push(c);
                }
                let specifiers = d
                    .specifiers
                    .iter()
                    .flatten()
                    .map(import_specifier)
                    .collect();
                chunk.imports.push(ImportDecl { spec, specifiers });
                StmtKind::Removed
            }
            Statement::ExportNamedDeclaration(d) => {
                for s in &d.specifiers {
                    let target = self.export_local(&s.local)?;
                    chunk
                        .exports
                        .push((export_name(&s.exported), ExportTarget::Local(target)));
                }
                StmtKind::Removed
            }
            Statement::ExportFromDeclaration(d) => {
                let spec = loader.spec(&d.source.value)?;
                if let Spec::Chunk(c) = spec {
                    chunk.static_deps.push(c);
                }
                for s in &d.specifiers {
                    let from =
                        ExportTarget::From(spec.clone(), Imported::Name(export_name(&s.local)));
                    chunk.exports.push((export_name(&s.exported), from));
                }
                StmtKind::Removed
            }
            Statement::ExportAllDeclaration(_) => {
                return Err(format!("{}: `export *` is not linked", self.path.display()));
            }
            Statement::ExportDefaultDeclaration(d) => self.export_default(&d.declaration, chunk),
            Statement::ExportDeclaration(d) => {
                self.export_declaration(&d.declaration, chunk);
                StmtKind::ExportPrefix(d.declaration.span().start)
            }
            Statement::FunctionDeclaration(_) => StmtKind::Function,
            Statement::ClassDeclaration(c) => match &c.id {
                Some(id) => StmtKind::Class {
                    pure: class_is_pure(c),
                    name: id.symbol_id(),
                },
                None => StmtKind::Other { nested_decl: false },
            },
            Statement::VariableDeclaration(v) => StmtKind::Var(self.declarators(&v.declarations)),
            _ => StmtKind::Other { nested_decl: false },
        })
    }

    fn export_local(&self, local: &ModuleExportName<'_>) -> Result<SymbolId, String> {
        match local {
            ModuleExportName::IdentifierReference(r) => self.symbol_of_ref(r),
            _ => None,
        }
        .ok_or_else(|| format!("{}: exports an unbound name", self.path.display()))
    }

    fn export_default(&self, d: &ExportDefaultDeclarationKind<'_>, chunk: &mut Chunk) -> StmtKind {
        let named = match d {
            ExportDefaultDeclarationKind::FunctionDeclaration(f) => {
                f.id.as_ref().map(|i| (i.symbol_id(), f.span))
            }
            ExportDefaultDeclarationKind::ClassDeclaration(c) => {
                c.id.as_ref().map(|i| (i.symbol_id(), c.span))
            }
            _ => None,
        };
        if let Some((sym, span)) = named {
            chunk
                .exports
                .push(("default".into(), ExportTarget::Local(sym)));
            return StmtKind::ExportPrefix(span.start);
        }
        chunk
            .exports
            .push(("default".into(), ExportTarget::DefaultExpr));
        StmtKind::DefaultExpr {
            expr: d.span(),
            purity: self.purity_of(d.as_expression()),
        }
    }

    fn export_declaration(&self, d: &Declaration<'_>, chunk: &mut Chunk) {
        let mut names = Vec::new();
        match d {
            Declaration::VariableDeclaration(v) => {
                v.declarations
                    .iter()
                    .for_each(|dc| binding_names(&dc.id, &mut names));
            }
            Declaration::FunctionDeclaration(f) => {
                names.extend(f.id.as_ref().map(|i| i.symbol_id()))
            }
            Declaration::ClassDeclaration(c) => names.extend(c.id.as_ref().map(|i| i.symbol_id())),
            _ => {}
        }
        for s in names {
            chunk.exports.push((
                self.scoping.symbol_name(s).to_string(),
                ExportTarget::Local(s),
            ));
        }
    }

    fn declarators(&self, decls: &[oxc_ast::ast::VariableDeclarator<'_>]) -> Vec<Declarator> {
        decls
            .iter()
            .map(|dc| {
                let mut names = Vec::new();
                binding_names(&dc.id, &mut names);
                Declarator {
                    pattern: dc.id.span(),
                    init: dc.init.as_ref().map(GetSpan::span),
                    simple: matches!(dc.id, BindingPattern::BindingIdentifier(_)),
                    names,
                    purity: self.purity_of(dc.init.as_ref()),
                }
            })
            .collect()
    }

    /// One pass over every node: lazy loads, `import.meta`, top-level
    /// await (refused), and the shorthand binding sites.
    fn walk_nodes(
        &self,
        loader: &mut Loader,
        chunk: &mut Chunk,
    ) -> Result<BTreeSet<(u32, u32)>, String> {
        let mut shorthand = BTreeSet::new();
        for node in self.nodes.iter() {
            match node.kind() {
                AstKind::BindingProperty(p) if p.shorthand => {
                    let s = p.value.span();
                    shorthand.insert((s.start, s.end));
                }
                AstKind::ImportExpression(ie) => {
                    if let Expression::StringLiteral(s) = &ie.source
                        && s.value.starts_with(ROOT)
                        && let Spec::Chunk(target) = loader.spec(&s.value)?
                    {
                        chunk.lazy.push(LazyEdge {
                            span: ie.span,
                            target,
                            sync: false,
                        });
                    }
                }
                AstKind::ImportMeta(m) => self.import_meta(node.id(), m.span, loader, chunk)?,
                AstKind::AwaitExpression(_) => self.refuse_top_level_await(node.id())?,
                AstKind::ForOfStatement(f) if f.r#await => {
                    self.refuse_top_level_await(node.id())?
                }
                _ => {}
            }
        }
        Ok(shorthand)
    }

    fn refuse_top_level_await(&self, id: oxc_semantic::NodeId) -> Result<(), String> {
        let in_function = self.nodes.ancestor_kinds(id).any(|k| {
            matches!(
                k,
                AstKind::Function(_) | AstKind::ArrowFunctionExpression(_)
            )
        });
        if in_function {
            return Ok(());
        }
        Err(format!(
            "{}: top-level await cannot be linked into one program",
            self.path.display()
        ))
    }

    /// `import.meta.<p>` → its CommonJS equivalent; `import.meta.require(
    /// "/$bunfs/root/…")` → a synchronous load of that chunk.
    fn import_meta(
        &self,
        id: oxc_semantic::NodeId,
        span: Span,
        loader: &mut Loader,
        chunk: &mut Chunk,
    ) -> Result<(), String> {
        let parent = self.nodes.parent_id(id);
        let AstKind::StaticMemberExpression(member) = self.nodes.kind(parent) else {
            return Err(format!(
                "{}: a bare `import.meta` is not linked",
                self.path.display()
            ));
        };
        let prop = member.property.name.as_str();
        if prop == "require"
            && let AstKind::CallExpression(call) = self.nodes.parent_kind(parent)
            && let Some(Expression::StringLiteral(s)) =
                call.arguments.first().and_then(|a| a.as_expression())
            && s.value.starts_with(ROOT)
            && let Spec::Chunk(target) = loader.spec(&s.value)?
        {
            chunk.lazy.push(LazyEdge {
                span: call.span,
                target,
                sync: true,
            });
            return Ok(());
        }
        let (text, needs) = match prop {
            "url" => (
                "require(\"url\").pathToFileURL(__filename).href",
                ["require", "__filename"].as_slice(),
            ),
            // `dir`, `path`, `file` and `main` are Bun's own properties.
            "dirname" | "dir" => ("__dirname", ["__dirname"].as_slice()),
            "filename" | "path" => ("__filename", ["__filename"].as_slice()),
            "file" => (
                "require(\"path\").basename(__filename)",
                ["require", "__filename"].as_slice(),
            ),
            "main" => ("(require.main===module)", ["require", "module"].as_slice()),
            "require" => ("require", ["require"].as_slice()),
            "env" => ("process.env", ["process"].as_slice()),
            other => {
                return Err(format!(
                    "{}: `import.meta.{other}` is not linked",
                    self.path.display()
                ));
            }
        };
        let scope = self.nodes.get_node(id).scope_id();
        if let Some(name) = needs
            .iter()
            .find(|n| self.scoping.find_binding(scope, (**n).into()).is_some())
        {
            return Err(format!(
                "{}: `import.meta.{prop}` sits where `{name}` is bound; not linked",
                self.path.display()
            ));
        }
        chunk.meta_edits.push(Edit {
            start: member.span.start as usize,
            end: member.span.end as usize,
            text: text.to_string(),
        });
        let _ = span;
        Ok(())
    }

    /// Every top-level binding with its declaration and reference sites;
    /// the nested-scope and unresolved name sets.
    fn bindings(&self, chunk: &mut Chunk, shorthand: &BTreeSet<(u32, u32)>) {
        let mut ids: Vec<SymbolId> = self
            .scoping
            .get_bindings(self.root)
            .values()
            .copied()
            .collect();
        ids.sort();
        let site = |span: Span| Site {
            span,
            shorthand: shorthand.contains(&(span.start, span.end)),
        };
        for id in ids {
            let mut decls = vec![site(self.scoping.symbol_span(id))];
            decls.extend(
                self.scoping
                    .symbol_redeclarations(id)
                    .iter()
                    .map(|r| site(r.span)),
            );
            let refs = self
                .scoping
                .get_resolved_references(id)
                .filter_map(|r| self.reference_site(r.node_id()))
                .collect();
            chunk.sym_index.insert(id, chunk.syms.len());
            chunk.syms.push(Sym {
                id,
                name: self.scoping.symbol_name(id).to_string(),
                is_import: self.scoping.symbol_flags(id).contains(SymbolFlags::Import),
                decls,
                refs,
            });
        }
        for id in self.scoping.symbol_ids() {
            if self.scoping.symbol_scope_id(id) != self.root {
                chunk
                    .nested_names
                    .insert(self.scoping.symbol_name(id).to_string());
            }
        }
        chunk.unresolved = self
            .scoping
            .root_unresolved_references()
            .keys()
            .map(|k| k.to_string())
            .collect();
    }

    fn reference_site(&self, node: oxc_semantic::NodeId) -> Option<Site> {
        let AstKind::IdentifierReference(id) = self.nodes.kind(node) else {
            return None;
        };
        let shorthand = match self.nodes.parent_kind(node) {
            AstKind::ObjectProperty(p) => p.shorthand,
            AstKind::AssignmentTargetPropertyIdentifier(_) => true,
            _ => false,
        };
        Some(Site {
            span: id.span,
            shorthand,
        })
    }
}

fn import_specifier(s: &ImportDeclarationSpecifier<'_>) -> (Imported, SymbolId) {
    match s {
        ImportDeclarationSpecifier::ImportSpecifier(s) => (
            Imported::Name(export_name(&s.imported)),
            s.local.symbol_id(),
        ),
        ImportDeclarationSpecifier::ImportDefaultSpecifier(s) => {
            (Imported::Default, s.local.symbol_id())
        }
        ImportDeclarationSpecifier::ImportNamespaceSpecifier(s) => {
            (Imported::Namespace, s.local.symbol_id())
        }
    }
}

/// A non-declaration statement that declares a top-level `var` from a
/// nested block (`if (x) { var y }`) must stay where it is: moved into a
/// chunk's init, its `var` would become the init's own.
fn mark_nested_decls(chunk: &mut Chunk) {
    let decl_spans: Vec<Span> = chunk
        .syms
        .iter()
        .flat_map(|s| s.decls.iter().map(|d| d.span))
        .collect();
    for st in &mut chunk.stmts {
        if let StmtKind::Other { nested_decl } = &mut st.kind {
            *nested_decl = decl_spans
                .iter()
                .any(|d| d.start >= st.span.start && d.end <= st.span.end);
        }
    }
}
