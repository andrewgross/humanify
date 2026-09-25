//! Babel's scope model, reconstructed over oxc (WP3.1).
//!
//! Every legality rule in `src/rename/validated-rename.ts` reads BABEL's
//! scope objects: `scope.bindings` (own map, keyed by the CURRENT name),
//! `scope.parent`, `binding.referencePaths` / `binding.constantViolations`
//! (each path with its own `path.scope` and node span), `scope.block`'s span,
//! and the program's `globals`. oxc's `Scoping` answers a different model in
//! a handful of places the probe (`test/parity/wp31-scope-probe.mjs`, frozen
//! at `test/parity/wp31-scope-view.json`) pinned, so this view is rebuilt
//! from AST ANCESTRY by Babel's own rules rather than read off oxc's scopes:
//!
//! - a catch clause and its body are ONE Babel scope (oxc: the param lives in
//!   the body block's scope);
//! - a function declaration in a block is BLOCK-scoped in Babel (oxc hoists
//!   it Annex-B style to the function scope), so a call after the block is a
//!   Babel GLOBAL (`h` in `if (a) { function h() {} } h();`);
//! - a class declaration's name is in the parent scope AND aliased in the
//!   class's own scope map (`path.scope.bindings[name] = parent binding`) —
//!   the alias is never updated by a rename through the parent scope;
//! - methods are one scope whose block span INCLUDES the key (oxc: the inner
//!   Function starts at the params);
//! - a method's computed key / decorators, and a switch's discriminant,
//!   resolve in the ENCLOSING scope (`setScope`'s special case);
//! - a pattern or defaulted parameter is its own (binding-less) scope, and a
//!   lookup climbing out of one skips non-param bindings of the function
//!   (`getBinding`'s spec 9.2.10.28 rule); `arguments` inside a non-arrow
//!   function never resolves outward;
//! - `export` declarations are REFERENCE paths of the names they declare;
//! - constant violations are Babel's nodes (the AssignmentExpression,
//!   UpdateExpression, `delete` UnaryExpression, for-in/of statement,
//!   redeclaring VariableDeclarator / FunctionDeclaration) with the node's
//!   own scope, in Babel's three-group order (redeclarations, assignments,
//!   the rest).
//!
//! References are RE-RESOLVED by name through these maps (Babel's crawl
//! does exactly that), so oxc's resolution never leaks in. Iteration over
//! a scope's bindings reproduces `Object.keys(scope.bindings)` —
//! registration order — because downstream TS reads it as a decision input
//! (function-bindings.ts, context-builder.ts; porting lesson 4).

use std::collections::{BTreeMap, BTreeSet};

use oxc_ast::AstKind;
use oxc_ast::ast::{
    FunctionType, PropertyKey, PropertyKind, UnaryOperator, VariableDeclarationKind,
};
use oxc_semantic::{AstNodes, NodeId, Semantic, SymbolId};
use oxc_span::{GetSpan, Span};

/// A Babel scope, by index into [`BabelScopes::scopes`].
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct BScopeId(pub u32);

/// A Babel binding, by index into [`BabelScopes::bindings`].
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct BindingId(pub u32);

/// The Babel node type that owns a scope (`scope.block.type`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ScopeType {
    Program,
    BlockStatement,
    CatchClause,
    ForStatement,
    ForInStatement,
    ForOfStatement,
    WhileStatement,
    DoWhileStatement,
    SwitchStatement,
    StaticBlock,
    FunctionDeclaration,
    FunctionExpression,
    ArrowFunctionExpression,
    ObjectMethod,
    ClassMethod,
    ClassPrivateMethod,
    ClassDeclaration,
    ClassExpression,
    ObjectPattern,
    ArrayPattern,
    AssignmentPattern,
}

impl ScopeType {
    /// Babel's type name.
    pub fn as_str(self) -> &'static str {
        match self {
            ScopeType::Program => "Program",
            ScopeType::BlockStatement => "BlockStatement",
            ScopeType::CatchClause => "CatchClause",
            ScopeType::ForStatement => "ForStatement",
            ScopeType::ForInStatement => "ForInStatement",
            ScopeType::ForOfStatement => "ForOfStatement",
            ScopeType::WhileStatement => "WhileStatement",
            ScopeType::DoWhileStatement => "DoWhileStatement",
            ScopeType::SwitchStatement => "SwitchStatement",
            ScopeType::StaticBlock => "StaticBlock",
            ScopeType::FunctionDeclaration => "FunctionDeclaration",
            ScopeType::FunctionExpression => "FunctionExpression",
            ScopeType::ArrowFunctionExpression => "ArrowFunctionExpression",
            ScopeType::ObjectMethod => "ObjectMethod",
            ScopeType::ClassMethod => "ClassMethod",
            ScopeType::ClassPrivateMethod => "ClassPrivateMethod",
            ScopeType::ClassDeclaration => "ClassDeclaration",
            ScopeType::ClassExpression => "ClassExpression",
            ScopeType::ObjectPattern => "ObjectPattern",
            ScopeType::ArrayPattern => "ArrayPattern",
            ScopeType::AssignmentPattern => "AssignmentPattern",
        }
    }

    /// Babel's `Function` alias (`path.isFunction()`).
    pub fn is_function(self) -> bool {
        matches!(
            self,
            ScopeType::FunctionDeclaration
                | ScopeType::FunctionExpression
                | ScopeType::ArrowFunctionExpression
                | ScopeType::ObjectMethod
                | ScopeType::ClassMethod
                | ScopeType::ClassPrivateMethod
        )
    }

    /// Babel's `FunctionParent` alias (`getFunctionParent`): the functions
    /// plus `StaticBlock`.
    pub fn is_function_parent(self) -> bool {
        self.is_function() || self == ScopeType::StaticBlock
    }

    /// Babel's `BlockParent` alias (`getBlockParent`): every scope type
    /// except classes and patterns.
    pub fn is_block_parent(self) -> bool {
        !matches!(
            self,
            ScopeType::ClassDeclaration
                | ScopeType::ClassExpression
                | ScopeType::ObjectPattern
                | ScopeType::ArrayPattern
                | ScopeType::AssignmentPattern
        )
    }

    /// Babel's `Pattern` alias.
    pub fn is_pattern(self) -> bool {
        matches!(
            self,
            ScopeType::ObjectPattern | ScopeType::ArrayPattern | ScopeType::AssignmentPattern
        )
    }
}

