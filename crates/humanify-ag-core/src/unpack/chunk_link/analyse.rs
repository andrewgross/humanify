//! One chunk, read once: its imports, exports, lazy loads, top-level
//! statements and top-level bindings (with every site that names them),
//! as owned facts — the parse is dropped before the next chunk is read.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use oxc_allocator::Allocator;
use oxc_ast::AstKind;
use oxc_ast::ast::{
    BindingPattern, Class, Declaration, ExportDefaultDeclarationKind, Expression,
    ImportDeclarationSpecifier, ModuleExportName, Statement,
};
use oxc_parser::Parser;
use oxc_semantic::{AstNodes, Scoping, SemanticBuilder, SymbolId};
use oxc_span::{GetSpan, SourceType, Span};
use oxc_syntax::scope::ScopeId;
use oxc_syntax::symbol::SymbolFlags;

use super::ROOT;
use super::embedded::{Callee, FileRef, is_bun_module};

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

pub(super) struct Declarator {
    pub pattern: Span,
    pub init: Option<Span>,
    pub simple: bool,
    pub names: Vec<SymbolId>,
    /// `var x = h(fn, …)` (x a plain name, every argument a function, `h`
    /// a top-level binding): a factory call, order-free when `h` is a
    /// thunk factory ([`Chunk::thunk_factories`]) already declared.
    pub factory_call: Option<SymbolId>,
    /// `var h = (…) => () => …` (h a plain name, never written): a thunk
    /// factory, order-free (building a closure reads nothing).
    pub thunk_factory: bool,
    /// `var x = import.meta.require` (x never written): Bun's ESM runtime
    /// alias of `require`, which the linked program can drop for the
    /// wrapper's own `require` (see `names`).
    pub require_alias: bool,
}