/// One Babel scope: the owning node, its block span, its parent (Babel's
/// `scope.parent` getter).
#[derive(Clone, Debug)]
pub struct BabelScope {
    pub ty: ScopeType,
    pub node: NodeId,
    pub span: Span,
    pub parent: Option<BScopeId>,
}

/// Babel's `binding.kind`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BindingKind {
    Var,
    Let,
    Const,
    Hoisted,
    Param,
    Local,
    Module,
}

impl BindingKind {
    pub fn as_str(self) -> &'static str {
        match self {
            BindingKind::Var => "var",
            BindingKind::Let => "let",
            BindingKind::Const => "const",
            BindingKind::Hoisted => "hoisted",
            BindingKind::Param => "param",
            BindingKind::Local => "local",
            BindingKind::Module => "module",
        }
    }
}

/// The Babel node type of a reference or violation path.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SiteType {
    Identifier,
    ExportNamedDeclaration,
    ExportDefaultDeclaration,
    AssignmentExpression,
    UpdateExpression,
    UnaryExpression,
    ForInStatement,
    ForOfStatement,
    VariableDeclarator,
    FunctionDeclaration,
    ClassDeclaration,
}

impl SiteType {
    pub fn as_str(self) -> &'static str {
        match self {
            SiteType::Identifier => "Identifier",
            SiteType::ExportNamedDeclaration => "ExportNamedDeclaration",
            SiteType::ExportDefaultDeclaration => "ExportDefaultDeclaration",
            SiteType::AssignmentExpression => "AssignmentExpression",
            SiteType::UpdateExpression => "UpdateExpression",
            SiteType::UnaryExpression => "UnaryExpression",
            SiteType::ForInStatement => "ForInStatement",
            SiteType::ForOfStatement => "ForOfStatement",
            SiteType::VariableDeclarator => "VariableDeclarator",
            SiteType::FunctionDeclaration => "FunctionDeclaration",
            SiteType::ClassDeclaration => "ClassDeclaration",
        }
    }
}

/// A reference path or constant-violation path: the Babel node, its span
/// and its `path.scope`.
#[derive(Clone, Copy, Debug)]
pub struct Site {
    pub ty: SiteType,
    pub node: NodeId,
    pub span: Span,
    pub scope: BScopeId,
}

/// A Babel binding as crawled (original names; the rename state keeps the
/// current names).
#[derive(Clone, Debug)]
pub struct BabelBinding {
    /// The oxc symbol of the binding's first declaration.
    pub symbol: SymbolId,
    /// The crawl-time (original) name.
    pub name: String,
    pub kind: BindingKind,
    /// `binding.scope`.
    pub owner: BScopeId,
    /// `binding.identifier`'s span.
    pub id_span: Span,
    /// `binding.path`'s node.
    pub path_node: NodeId,
    /// `binding.referencePaths`, in Babel's order.
    pub refs: Vec<Site>,
    /// `binding.constantViolations`, in Babel's order.
    pub violations: Vec<Site>,
    /// Per violation (parallel to `violations`), the identifier spans of
    /// THIS binding it writes — `violationWriteTargetPaths` (the rename
    /// ledger's write occurrences).
    pub violation_targets: Vec<Vec<Span>>,
    /// The export declaration (`export var/let/const/function/class`,
    /// `export default function/class`) that is an ancestor-or-self of
    /// `binding.path`, when one is.
    pub export_ancestor: Option<NodeId>,
    /// Some reference path's parent is an `ExportSpecifier`.
    pub specifier_referenced: bool,
    /// `isExportDeclarationId`: `binding.path` IS an export declaration's
    /// own declaration (`export function f(){}`, `export default class C{}`).
    pub export_declaration_id: bool,
    /// Babel's renamer would SPLIT the export declaration when renaming this
    /// binding (`export const a = 1` → `const b = 1; export { b as a }`):
    /// the binding is declared by a `VariableDeclaration` directly under an
    /// export declaration.
    pub declared_in_export_var: bool,
    /// The span of `binding.path` when it is a `VariableDeclarator` WITH an
    /// initializer — the write the capture guard must see when the
    /// declaration sits inside the renamed scope (16-findings-queue #15).
    pub initialized_declarator_span: Option<Span>,
}

/// The whole Babel scope view of one program.
pub struct BabelScopes {
    pub scopes: Vec<BabelScope>,
    pub bindings: Vec<BabelBinding>,
    /// Per scope, the crawl-time `scope.bindings` map in `Object.keys`
    /// order: (name, binding).
    pub initial_maps: Vec<Vec<(String, BindingId)>>,
    /// `path.scope` per oxc node.
    node_scope: Vec<BScopeId>,
    /// oxc symbol → the Babel binding its first declaration registered.
    symbol_binding: Vec<Option<BindingId>>,
    /// The program scope's `globals` (free names).
    pub globals: BTreeSet<String>,
}

impl BabelScopes {
    /// Build the view (one pass for scopes, one for bindings, one for
    /// reference and violation paths).
    pub fn build(semantic: &Semantic<'_>) -> BabelScopes {
        let mut builder = Builder::new(semantic);
        builder.build_scopes();
        builder.register_bindings();
        builder.collect_sites();
        builder.finish()
    }

    /// `path.scope` for an oxc node.
    pub fn scope_of_node(&self, node: NodeId) -> BScopeId {
        self.node_scope[node.index()]
    }

    /// The Babel binding an oxc symbol's first declaration registered.
    pub fn binding_of_symbol(&self, symbol: SymbolId) -> Option<BindingId> {
        self.symbol_binding.get(symbol.index()).copied().flatten()
    }

    pub fn scope(&self, id: BScopeId) -> &BabelScope {
        &self.scopes[id.0 as usize]
    }

    pub fn binding(&self, id: BindingId) -> &BabelBinding {
        &self.bindings[id.0 as usize]
    }

    /// The program scope (always index 0).
    pub fn program_scope(&self) -> BScopeId {
        BScopeId(0)
    }

    /// Babel's `scope.getFunctionParent()`: the nearest scope (itself
    /// included) whose type is a FunctionParent; None at module level.
    pub fn function_parent(&self, from: BScopeId) -> Option<BScopeId> {
        let mut cur = Some(from);
        while let Some(id) = cur {
            if self.scope(id).ty.is_function_parent() {
                return Some(id);
            }
            cur = self.scope(id).parent;
        }
        None
    }
}

// ---------------------------------------------------------------------------
// The builder
// ---------------------------------------------------------------------------

/// How an oxc node relates to Babel's scopes.
enum ScopeDecision {
    /// The node owns a new Babel scope of this type.
    New(ScopeType),
    /// The node is a method's inner Function: Babel has no such node, the
    /// method's scope continues through it.
    SameAsParent,
    /// Not a scope node.
    None,
}

/// A declaration event: one declaration identifier of an oxc symbol (the
/// first, or a redeclaration), with its Babel kind and owner.
struct DeclEvent {
    symbol: SymbolId,
    name: String,
    kind: BindingKind,
    owner: BScopeId,
    id_span: Span,
    /// `binding.path`'s node for a new binding / the violation node for a
    /// merged redeclaration.
    path_node: NodeId,
    /// Registration-order key within the owner scope.
    order: u64,
    /// The class declaration's own scope, for the alias entry.
    class_alias: Option<BScopeId>,
}

struct Builder<'s, 'a> {
    semantic: &'s Semantic<'a>,
    nodes: &'s AstNodes<'a>,
    scopes: Vec<BabelScope>,
    node_scope: Vec<BScopeId>,
    /// The scope a node OWNS (New or SameAsParent), per node.
    own_scope: Vec<Option<BScopeId>>,
    bindings: Vec<BabelBinding>,
    /// Per scope: name → (order key, binding). Crawl-time.
    maps: Vec<BTreeMap<String, (u64, BindingId)>>,
    symbol_binding: Vec<Option<BindingId>>,
    /// Redeclaration violations, keyed by the violation node's id (order).
    redecl_sites: Vec<(NodeId, BindingId, Site, Span)>,
    globals: BTreeSet<String>,
}

impl<'s, 'a> Builder<'s, 'a> {
    fn new(semantic: &'s Semantic<'a>) -> Self {
        let nodes = semantic.nodes();
        Builder {
            semantic,
            nodes,
            scopes: Vec::new(),
            node_scope: vec![BScopeId(0); nodes.len()],
            own_scope: vec![None; nodes.len()],
            bindings: Vec::new(),
            maps: Vec::new(),
            symbol_binding: vec![None; semantic.scoping().symbols_len()],
            redecl_sites: Vec::new(),
            globals: BTreeSet::new(),
        }
    }

    fn parent(&self, id: NodeId) -> Option<NodeId> {
        let p = self.nodes.parent_id(id);
        (p != id).then_some(p)
    }

    // -- pass 1: scopes ------------------------------------------------------

    fn build_scopes(&mut self) {
        for node in self.nodes.iter() {
            let id = node.id();
            debug_assert!(
                self.parent(id).is_none_or(|p| p.index() < id.index()),
                "oxc node ids must be pre-order"
            );
            match self.decide(id) {
                ScopeDecision::New(ty) => {
                    let parent = self.scope_parent(id);
                    let sid = BScopeId(self.scopes.len() as u32);
                    self.scopes.push(BabelScope {
                        ty,
                        node: id,
                        span: node.span(),
                        parent,
                    });
                    self.maps.push(BTreeMap::new());
                    self.own_scope[id.index()] = Some(sid);
                    self.node_scope[id.index()] = sid;
                }
                ScopeDecision::SameAsParent => {
                    let parent = self.parent(id).expect("a method function has a parent");
                    let sid = self.node_scope[parent.index()];
                    self.own_scope[id.index()] = Some(sid);
                    self.node_scope[id.index()] = sid;
                }
                ScopeDecision::None => {
                    self.node_scope[id.index()] = self.inherited_scope(id);
                }
            }
        }
    }

    /// Babel's `isScope` over oxc node shapes.
    fn decide(&self, id: NodeId) -> ScopeDecision {
        let parent_kind = self.parent(id).map(|p| self.nodes.kind(p));
        match self.nodes.kind(id) {
            AstKind::Program(_) => ScopeDecision::New(ScopeType::Program),
            AstKind::BlockStatement(_) => {
                if matches!(parent_kind, Some(AstKind::CatchClause(_))) {
                    ScopeDecision::None
                } else {
                    ScopeDecision::New(ScopeType::BlockStatement)
                }
            }
            AstKind::CatchClause(_) => ScopeDecision::New(ScopeType::CatchClause),
            AstKind::ForStatement(_) => ScopeDecision::New(ScopeType::ForStatement),
            AstKind::ForInStatement(_) => ScopeDecision::New(ScopeType::ForInStatement),
            AstKind::ForOfStatement(_) => ScopeDecision::New(ScopeType::ForOfStatement),
            AstKind::WhileStatement(_) => ScopeDecision::New(ScopeType::WhileStatement),
            AstKind::DoWhileStatement(_) => ScopeDecision::New(ScopeType::DoWhileStatement),
            AstKind::SwitchStatement(_) => ScopeDecision::New(ScopeType::SwitchStatement),
            AstKind::StaticBlock(_) => ScopeDecision::New(ScopeType::StaticBlock),
            AstKind::ArrowFunctionExpression(_) => {
                ScopeDecision::New(ScopeType::ArrowFunctionExpression)
            }
            kind => decide_structural(kind, parent_kind),
        }
    }

    /// Babel's `Scope.parent` getter for the scope owned by `id`: climb,
    /// skipping a method when stepping out of its key or decorators.
    fn scope_parent(&self, id: NodeId) -> Option<BScopeId> {
        let mut cur = id;
        loop {
            let p = self.parent(cur)?;
            let target = if self.is_method_key_or_decorator(cur, p) {
                self.parent(p)?
            } else {
                p
            };
            if let Some(sid) = self.own_scope[target.index()] {
                return Some(sid);
            }
            cur = target;
        }
    }

    /// `setScope` for a non-scope node: the parent's scope, except a
    /// method's key/decorators and a switch's discriminant take the
    /// grandparent's.
    fn inherited_scope(&self, id: NodeId) -> BScopeId {
        let Some(p) = self.parent(id) else {
            return BScopeId(0);
        };
        let skip = self.is_method_key_or_decorator(id, p) || self.is_switch_discriminant(id, p);
        let target = if skip { self.parent(p).unwrap_or(p) } else { p };
        self.node_scope[target.index()]
    }

    /// `child` is a Babel method's `key` or one of its `decorators` — any
    /// child of the method node other than its value Function.
    fn is_method_key_or_decorator(&self, child: NodeId, parent: NodeId) -> bool {
        is_babel_method(self.nodes.kind(parent))
            && !matches!(self.nodes.kind(child), AstKind::Function(_))
    }