pub(super) enum StmtKind {
    /// An import or `export {…}` / `export … from`: removed.
    Removed,
    /// A function declaration (also `export function` / `export default
    /// function f`).
    Function,
    Class {
        name: SymbolId,
    },
    Var(Vec<Declarator>),
    DefaultExpr {
        expr: Span,
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
    /// No reference sits where a nested scope binds `require`: the
    /// binding could be read as the wrapper's `require`.
    pub require_safe: bool,
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
    /// Top-level `var h = (…) => () => …` bindings, never written, with
    /// the index of the statement declaring each: calling one has no
    /// effect (Bun's `__esm` / `__commonJS` shapes).
    pub thunk_factories: BTreeMap<SymbolId, usize>,
    /// The chunk's Bun `__toESM` helper, recognised by shape (the
    /// toolchain's interop piece), when it declares one.
    pub to_esm: Option<SymbolId>,
    /// Block comments before the first statement (`/*!` licences).
    pub leading_comments: Vec<String>,
    /// `import.meta.*` rewritten to the CommonJS bindings.
    pub meta_edits: Vec<Edit>,
    /// `export <declaration>` prefixes (`[start, declaration)`), removed.
    pub export_prefixes: Vec<(u32, u32)>,
    pub static_deps: Vec<usize>,
    /// Every use of an embedded (non-chunk) bunfs file (`embedded`).
    pub file_refs: Vec<FileRef>,
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
        thunk_factories: BTreeMap::new(),
        to_esm: None,
        leading_comments: Vec::new(),
        meta_edits: Vec::new(),
        export_prefixes: Vec::new(),
        static_deps: Vec::new(),
        file_refs: Vec::new(),
    };
    for st in &program.body {
        let kind = ctx.statement(st, loader, &mut chunk)?;
        chunk.stmts.push(Stmt {
            span: st.span(),
            kind,
        });
    }
    chunk.thunk_factories = chunk
        .stmts
        .iter()
        .enumerate()
        .flat_map(|(i, st)| match &st.kind {
            StmtKind::Var(ds) => ds
                .iter()
                .filter(|d| d.thunk_factory)
                .flat_map(|d| d.names.iter().map(move |s| (*s, i)))
                .collect(),
            _ => Vec::new(),
        })
        .collect();
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

/// A class declaration's kind (an anonymous one is any other statement).
fn class_kind(c: &Class<'_>) -> StmtKind {
    match &c.id {
        Some(id) => StmtKind::Class {
            name: id.symbol_id(),
        },
        None => StmtKind::Other { nested_decl: false },
    }
}

/// `import.meta.require`.
fn is_import_meta_require(e: &Expression<'_>) -> bool {
    match unparen(e) {
        Expression::StaticMemberExpression(m) => {
            m.property.name == "require" && matches!(m.object, Expression::ImportMeta(_))
        }
        _ => false,
    }
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

impl Ctx<'_, '_> {
    fn symbol_of_ref(&self, r: &oxc_ast::ast::IdentifierReference<'_>) -> Option<SymbolId> {
        self.scoping.get_reference(r.reference_id()).symbol_id()
    }

    /// `h(fn, …)` with `h` a top-level binding and every argument a
    /// function: the callee.
    fn factory_call(&self, e: &Expression<'_>) -> Option<SymbolId> {
        let Expression::CallExpression(c) = unparen(e) else {
            return None;
        };
        let Expression::Identifier(callee) = unparen(&c.callee) else {
            return None;
        };
        let sym = self.symbol_of_ref(callee)?;
        let args_are_functions = !c.arguments.is_empty()
            && c.arguments
                .iter()
                .all(|a| a.as_expression().is_some_and(is_function_value));
        (self.scoping.symbol_scope_id(sym) == self.root && args_are_functions).then_some(sym)
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
            Statement::ExportDefaultDeclaration(d) => {
                self.export_default(d.span.start, &d.declaration, chunk)
            }
            Statement::ExportDeclaration(d) => {
                self.export_declaration(&d.declaration, chunk);
                chunk
                    .export_prefixes
                    .push((d.span.start, d.declaration.span().start));
                match &d.declaration {
                    Declaration::VariableDeclaration(v) => {
                        StmtKind::Var(self.declarators(&v.declarations))
                    }
                    Declaration::FunctionDeclaration(_) => StmtKind::Function,
                    Declaration::ClassDeclaration(c) => class_kind(c),
                    _ => StmtKind::Other { nested_decl: false },
                }
            }
            Statement::FunctionDeclaration(_) => StmtKind::Function,
            Statement::ClassDeclaration(c) => class_kind(c),
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

    fn export_default(
        &self,
        start: u32,
        d: &ExportDefaultDeclarationKind<'_>,
        chunk: &mut Chunk,
    ) -> StmtKind {
        let named = match d {
            ExportDefaultDeclarationKind::FunctionDeclaration(f) => {
                f.id.as_ref()
                    .map(|i| (i.symbol_id(), f.span, StmtKind::Function))
            }
            ExportDefaultDeclarationKind::ClassDeclaration(c) => {
                c.id.as_ref()
                    .map(|i| (i.symbol_id(), c.span, class_kind(c)))
            }
            _ => None,
        };
        if let Some((sym, span, kind)) = named {
            chunk
                .exports
                .push(("default".into(), ExportTarget::Local(sym)));
            chunk.export_prefixes.push((start, span.start));
            return kind;
        }
        chunk
            .exports
            .push(("default".into(), ExportTarget::DefaultExpr));
        StmtKind::DefaultExpr { expr: d.span() }
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
                let plain = match (&dc.id, &dc.init) {
                    (BindingPattern::BindingIdentifier(b), Some(init)) => {
                        Some((!self.scoping.symbol_is_mutated(b.symbol_id()), init))
                    }
                    _ => None,
                };
                Declarator {
                    pattern: dc.id.span(),
                    init: dc.init.as_ref().map(GetSpan::span),
                    simple: matches!(dc.id, BindingPattern::BindingIdentifier(_)),
                    names,
                    factory_call: plain.and_then(|(_, init)| self.factory_call(init)),
                    thunk_factory: plain
                        .is_some_and(|(unwritten, init)| unwritten && is_thunk_factory(init)),
                    require_alias: plain
                        .is_some_and(|(unwritten, init)| unwritten && is_import_meta_require(init)),
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
                AstKind::StringLiteral(s)
                    if s.value.len() > ROOT.len() && s.value.starts_with(ROOT) =>
                {
                    self.file_ref(node.id(), s, loader, chunk);
                }
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
            && is_chunk_specifier(loader, &s.value)
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

    /// A `"/$bunfs/root/<rest>"` string in expression position that is not
    /// a chunk specifier: a use of an embedded file (`embedded`).
    fn file_ref(
        &self,
        id: oxc_semantic::NodeId,
        s: &oxc_ast::ast::StringLiteral<'_>,
        loader: &Loader,
        chunk: &mut Chunk,
    ) {
        let mut call = None;
        let mut extra_args = false;
        match self.nodes.parent_kind(id) {
            AstKind::ImportDeclaration(_)
            | AstKind::ExportNamedDeclaration(_)
            | AstKind::ExportFromDeclaration(_)
            | AstKind::ExportAllDeclaration(_)
            | AstKind::ImportExpression(_)
            | AstKind::ImportSpecifier(_)
            | AstKind::ExportSpecifier(_)
            | AstKind::ImportAttribute(_)
            | AstKind::Directive(_) => return,
            AstKind::ObjectProperty(p) if p.key.span() == s.span => return,
            AstKind::BindingProperty(p) if p.key.span() == s.span => return,
            AstKind::MethodDefinition(p) if p.key.span() == s.span => return,
            AstKind::PropertyDefinition(p) if p.key.span() == s.span => return,
            AstKind::AccessorProperty(p) if p.key.span() == s.span => return,
            AstKind::CallExpression(c)
                if c.arguments.first().is_some_and(|a| a.span() == s.span) =>
            {
                extra_args = c.arguments.len() > 1;
                if is_import_meta_require(&c.callee) {
                    if is_chunk_specifier(loader, &s.value) {
                        return; // a synchronous chunk load (`lazy`)
                    }
                    call = Some((c.span, Callee::MetaRequire));
                } else if let Expression::Identifier(r) = unparen(&c.callee)
                    && let Some(sym) = self.symbol_of_ref(r)
                    && self.scoping.symbol_scope_id(sym) == self.root
                {
                    call = Some((c.span, Callee::Sym(sym)));
                }
            }
            _ => {}
        }
        let scope = self.nodes.get_node(id).scope_id();
        chunk.file_refs.push(FileRef {
            literal: s.span,
            rest: s.value[ROOT.len()..].to_string(),
            call,
            extra_args,
            dirname_free: self
                .scoping
                .find_binding(scope, "__dirname".into())
                .is_none(),
        });
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
            let require_safe = self
                .scoping
                .get_resolved_references(id)
                .all(|r| self.require_unshadowed(r.node_id()));
            chunk.sym_index.insert(id, chunk.syms.len());
            chunk.syms.push(Sym {
                id,
                name: self.scoping.symbol_name(id).to_string(),
                is_import: self.scoping.symbol_flags(id).contains(SymbolFlags::Import),
                decls,
                refs,
                require_safe,
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

    /// Does `require` at this node mean the program's top-level
    /// `require` (no nested scope binds the name)?
    fn require_unshadowed(&self, node: oxc_semantic::NodeId) -> bool {
        let scope = self.nodes.get_node(node).scope_id();
        self.scoping
            .find_binding(scope, "require".into())
            .is_none_or(|s| self.scoping.symbol_scope_id(s) == self.root)
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

/// Does `import.meta.require(spec)` load a chunk (a Bun-built module
/// beside the entry), not an embedded file?
fn is_chunk_specifier(loader: &Loader, spec: &str) -> bool {
    spec.strip_prefix(ROOT)
        .is_some_and(|rest| is_bun_module(&loader.dir.join(rest)))
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