    fn is_switch_discriminant(&self, child: NodeId, parent: NodeId) -> bool {
        matches!(self.nodes.kind(parent), AstKind::SwitchStatement(_))
            && !matches!(self.nodes.kind(child), AstKind::SwitchCase(_))
    }

    // -- scope climbing helpers ---------------------------------------------

    fn climb_to(&self, from: BScopeId, pred: impl Fn(ScopeType) -> bool) -> BScopeId {
        let mut cur = from;
        loop {
            if pred(self.scopes[cur.0 as usize].ty) {
                return cur;
            }
            match self.scopes[cur.0 as usize].parent {
                Some(p) => cur = p,
                None => return cur,
            }
        }
    }

    fn function_parent_or_program(&self, from: BScopeId) -> BScopeId {
        self.climb_to(from, |t| t.is_function_parent() || t == ScopeType::Program)
    }

    fn block_parent(&self, from: BScopeId) -> BScopeId {
        self.climb_to(from, ScopeType::is_block_parent)
    }

    /// Block parent of the scope ABOVE a declaration's own scope (Babel's
    /// `BlockScoped` handler: `if (scope.path === path) scope = scope.parent`).
    fn block_parent_above(&self, own: BScopeId) -> BScopeId {
        let above = self.scopes[own.0 as usize].parent.unwrap_or(own);
        self.block_parent(above)
    }

    /// The nearest ancestor-or-self Function/Arrow node's scope (a
    /// parameter's owner).
    fn enclosing_function_scope(&self, id: NodeId) -> BScopeId {
        let mut cur = Some(id);
        while let Some(n) = cur {
            if matches!(
                self.nodes.kind(n),
                AstKind::Function(_) | AstKind::ArrowFunctionExpression(_)
            ) {
                return self.node_scope[n.index()];
            }
            cur = self.parent(n);
        }
        BScopeId(0)
    }

    // -- pass 2: bindings ----------------------------------------------------

    fn register_bindings(&mut self) {
        let mut events: Vec<DeclEvent> = Vec::new();
        let scoping = self.semantic.scoping();
        for symbol in scoping.symbol_ids() {
            let name = scoping.symbol_name(symbol).to_string();
            let redeclarations = scoping.symbol_redeclarations(symbol);
            if redeclarations.is_empty() {
                let span = scoping.symbol_span(symbol);
                let decl = scoping.symbol_declaration(symbol);
                events.push(self.classify(symbol, &name, span, decl));
            } else {
                for r in redeclarations {
                    events.push(self.classify(symbol, &name, r.span, r.declaration));
                }
            }
        }
        // Babel registers in traversal order: the identifier's position,
        // except a function expression's own id (after its params).
        events.sort_by_key(|e| (e.order, e.id_span.start));
        for event in events {
            self.register(event);
        }
    }

    fn register(&mut self, event: DeclEvent) {
        let owner_map = &self.maps[event.owner.0 as usize];
        if let Some(&(_, existing)) = owner_map.get(&event.name) {
            // `registerBinding` on an existing own binding: `reassign`.
            if self.bindings[existing.0 as usize].id_span != event.id_span {
                let site = Site {
                    ty: violation_type_of(self.nodes.kind(event.path_node)),
                    node: event.path_node,
                    span: self.nodes.get_node(event.path_node).span(),
                    scope: self.node_scope[event.path_node.index()],
                };
                self.redecl_sites
                    .push((event.path_node, existing, site, event.id_span));
            }
            self.link_symbol(event.symbol, existing);
            return;
        }
        let bid = BindingId(self.bindings.len() as u32);
        let export_ancestor = self.export_ancestor(event.path_node);
        self.bindings.push(BabelBinding {
            symbol: event.symbol,
            name: event.name.clone(),
            kind: event.kind,
            owner: event.owner,
            id_span: event.id_span,
            path_node: event.path_node,
            refs: Vec::new(),
            violations: Vec::new(),
            violation_targets: Vec::new(),
            export_ancestor,
            specifier_referenced: false,
            export_declaration_id: self.is_export_declaration_id(event.path_node),
            declared_in_export_var: self.is_declared_in_export_var(event.path_node),
            initialized_declarator_span: self.initialized_declarator_span(event.path_node),
        });
        if matches!(event.kind, BindingKind::Var | BindingKind::Hoisted)
            && self.is_init_in_loop(event.path_node)
        {
            // The Binding constructor's `reassign(path)`: its own path is
            // its first constant violation.
            let site = Site {
                ty: violation_type_of(self.nodes.kind(event.path_node)),
                node: event.path_node,
                span: self.nodes.get_node(event.path_node).span(),
                scope: self.node_scope[event.path_node.index()],
            };
            self.redecl_sites
                .push((event.path_node, bid, site, event.id_span));
        }
        self.maps[event.owner.0 as usize].insert(event.name.clone(), (event.order, bid));
        if let Some(alias) = event.class_alias {
            self.maps[alias.0 as usize].insert(event.name, (event.order, bid));
        }
        self.link_symbol(event.symbol, bid);
    }

    fn link_symbol(&mut self, symbol: SymbolId, binding: BindingId) {
        let slot = &mut self.symbol_binding[symbol.index()];
        if slot.is_none() {
            *slot = Some(binding);
        }
    }

    /// Babel's kind + owner for one declaration identifier.
    fn classify(&self, symbol: SymbolId, name: &str, span: Span, decl: NodeId) -> DeclEvent {
        let mut event = DeclEvent {
            symbol,
            name: name.to_string(),
            kind: BindingKind::Var,
            owner: BScopeId(0),
            id_span: span,
            path_node: decl,
            order: registration_order(self.nodes.kind(decl), name, span),
            class_alias: None,
        };
        match self.nodes.kind(decl) {
            AstKind::VariableDeclarator(_) => self.classify_declarator(&mut event, decl),
            AstKind::Function(f) => {
                let own = self.node_scope[decl.index()];
                if f.r#type == FunctionType::FunctionExpression {
                    event.kind = BindingKind::Local;
                    event.owner = own;
                } else {
                    event.kind = BindingKind::Hoisted;
                    event.owner = self.block_parent_above(own);
                }
            }
            AstKind::Class(c) => self.classify_class(&mut event, decl, c.is_expression()),
            AstKind::CatchParameter(_) => {
                event.kind = BindingKind::Let;
                event.owner = self.node_scope[decl.index()];
                event.path_node = self.parent(decl).unwrap_or(decl);
            }
            AstKind::FormalParameter(_)
            | AstKind::FormalParameterRest(_)
            | AstKind::FormalParameters(_)
            | AstKind::BindingRestElement(_) => {
                event.kind = BindingKind::Param;
                event.owner = self.enclosing_function_scope(decl);
            }
            AstKind::ImportSpecifier(_)
            | AstKind::ImportDefaultSpecifier(_)
            | AstKind::ImportNamespaceSpecifier(_) => {
                event.kind = BindingKind::Module;
                event.owner = self.block_parent(self.node_scope[decl.index()]);
            }
            other => panic!(
                "validated-rename scope view: unhandled declaration kind {} for `{name}` at {span:?}",
                other.debug_name()
            ),
        }
        event
    }

    fn classify_class(&self, event: &mut DeclEvent, decl: NodeId, expression: bool) {
        let own = self.node_scope[decl.index()];
        if expression {
            event.kind = BindingKind::Local;
            event.owner = own;
        } else {
            event.kind = BindingKind::Let;
            event.owner = self.block_parent_above(own);
            event.class_alias = Some(own);
        }
    }

    fn classify_declarator(&self, event: &mut DeclEvent, decl: NodeId) {
        let declaration = self.parent(decl).expect("a declarator has a declaration");
        let AstKind::VariableDeclaration(vd) = self.nodes.kind(declaration) else {
            unreachable!("a declarator's parent is a VariableDeclaration")
        };
        let scope = self.node_scope[decl.index()];
        match vd.kind {
            VariableDeclarationKind::Var => {
                event.kind = BindingKind::Var;
                event.owner = self.function_parent_or_program(scope);
            }
            kind => {
                // Babel registers `using` / `await using` as "const"
                // (probed on 2.1.197/2.1.215, whose bundles carry them).
                event.kind = if kind == VariableDeclarationKind::Let {
                    BindingKind::Let
                } else {
                    BindingKind::Const
                };
                event.owner = self.block_parent(scope);
            }
        }
    }

    /// Babel's `isInitInLoop(binding.path)` (binding.js): below the nearest
    /// FunctionParent, the path sits in a for-in/of `left`, or — when it is
    /// a function declaration or an initialized declarator — in any loop's
    /// `body`. Such a `var`/`hoisted` binding starts with its own path as a
    /// constant violation (`var $ = H[_]` inside a `while`).
    fn is_init_in_loop(&self, path_node: NodeId) -> bool {
        let has_init = match self.nodes.kind(path_node) {
            AstKind::VariableDeclarator(d) => d.init.is_some(),
            _ => true,
        };
        let mut child = path_node;
        while let Some(parent) = self.parent(child) {
            let child_span = self.nodes.get_node(child).span();
            match self.nodes.kind(parent) {
                AstKind::Function(_)
                | AstKind::ArrowFunctionExpression(_)
                | AstKind::StaticBlock(_) => return false,
                AstKind::ForInStatement(f) => {
                    if f.left.span() == child_span || (has_init && f.body.span() == child_span) {
                        return true;
                    }
                }
                AstKind::ForOfStatement(f) => {
                    if f.left.span() == child_span || (has_init && f.body.span() == child_span) {
                        return true;
                    }
                }
                AstKind::ForStatement(f) if has_init && f.body.span() == child_span => return true,
                AstKind::WhileStatement(w) if has_init && w.body.span() == child_span => {
                    return true;
                }
                AstKind::DoWhileStatement(w) if has_init && w.body.span() == child_span => {
                    return true;
                }
                _ => {}
            }
            child = parent;
        }
        false
    }

    /// `binding.path.find(p => p.isExportDeclaration())`: an export
    /// declaration among the path node's ancestors-or-self.
    fn export_ancestor(&self, path_node: NodeId) -> Option<NodeId> {
        let mut cur = Some(path_node);
        while let Some(n) = cur {
            if is_export_declaration(self.nodes.kind(n)) {
                return Some(n);
            }
            cur = self.parent(n);
        }
        None
    }

    /// `isExportDeclarationId`: the binding path IS the declaration of an
    /// export (named or default) — a function or class node directly under it.
    fn is_export_declaration_id(&self, path_node: NodeId) -> bool {
        if !matches!(
            self.nodes.kind(path_node),
            AstKind::Function(_) | AstKind::Class(_)
        ) {
            return false;
        }
        self.parent(path_node).is_some_and(|p| {
            matches!(
                self.nodes.kind(p),
                AstKind::ExportDeclaration(_) | AstKind::ExportDefaultDeclaration(_)
            )
        })
    }

    /// The Renamer's `maybeConvertFromExportDeclaration` fires: the binding
    /// is declared by a VariableDeclaration directly under `export`.
    /// `binding.path` is a `VariableDeclarator` with an initializer: its
    /// span (else None).
    fn initialized_declarator_span(&self, path_node: NodeId) -> Option<Span> {
        match self.nodes.kind(path_node) {
            AstKind::VariableDeclarator(d) if d.init.is_some() => Some(d.span),
            _ => None,
        }
    }

    fn is_declared_in_export_var(&self, path_node: NodeId) -> bool {
        if !matches!(self.nodes.kind(path_node), AstKind::VariableDeclarator(_)) {
            return false;
        }
        let Some(declaration) = self.parent(path_node) else {
            return false;
        };
        self.parent(declaration)
            .is_some_and(|p| matches!(self.nodes.kind(p), AstKind::ExportDeclaration(_)))
    }

    // -- resolution ----------------------------------------------------------

    /// Babel's `scope.getBinding(name)` over the crawl-time maps.
    fn resolve(&self, name: &str, from: BScopeId) -> Option<BindingId> {
        resolve_in(
            &self.scopes,
            name,
            from,
            |sid, n| self.maps[sid.0 as usize].get(n).map(|&(_, b)| b),
            |b| self.bindings[b.0 as usize].kind,
        )
    }

    // -- pass 3: reference and violation paths --------------------------------

    fn collect_sites(&mut self) {
        let mut export_refs: Vec<(BindingId, Site)> = Vec::new();
        let mut id_refs: Vec<(BindingId, Site)> = Vec::new();
        let mut assignment_sites: Vec<Violation> = Vec::new();
        let mut other_sites: Vec<Violation> = Vec::new();
        for node in self.nodes.iter() {
            let id = node.id();
            match node.kind() {
                AstKind::IdentifierReference(ir) => {
                    let sink = SiteSinks {
                        refs: &mut id_refs,
                        assignments: &mut assignment_sites,
                        others: &mut other_sites,
                    };
                    self.collect_identifier(id, ir.name.as_str(), ir.span, sink);
                }
                AstKind::ExportDeclaration(_) | AstKind::ExportDefaultDeclaration(_) => {
                    self.collect_export_refs(id, &mut export_refs);
                }
                _ => {}
            }
        }
        self.attach(export_refs, id_refs, assignment_sites, other_sites);
    }

    fn collect_identifier(&mut self, id: NodeId, name: &str, span: Span, sink: SiteSinks<'_>) {
        let scope = self.node_scope[id.index()];
        let write = self.is_write_reference(id);
        let owner = if write {
            babel_write_site(self.nodes, id)
        } else {
            delete_site(self.nodes, id).map(|n| (WriteSite::Delete, n))
        };
        let is_ref = !matches!(
            owner,
            Some((WriteSite::Assignment | WriteSite::ForXPattern, _))
        );
        if is_ref {
            match self.resolve(name, scope) {
                Some(b) => sink.refs.push((b, self.identifier_site(id, span, scope))),
                None => {
                    self.globals.insert(name.to_string());
                }
            }
        }
        let Some((site, node)) = owner else {
            return;
        };
        let (ty, vscope) = self.violation_shape(site, node);
        let Some(b) = self.resolve(name, vscope) else {
            if site == WriteSite::Assignment {
                self.globals.insert(name.to_string());
            }
            return;
        };
        let entry = (
            b,
            Site {
                ty,
                node,
                span: self.nodes.get_node(node).span(),
                scope: vscope,
            },
            span,
        );
        if site == WriteSite::Assignment {
            sink.assignments.push(entry);
        } else {
            sink.others.push(entry);
        }
    }

    fn is_write_reference(&self, id: NodeId) -> bool {
        let AstKind::IdentifierReference(ir) = self.nodes.kind(id) else {
            return false;
        };
        ir.reference_id
            .get()
            .is_some_and(|r| self.semantic.scoping().get_reference(r).flags().is_write())
    }

    fn identifier_site(&mut self, id: NodeId, span: Span, scope: BScopeId) -> Site {
        Site {
            ty: SiteType::Identifier,
            node: id,
            span,
            scope,
        }
    }

    /// The violation node's Babel type and `path.scope` (a for-in/of
    /// statement is its own scope; the others inherit).
    fn violation_shape(&self, site: WriteSite, node: NodeId) -> (SiteType, BScopeId) {
        let ty = match site {
            WriteSite::Assignment => SiteType::AssignmentExpression,
            WriteSite::Update => SiteType::UpdateExpression,
            WriteSite::Delete => SiteType::UnaryExpression,
            WriteSite::ForXSimple | WriteSite::ForXPattern => match self.nodes.kind(node) {
                AstKind::ForInStatement(_) => SiteType::ForInStatement,
                _ => SiteType::ForOfStatement,
            },
        };
        (ty, self.node_scope[node.index()])
    }

    /// Babel's `ExportDeclaration` exit handler: the export declaration is
    /// a reference path of every name it declares.
    fn collect_export_refs(&self, id: NodeId, out: &mut Vec<(BindingId, Site)>) {
        let (ty, names) = match self.nodes.kind(id) {
            AstKind::ExportDeclaration(e) => (
                SiteType::ExportNamedDeclaration,
                declared_names_of_declaration(&e.declaration),
            ),
            AstKind::ExportDefaultDeclaration(e) => (
                SiteType::ExportDefaultDeclaration,
                default_declared_name(&e.declaration).into_iter().collect(),
            ),
            _ => return,
        };
        let scope = self.node_scope[id.index()];
        for name in names {
            if let Some(b) = self.resolve(&name, scope) {
                out.push((
                    b,
                    Site {
                        ty,
                        node: id,
                        span: self.nodes.get_node(id).span(),
                        scope,
                    },
                ));
            }
        }
    }

    fn attach(
        &mut self,
        export_refs: Vec<(BindingId, Site)>,
        id_refs: Vec<(BindingId, Site)>,
        mut assignments: Vec<Violation>,
        mut others: Vec<Violation>,
    ) {
        // Babel pushes a violation path at the node's ENTER: traversal
        // (pre-order = node id) order of the violation nodes, not of the
        // identifiers they write (`[x = (a = 2), a] = y`).
        assignments.sort_by_key(|(_, site, _)| site.node.index());
        others.sort_by_key(|(_, site, _)| site.node.index());
        for (b, site) in export_refs.into_iter().chain(id_refs) {
            if site.ty == SiteType::Identifier && self.parent_is_export_specifier(site.node) {
                self.bindings[b.0 as usize].specifier_referenced = true;
            }
            self.bindings[b.0 as usize].refs.push(site);
        }
        let mut redecls = std::mem::take(&mut self.redecl_sites);
        redecls.sort_by_key(|(node, _, _, _)| node.index());
        let redecls: Vec<Violation> = redecls.into_iter().map(|(_, b, s, t)| (b, s, t)).collect();
        // One violation per (binding, node) — Babel keys
        // `getBindingIdentifiers` by name — carrying every target it writes.
        for group in [redecls, assignments, others] {
            for (b, site, targets) in merge_sites(group) {
                let binding = &mut self.bindings[b.0 as usize];
                binding.violations.push(site);
                binding.violation_targets.push(targets);
            }
        }
    }

    fn parent_is_export_specifier(&self, id: NodeId) -> bool {
        self.parent(id)
            .is_some_and(|p| matches!(self.nodes.kind(p), AstKind::ExportSpecifier(_)))
    }

    fn finish(self) -> BabelScopes {
        let initial_maps = self
            .maps
            .iter()
            .map(|m| {
                let mut entries: Vec<(u64, String, BindingId)> =
                    m.iter().map(|(n, &(o, b))| (o, n.clone(), b)).collect();
                entries.sort();
                entries.into_iter().map(|(_, n, b)| (n, b)).collect()
            })
            .collect();
        BabelScopes {
            scopes: self.scopes,
            bindings: self.bindings,
            initial_maps,
            node_scope: self.node_scope,
            symbol_binding: self.symbol_binding,
            globals: self.globals,
        }
    }
}

/// A constant violation of a binding, with the identifier it writes.
type Violation = (BindingId, Site, Span);

/// The three sink lists a reference identifier can feed.
struct SiteSinks<'v> {
    refs: &'v mut Vec<(BindingId, Site)>,
    assignments: &'v mut Vec<Violation>,
    others: &'v mut Vec<Violation>,
}

/// One violation per (binding, node), first-seen order, each carrying
/// every identifier target it writes (deduplicated).
fn merge_sites(sites: Vec<Violation>) -> Vec<(BindingId, Site, Vec<Span>)> {
    let mut out: Vec<(BindingId, Site, Vec<Span>)> = Vec::new();
    let mut index: BTreeMap<(BindingId, usize), usize> = BTreeMap::new();
    for (b, site, target) in sites {
        let key = (b, site.node.index());
        match index.get(&key) {
            Some(&i) => {
                if !out[i].2.contains(&target) {
                    out[i].2.push(target);
                }
            }
            None => {
                index.insert(key, out.len());
                out.push((b, site, vec![target]));
            }
        }
    }
    out
}

/// `getBinding` over any map source — shared by the crawl (original names)
/// and the rename state (current names). The pattern rule: a lookup that
/// climbed out of a pattern scope sees only `param`/`local` bindings of the
/// next scope; `arguments` never resolves past a non-arrow function.
pub(crate) fn resolve_in(
    scopes: &[BabelScope],
    name: &str,
    from: BScopeId,
    lookup: impl Fn(BScopeId, &str) -> Option<BindingId>,
    kind_of: impl Fn(BindingId) -> BindingKind,
) -> Option<BindingId> {
    let mut cur = Some(from);
    let mut previous_was_pattern = false;
    while let Some(sid) = cur {
        let scope = &scopes[sid.0 as usize];
        match lookup(sid, name) {
            Some(b) => {
                let kind = kind_of(b);
                if !(previous_was_pattern
                    && kind != BindingKind::Param
                    && kind != BindingKind::Local)
                {
                    return Some(b);
                }
            }
            None => {
                if name == "arguments"
                    && scope.ty.is_function()
                    && scope.ty != ScopeType::ArrowFunctionExpression
                {
                    return None;
                }
            }
        }
        previous_was_pattern = scope.ty.is_pattern();
        cur = scope.parent;
    }
    None
}

// ---------------------------------------------------------------------------
// Node-shape helpers
// ---------------------------------------------------------------------------

/// The structural scope decisions (functions, methods, classes, patterns).
fn decide_structural(kind: AstKind<'_>, parent_kind: Option<AstKind<'_>>) -> ScopeDecision {
    match kind {
        AstKind::Function(f) => {
            if parent_kind.is_some_and(is_babel_method) {
                ScopeDecision::SameAsParent
            } else if f.r#type == FunctionType::FunctionExpression {
                ScopeDecision::New(ScopeType::FunctionExpression)
            } else {
                ScopeDecision::New(ScopeType::FunctionDeclaration)
            }
        }
        AstKind::MethodDefinition(m) => {
            if matches!(m.key, PropertyKey::PrivateIdentifier(_)) {
                ScopeDecision::New(ScopeType::ClassPrivateMethod)
            } else {
                ScopeDecision::New(ScopeType::ClassMethod)
            }
        }
        k @ AstKind::ObjectProperty(_) if is_babel_method(k) => {
            ScopeDecision::New(ScopeType::ObjectMethod)
        }
        AstKind::Class(c) => {
            if c.is_expression() {
                ScopeDecision::New(ScopeType::ClassExpression)
            } else {
                ScopeDecision::New(ScopeType::ClassDeclaration)
            }
        }
        AstKind::FormalParameter(p) if p.initializer.is_some() => {
            ScopeDecision::New(ScopeType::AssignmentPattern)
        }
        AstKind::ObjectPattern(_) if is_pattern_param_parent(parent_kind) => {
            ScopeDecision::New(ScopeType::ObjectPattern)
        }
        AstKind::ArrayPattern(_) if is_pattern_param_parent(parent_kind) => {
            ScopeDecision::New(ScopeType::ArrayPattern)
        }
        _ => ScopeDecision::None,
    }
}

/// A pattern is a Babel scope when it is a function parameter (without a
/// default — that wraps it in an AssignmentPattern) or the catch parameter.
fn is_pattern_param_parent(parent_kind: Option<AstKind<'_>>) -> bool {
    match parent_kind {
        Some(AstKind::FormalParameter(p)) => p.initializer.is_none(),
        Some(AstKind::CatchParameter(_)) => true,
        _ => false,
    }
}

/// Babel's `Method` alias over oxc: a class method (MethodDefinition) or an
/// object method / getter / setter (ObjectProperty with method or a
/// get/set kind).
fn is_babel_method(kind: AstKind<'_>) -> bool {
    match kind {
        AstKind::MethodDefinition(_) => true,
        AstKind::ObjectProperty(p) => p.method || p.kind != PropertyKind::Init,
        _ => false,
    }
}

fn is_export_declaration(kind: AstKind<'_>) -> bool {
    matches!(
        kind,
        AstKind::ExportDeclaration(_)
            | AstKind::ExportNamedDeclaration(_)
            | AstKind::ExportDefaultDeclaration(_)
            | AstKind::ExportAllDeclaration(_)
            | AstKind::ExportFromDeclaration(_)
    )
}

/// The Babel violation type of a redeclaring declaration node.
fn violation_type_of(kind: AstKind<'_>) -> SiteType {
    match kind {
        AstKind::Function(_) => SiteType::FunctionDeclaration,
        AstKind::Class(_) => SiteType::ClassDeclaration,
        _ => SiteType::VariableDeclarator,
    }
}

/// Which Babel constant violation owns a write position.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum WriteSite {
    /// Inside an AssignmentExpression's left (simple, compound or
    /// destructuring): a violation, NOT a reference path.
    Assignment,
    /// The whole left of a for-in/of (`for (a in o)`): a reference path AND
    /// a violation.
    ForXSimple,
    /// Inside a destructuring for-in/of left: a violation only.
    ForXPattern,
    /// `a++` / `--a`: a reference path AND a violation.
    Update,
    /// `delete a`: a reference path AND a violation.
    Delete,
}

/// The Babel violation node for a WRITE reference: walk up to the first
/// assignment / update / for-in-of that owns the position, stopping at
/// statement and function boundaries. One owner of this question for the
/// validated-rename scope view (graph.rs's `is_babel_assignment_target`
/// answers the narrower "under an assignment's left" for the WP1.4 edges).
pub(crate) fn babel_write_site(nodes: &AstNodes<'_>, id: NodeId) -> Option<(WriteSite, NodeId)> {
    let span = nodes.get_node(id).span();
    let mut child = id;
    let mut cur = nodes.parent_id(id);
    while cur != child {
        match nodes.kind(cur) {
            AstKind::AssignmentExpression(a) => {
                return a
                    .left
                    .span()
                    .contains_inclusive(span)
                    .then_some((WriteSite::Assignment, cur));
            }
            AstKind::UpdateExpression(_) => return Some((WriteSite::Update, cur)),
            AstKind::ForInStatement(f) => return forx_site(f.left.span(), span, child, id, cur),
            AstKind::ForOfStatement(f) => return forx_site(f.left.span(), span, child, id, cur),
            kind if kind.is_statement()
                || matches!(
                    kind,
                    AstKind::Function(_) | AstKind::ArrowFunctionExpression(_)
                ) =>
            {
                return None;
            }
            _ => {}
        }
        child = cur;
        cur = nodes.parent_id(cur);
    }
    None
}

fn forx_site(
    left: Span,
    span: Span,
    child: NodeId,
    id: NodeId,
    forx: NodeId,
) -> Option<(WriteSite, NodeId)> {
    if !left.contains_inclusive(span) {
        return None;
    }
    let site = if child == id {
        WriteSite::ForXSimple
    } else {
        WriteSite::ForXPattern
    };
    Some((site, forx))
}

/// `delete a` (parens skipped): the UnaryExpression node.
fn delete_site(nodes: &AstNodes<'_>, id: NodeId) -> Option<NodeId> {
    let mut cur = nodes.parent_id(id);
    while matches!(nodes.kind(cur), AstKind::ParenthesizedExpression(_)) {
        cur = nodes.parent_id(cur);
    }
    match nodes.kind(cur) {
        AstKind::UnaryExpression(u) if u.operator == UnaryOperator::Delete => Some(cur),
        _ => None,
    }
}

/// Every binding name a declaration introduces (`getBindingIdentifiers`).
fn declared_names_of_declaration(decl: &oxc_ast::ast::Declaration<'_>) -> Vec<String> {
    use oxc_ast::ast::Declaration;
    match decl {
        Declaration::VariableDeclaration(vd) => vd
            .declarations
            .iter()
            .flat_map(|d| binding_pattern_names(&d.id))
            .collect(),
        Declaration::FunctionDeclaration(f) => f.id.iter().map(|id| id.name.to_string()).collect(),
        Declaration::ClassDeclaration(c) => c.id.iter().map(|id| id.name.to_string()).collect(),
        _ => Vec::new(),
    }
}

/// `export default function f(){}` / `class C{}`: the declared name.
fn default_declared_name(kind: &oxc_ast::ast::ExportDefaultDeclarationKind<'_>) -> Option<String> {
    use oxc_ast::ast::ExportDefaultDeclarationKind as K;
    match kind {
        K::FunctionDeclaration(f) => f.id.as_ref().map(|id| id.name.to_string()),
        K::ClassDeclaration(c) => c.id.as_ref().map(|id| id.name.to_string()),
        _ => None,
    }
}

/// Babel's registration position for one declaration identifier: the
/// registering unit's position (a declarator, a parameter, the catch
/// clause's param; a function expression's own id registers at Function
/// enter AFTER the params), then the name's rank in the unit's
/// `getOuterBindingIdentifiers` — a BREADTH-first walk, so
/// `var { e, f: [g], ...h }` registers e, h, g.
fn registration_order(kind: AstKind<'_>, name: &str, span: Span) -> u64 {
    let (unit, pattern) = match kind {
        AstKind::VariableDeclarator(d) => (d.span.start, Some(&d.id)),
        AstKind::FormalParameter(p) => (p.span.start, Some(&p.pattern)),
        AstKind::FormalParameterRest(r) => (r.span.start, Some(&r.rest.argument)),
        AstKind::CatchParameter(c) => (c.span.start, Some(&c.pattern)),
        AstKind::Function(f) if f.r#type == FunctionType::FunctionExpression => {
            (f.params.span.end, None)
        }
        _ => (span.start, None),
    };
    let rank = pattern
        .and_then(|p| bfs_binding_names(p).iter().position(|n| n == name))
        .unwrap_or(0);
    (u64::from(unit) << 32) | rank as u64
}

/// Babel's `getBindingIdentifiers` walk order (a queue, breadth-first):
/// ObjectPattern → properties (rest last) → values; ArrayPattern →
/// elements (rest last); AssignmentPattern → left; RestElement → argument.
fn bfs_binding_names(pattern: &oxc_ast::ast::BindingPattern<'_>) -> Vec<String> {
    use oxc_ast::ast::BindingPattern as P;
    let mut out = Vec::new();
    // A property / rest wrapper is one extra queue level in Babel; model
    // it by queueing the wrapped pattern behind a level marker.
    let mut wrapped: std::collections::VecDeque<(bool, &oxc_ast::ast::BindingPattern<'_>)> =
        std::collections::VecDeque::new();
    wrapped.push_back((false, pattern));
    while let Some((is_wrapper, p)) = wrapped.pop_front() {
        if is_wrapper {
            wrapped.push_back((false, p));
            continue;
        }
        match p {
            P::BindingIdentifier(id) => out.push(id.name.to_string()),
            P::ObjectPattern(o) => {
                for prop in &o.properties {
                    wrapped.push_back((true, &prop.value));
                }
                if let Some(rest) = &o.rest {
                    wrapped.push_back((true, &rest.argument));
                }
            }
            P::ArrayPattern(a) => {
                for element in a.elements.iter().flatten() {
                    wrapped.push_back((false, element));
                }
                if let Some(rest) = &a.rest {
                    wrapped.push_back((true, &rest.argument));
                }
            }
            P::AssignmentPattern(a) => wrapped.push_back((false, &a.left)),
        }
    }
    out
}

/// The names a binding pattern binds, in source order.
fn binding_pattern_names(pattern: &oxc_ast::ast::BindingPattern<'_>) -> Vec<String> {
    pattern
        .get_binding_identifiers()
        .iter()
        .map(|id| id.name.to_string())
        .collect()
}

#[cfg(test)]
mod scopes_test;
