//! Load-time dependency model for a module's top-level statements (exp038)
//! — TS `src/split/load-order.ts`.
//!
//! What each statement READS, WRITES and whether it has an observable
//! EFFECT while the module loads. Function/arrow BODIES are excluded (they
//! run later), which is the whole unlock: `var x = lazy(() => {...})`
//! touches nothing at load time. Soundness rests on three rules: effect
//! statements are barriers, hoisted function declarations are
//! unconstrained, and every edge points forward in bundle order (so the
//! bundle order is always a legal answer).
//!
//! The walk is Babel's, over oxc's typed AST: the TS dispatches on Babel
//! node types (`t.isIdentifier`, `t.isFunction`, …) and falls back to
//! `VISITOR_KEYS` for everything else, so every Babel Identifier the walk
//! can reach is a READ — labels, catch parameters, a nested declarator's
//! id, destructuring targets, a `#private` name's id, `import.meta`'s two
//! halves. The oxc shapes Babel does not have are translated in place:
//! `ParenthesizedExpression`/`ChainExpression` are transparent,
//! `ImportExpression` is a call on `Import`, `PrivateInExpression` a
//! BinaryExpression whose left is a PrivateName, `ImportMeta`/`NewTarget`
//! a MetaProperty of two Identifiers, an object `Property` with
//! `method`/`get`/`set` an ObjectMethod (a Function: deferred), and an
//! assignment target's shorthand property an ObjectProperty whose value is
//! the Identifier (or an AssignmentPattern around it).
//!
//! Recursive — expressions at load time nest as deep as the source
//! does; the gate verb runs on a large-stack thread.

use std::collections::HashSet;

use oxc_ast::ast::{
    Argument, ArrayExpressionElement, AssignmentOperator, AssignmentTarget,
    AssignmentTargetMaybeDefault, AssignmentTargetProperty, BindingPattern, CallExpression,
    ChainElement, Class, ClassElement, Expression, ForStatementInit, ForStatementLeft,
    MemberExpression, ObjectPropertyKind, PropertyKey, PropertyKind, SimpleAssignmentTarget,
    Statement, UnaryOperator, VariableDeclaration, VariableDeclarationKind, VariableDeclarator,
};

use crate::babel_view::unparen;

use super::bun_helpers::identify_bun_lazy_init;

/// What one top-level statement does while the module is loading.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LoadOrderFacts {
    /// A function declaration: initialized before any statement runs.
    pub hoisted: bool,
    /// Module bindings it assigns at load time (insertion order).
    pub writes: Vec<String>,
    /// Bindings it reads at load time (never a function/arrow body).
    pub reads: Vec<String>,
    /// It can observably do something at load time: an order barrier.
    pub effects: bool,
}

/// `LoadOrderOptions`: callee names the caller verified STRUCTURALLY.
#[derive(Clone, Debug, Default)]
pub struct LoadOrderOptions {
    /// Calls with no observable load-time effect (Bun's lazy-init wrapper).
    pub pure_call_names: HashSet<String>,
    /// Calls that only WRITE their first argument (the export registrar).
    pub target_writing_call_names: HashSet<String>,
}

/// An insertion-ordered set of names (the TS `Set<string>`).
#[derive(Default)]
struct NameSet {
    order: Vec<String>,
    seen: HashSet<String>,
}

impl NameSet {
    fn add(&mut self, name: &str) {
        if self.seen.insert(name.to_string()) {
            self.order.push(name.to_string());
        }
    }
}

struct Ctx<'o> {
    reads: NameSet,
    writes: NameSet,
    effects: bool,
    opts: &'o LoadOrderOptions,
}

/// Terminal callee name of `f(…)`, `ns.f(…)` or `(0, ns.f)(…)` (Babel
/// shapes: parens dropped, an optional member is still a member).
fn callee_name<'a>(callee: &'a Expression<'a>) -> Option<&'a str> {
    match unparen(callee) {
        Expression::Identifier(id) => Some(id.name.as_str()),
        Expression::StaticMemberExpression(m) => Some(m.property.name.as_str()),
        Expression::ChainExpression(c) => match &c.expression {
            ChainElement::StaticMemberExpression(m) => Some(m.property.name.as_str()),
            _ => None,
        },
        Expression::SequenceExpression(seq) => seq.expressions.last().and_then(callee_name),
        _ => None,
    }
}

impl<'o> Ctx<'o> {
    fn new(opts: &'o LoadOrderOptions) -> Self {
        Ctx {
            reads: NameSet::default(),
            writes: NameSet::default(),
            effects: false,
            opts,
        }
    }

    fn read(&mut self, name: &str) {
        self.reads.add(name);
    }

    // -- expressions ----------------------------------------------------------

    fn expr(&mut self, e: &Expression<'_>) {
        match e {
            Expression::Identifier(id) => self.read(&id.name),
            Expression::ParenthesizedExpression(p) => self.expr(&p.expression),
            Expression::ChainExpression(c) => match &c.expression {
                ChainElement::CallExpression(call) => self.call(call),
                ChainElement::TSNonNullExpression(t) => self.expr(&t.expression),
                other => self.member(other.to_member_expression()),
            },
            Expression::StaticMemberExpression(_)
            | Expression::ComputedMemberExpression(_)
            | Expression::PrivateFieldExpression(_) => {
                self.member(e.to_member_expression());
            }
            // Deferred: closure creation only.
            Expression::FunctionExpression(_) | Expression::ArrowFunctionExpression(_) => {}
            Expression::ClassExpression(class) => self.class(class),
            Expression::CallExpression(call) => self.call(call),
            Expression::NewExpression(new) => {
                // `new` is never admitted as pure.
                self.effects = true;
                self.expr(&new.callee);
                self.args(&new.arguments);
            }
            Expression::ImportExpression(import) => {
                // Babel: CallExpression{callee: Import} — no callee name.
                self.effects = true;
                self.expr(&import.source);
                if let Some(options) = &import.options {
                    self.expr(options);
                }
            }
            Expression::AssignmentExpression(a) => self.assignment(a),
            Expression::UpdateExpression(u) => self.update(u),
            Expression::TaggedTemplateExpression(t) => {
                self.effects = true;
                self.expr(&t.tag);
                for q in &t.quasi.expressions {
                    self.expr(q);
                }
            }
            Expression::AwaitExpression(a) => {
                self.effects = true;
                self.expr(&a.argument);
            }
            Expression::YieldExpression(y) => {
                self.effects = true;
                if let Some(arg) = &y.argument {
                    self.expr(arg);
                }
            }
            Expression::UnaryExpression(u) => {
                if u.operator == UnaryOperator::Delete {
                    self.effects = true;
                }
                self.expr(&u.argument);
            }
            Expression::BinaryExpression(b) => {
                self.expr(&b.left);
                self.expr(&b.right);
            }
            Expression::PrivateInExpression(p) => {
                // BinaryExpression{left: PrivateName{id: Identifier}}.
                self.read(&p.left.name);
                self.expr(&p.right);
            }
            Expression::LogicalExpression(l) => {
                self.expr(&l.left);
                self.expr(&l.right);
            }
            Expression::ConditionalExpression(c) => {
                self.expr(&c.test);
                self.expr(&c.consequent);
                self.expr(&c.alternate);
            }
            Expression::SequenceExpression(s) => {
                for x in &s.expressions {
                    self.expr(x);
                }
            }
            Expression::ArrayExpression(a) => self.array(a),
            Expression::ObjectExpression(o) => self.object(o),
            Expression::TemplateLiteral(t) => {
                for x in &t.expressions {
                    self.expr(x);
                }
            }
            Expression::ImportMeta(_) => {
                self.read("import");
                self.read("meta");
            }
            Expression::NewTarget(_) => {
                self.read("new");
                self.read("target");
            }
            Expression::TSAsExpression(t) => self.expr(&t.expression),
            Expression::TSSatisfiesExpression(t) => self.expr(&t.expression),
            Expression::TSTypeAssertion(t) => self.expr(&t.expression),
            Expression::TSNonNullExpression(t) => self.expr(&t.expression),
            Expression::TSInstantiationExpression(t) => self.expr(&t.expression),
            // Literals, this, super, JSX, V8 intrinsics: nothing to read.
            _ => {}
        }
    }

    /// `walkAssignment`.
    fn assignment(&mut self, a: &oxc_ast::ast::AssignmentExpression<'_>) {
        match &a.left {
            AssignmentTarget::AssignmentTargetIdentifier(id) => {
                self.writes.add(&id.name);
                if a.operator != AssignmentOperator::Assign {
                    self.read(&id.name);
                }
            }
            target => {
                self.effects = true; // writing through a member or pattern
                self.assign_target(target);
            }
        }
        self.expr(&a.right);
    }

    /// `walkUpdate`.
    fn update(&mut self, u: &oxc_ast::ast::UpdateExpression<'_>) {
        match &u.argument {
            SimpleAssignmentTarget::AssignmentTargetIdentifier(id) => {
                self.read(&id.name);
                self.writes.add(&id.name);
            }
            target => {
                self.effects = true;
                self.simple_target(target);
            }
        }
    }

    fn array(&mut self, a: &oxc_ast::ast::ArrayExpression<'_>) {
        for el in &a.elements {
            match el {
                ArrayExpressionElement::SpreadElement(s) => {
                    self.effects = true;
                    self.expr(&s.argument);
                }
                ArrayExpressionElement::Elision(_) => {}
                other => self.expr(other.to_expression()),
            }
        }
    }

    /// ObjectProperty (walkReference: computed key + value), ObjectMethod
    /// (deferred: computed key only), SpreadElement (an effect).
    fn object(&mut self, o: &oxc_ast::ast::ObjectExpression<'_>) {
        for prop in &o.properties {
            match prop {
                ObjectPropertyKind::ObjectProperty(p) => {
                    let is_method = p.method || p.kind != PropertyKind::Init;
                    if p.computed {
                        self.key(&p.key);
                    }
                    if !is_method {
                        self.expr(&p.value);
                    }
                }
                ObjectPropertyKind::SpreadProperty(s) => {
                    self.effects = true;
                    self.expr(&s.argument);
                }
            }
        }
    }

    /// A computed key's expression (non-computed keys are never walked).
    fn key(&mut self, key: &PropertyKey<'_>) {
        if let Some(e) = key.as_expression() {
            self.expr(e);
        }
    }

    /// `walkMember`: the base binding is read; the read itself is
    /// effect-free.
    fn member(&mut self, m: &MemberExpression<'_>) {
        match m {
            MemberExpression::ComputedMemberExpression(c) => {
                self.expr(&c.object);
                self.expr(&c.expression);
            }
            MemberExpression::StaticMemberExpression(s) => self.expr(&s.object),
            MemberExpression::PrivateFieldExpression(p) => self.expr(&p.object),
        }
    }

    fn args(&mut self, args: &[Argument<'_>]) {
        for arg in args {
            self.arg(arg);
        }
    }

    fn arg(&mut self, arg: &Argument<'_>) {
        match arg {
            Argument::SpreadElement(s) => {
                self.effects = true;
                self.expr(&s.argument);
            }
            other => self.expr(other.to_expression()),
        }
    }

    /// `walkCall` for a (Babel) CallExpression / OptionalCallExpression.
    fn call(&mut self, call: &CallExpression<'_>) {
        let name = callee_name(&call.callee);
        if let Some(name) = name
            && self.opts.target_writing_call_names.contains(name)
        {
            // Writes its first argument and nothing else observable.
            // Not a plain identifier: the target cannot be named, so stay
            // conservative rather than guess.
            match call
                .arguments
                .first()
                .and_then(Argument::as_expression)
                .map(unparen)
            {
                Some(Expression::Identifier(id)) => self.writes.add(&id.name),
                _ => self.effects = true,
            }
            self.expr(&call.callee);
            for arg in call.arguments.iter().skip(1) {
                self.arg(arg);
            }
            return;
        }
        if name.is_none_or(|n| !self.opts.pure_call_names.contains(n)) {
            self.effects = true;
        }
        self.expr(&call.callee);
        self.args(&call.arguments);
    }

    // -- patterns / assignment targets (Babel pattern nodes via `walk`) ------

    fn binding_pattern(&mut self, p: &BindingPattern<'_>) {
        match p {
            BindingPattern::BindingIdentifier(id) => self.read(&id.name),
            BindingPattern::ObjectPattern(o) => {
                for prop in &o.properties {
                    // ObjectProperty (walkReference): computed key, value.
                    if prop.computed {
                        self.key(&prop.key);
                    }
                    self.binding_pattern(&prop.value);
                }
                if let Some(rest) = &o.rest {
                    self.binding_pattern(&rest.argument);
                }
            }
            BindingPattern::ArrayPattern(a) => {
                for el in a.elements.iter().flatten() {
                    self.binding_pattern(el);
                }
                if let Some(rest) = &a.rest {
                    self.binding_pattern(&rest.argument);
                }
            }
            BindingPattern::AssignmentPattern(a) => {
                self.binding_pattern(&a.left);
                self.expr(&a.right);
            }
        }
    }

    fn simple_target(&mut self, t: &SimpleAssignmentTarget<'_>) {
        match t {
            SimpleAssignmentTarget::AssignmentTargetIdentifier(id) => self.read(&id.name),
            SimpleAssignmentTarget::TSAsExpression(x) => self.expr(&x.expression),
            SimpleAssignmentTarget::TSSatisfiesExpression(x) => self.expr(&x.expression),
            SimpleAssignmentTarget::TSNonNullExpression(x) => self.expr(&x.expression),
            SimpleAssignmentTarget::TSTypeAssertion(x) => self.expr(&x.expression),
            other => self.member(other.to_member_expression()),
        }
    }

    fn assign_target(&mut self, t: &AssignmentTarget<'_>) {
        match t {
            AssignmentTarget::ArrayAssignmentTarget(a) => {
                for el in a.elements.iter().flatten() {
                    self.maybe_default(el);
                }
                if let Some(rest) = &a.rest {
                    self.assign_target(&rest.target);
                }
            }
            AssignmentTarget::ObjectAssignmentTarget(o) => {
                for prop in &o.properties {
                    match prop {
                        AssignmentTargetProperty::AssignmentTargetPropertyIdentifier(p) => {
                            // ObjectProperty{shorthand, value: Identifier |
                            // AssignmentPattern(Identifier, init)}.
                            self.read(&p.binding.name);
                            if let Some(init) = &p.init {
                                self.expr(init);
                            }
                        }
                        AssignmentTargetProperty::AssignmentTargetPropertyProperty(p) => {
                            if p.computed {
                                self.key(&p.name);
                            }
                            self.maybe_default(&p.binding);
                        }
                    }
                }
                if let Some(rest) = &o.rest {
                    self.assign_target(&rest.target);
                }
            }
            other => self.simple_target(other.to_simple_assignment_target()),
        }
    }

    fn maybe_default(&mut self, t: &AssignmentTargetMaybeDefault<'_>) {
        match t {
            AssignmentTargetMaybeDefault::AssignmentTargetWithDefault(d) => {
                self.assign_target(&d.binding);
                self.expr(&d.init);
            }
            other => self.assign_target(other.to_assignment_target()),
        }
    }

    // -- classes ---------------------------------------------------------------

    fn class(&mut self, class: &Class<'_>) {
        if !class.decorators.is_empty() {
            self.effects = true;
        }
        if let Some(heritage) = &class.heritage {
            self.expr(&heritage.expression);
        }
        for el in &class.body.body {
            self.class_member(el);
        }
    }

    fn class_member(&mut self, el: &ClassElement<'_>) {
        match el {
            ClassElement::StaticBlock(_) => self.effects = true, // runs at definition
            ClassElement::MethodDefinition(m) => {
                if !m.decorators.is_empty() {
                    self.effects = true;
                }
                if m.computed {
                    self.key(&m.key);
                }
            }
            ClassElement::PropertyDefinition(p) => {
                if !p.decorators.is_empty() {
                    self.effects = true;
                }
                if p.computed {
                    self.key(&p.key);
                }
                // Only STATIC values run at definition.
                if p.r#static
                    && let Some(v) = &p.value
                {
                    self.expr(v);
                }
            }
            ClassElement::AccessorProperty(a) => {
                if !a.decorators.is_empty() {
                    self.effects = true;
                }
                if a.computed {
                    self.key(&a.key);
                }
            }
            ClassElement::TSIndexSignature(_) => {}
        }
    }

    // -- nested statements (the generic VISITOR_KEYS walk) ---------------------

    fn nested_declaration(&mut self, decl: &VariableDeclaration<'_>) {
        for d in &decl.declarations {
            self.binding_pattern(&d.id);
            if let Some(init) = &d.init {
                self.expr(init);
            }
        }
    }

    fn stmt(&mut self, s: &Statement<'_>) {
        match s {
            Statement::BlockStatement(b) => self.stmts(&b.body),
            Statement::BreakStatement(b) => {
                if let Some(l) = &b.label {
                    self.read(&l.name);
                }
            }
            Statement::ContinueStatement(c) => {
                if let Some(l) = &c.label {
                    self.read(&l.name);
                }
            }
            Statement::DoWhileStatement(d) => {
                self.stmt(&d.body);
                self.expr(&d.test);
            }
            Statement::WhileStatement(w) => {
                self.expr(&w.test);
                self.stmt(&w.body);
            }
            Statement::ForStatement(f) => {
                match &f.init {
                    Some(ForStatementInit::VariableDeclaration(d)) => self.nested_declaration(d),
                    Some(init) => self.expr(init.to_expression()),
                    None => {}
                }
                if let Some(t) = &f.test {
                    self.expr(t);
                }
                if let Some(u) = &f.update {
                    self.expr(u);
                }
                self.stmt(&f.body);
            }
            Statement::ForInStatement(f) => {
                self.for_left(&f.left);
                self.expr(&f.right);
                self.stmt(&f.body);
            }
            Statement::ForOfStatement(f) => {
                self.for_left(&f.left);
                self.expr(&f.right);
                self.stmt(&f.body);
            }
            Statement::IfStatement(i) => {
                self.expr(&i.test);
                self.stmt(&i.consequent);
                if let Some(a) = &i.alternate {
                    self.stmt(a);
                }
            }
            Statement::LabeledStatement(l) => {
                self.read(&l.label.name);
                self.stmt(&l.body);
            }
            Statement::ReturnStatement(r) => {
                if let Some(a) = &r.argument {
                    self.expr(a);
                }
            }
            Statement::ThrowStatement(t) => self.expr(&t.argument),
            Statement::SwitchStatement(sw) => {
                self.expr(&sw.discriminant);
                for case in &sw.cases {
                    if let Some(t) = &case.test {
                        self.expr(t);
                    }
                    self.stmts(&case.consequent);
                }
            }
            Statement::TryStatement(t) => {
                self.stmts(&t.block.body);
                if let Some(h) = &t.handler {
                    if let Some(p) = &h.param {
                        self.binding_pattern(&p.pattern);
                    }
                    self.stmts(&h.body.body);
                }
                if let Some(f) = &t.finalizer {
                    self.stmts(&f.body);
                }
            }
            Statement::WithStatement(w) => {
                self.expr(&w.object);
                self.stmt(&w.body);
            }
            Statement::ExpressionStatement(e) => self.expr(&e.expression),
            Statement::VariableDeclaration(d) => self.nested_declaration(d),
            // A nested function declaration is deferred; a nested class is
            // walked as a class (its id is neither read nor written).
            Statement::FunctionDeclaration(_) => {}
            Statement::ClassDeclaration(c) => self.class(c),
            _ => {}
        }
    }

    fn stmts(&mut self, stmts: &[Statement<'_>]) {
        for s in stmts {
            self.stmt(s);
        }
    }

    fn for_left(&mut self, left: &ForStatementLeft<'_>) {
        match left {
            ForStatementLeft::VariableDeclaration(d) => self.nested_declaration(d),
            other => self.assign_target(other.to_assignment_target()),
        }
    }

    // -- top level --------------------------------------------------------------

    fn top_declarator(&mut self, kind: VariableDeclarationKind, d: &VariableDeclarator<'_>) {
        if !matches!(d.id, BindingPattern::BindingIdentifier(_)) {
            // Destructuring runs the iterator/getter protocol at load time.
            self.effects = true;
        }
        // A bare `var x;` is hoisted and free to sit anywhere; `let`/`const`
        // have a TDZ, which makes the declaration's position observable.
        if d.init.is_some() || kind != VariableDeclarationKind::Var {
            let mut names = Vec::new();
            binding_identifier_names(&d.id, &mut names);
            for n in names {
                self.writes.add(&n);
            }
        }
        if let Some(init) = &d.init {
            self.expr(init);
        }
    }

    fn finish(self, hoisted: bool) -> LoadOrderFacts {
        LoadOrderFacts {
            hoisted,
            writes: self.writes.order,
            reads: self.reads.order,
            effects: self.effects,
        }
    }
}

/// `getBindingIdentifiers(pattern)`'s names.
fn binding_identifier_names(p: &BindingPattern<'_>, out: &mut Vec<String>) {
    match p {
        BindingPattern::BindingIdentifier(id) => out.push(id.name.to_string()),
        BindingPattern::ObjectPattern(o) => {
            for prop in &o.properties {
                binding_identifier_names(&prop.value, out);
            }
            if let Some(rest) = &o.rest {
                binding_identifier_names(&rest.argument, out);
            }
        }
        BindingPattern::ArrayPattern(a) => {
            for el in a.elements.iter().flatten() {
                binding_identifier_names(el, out);
            }
            if let Some(rest) = &a.rest {
                binding_identifier_names(&rest.argument, out);
            }
        }
        BindingPattern::AssignmentPattern(a) => binding_identifier_names(&a.left, out),
    }
}

/// `analyzeStatement`: one top-level statement's facts.
pub fn analyze_statement(stmt: &Statement<'_>, opts: &LoadOrderOptions) -> LoadOrderFacts {
    let mut ctx = Ctx::new(opts);
    match stmt {
        Statement::FunctionDeclaration(_) => return ctx.finish(true),
        Statement::VariableDeclaration(decl) => {
            for d in &decl.declarations {
                ctx.top_declarator(decl.kind, d);
            }
        }
        Statement::ClassDeclaration(class) => {
            if let Some(id) = &class.id {
                ctx.writes.add(&id.name);
            }
            ctx.class(class);
        }
        Statement::ExpressionStatement(e) => ctx.expr(&e.expression),
        other => {
            // Control flow, `throw`, labelled blocks, module syntax: pinned
            // as a barrier.
            ctx.effects = true;
            ctx.stmt(other);
        }
    }
    ctx.finish(false)
}

/// `analyzeLoadOrder`: facts per statement, parallel to `stmts`.
pub fn analyze_load_order(stmts: &[Statement<'_>], opts: &LoadOrderOptions) -> Vec<LoadOrderFacts> {
    stmts.iter().map(|s| analyze_statement(s, opts)).collect()
}

// ---------------------------------------------------------------------------
// The export registrar, identified by SHAPE
// ---------------------------------------------------------------------------

/// The two identifier params of a `(a, b) => …` arrow.
fn registrar_name(d: &VariableDeclarator<'_>) -> Option<String> {
    let BindingPattern::BindingIdentifier(id) = &d.id else {
        return None;
    };
    let Some(Expression::ArrowFunctionExpression(arrow)) = d.init.as_ref().map(unparen) else {
        return None;
    };
    if arrow.params.items.len() != 2 || arrow.params.rest.is_some() {
        return None;
    }
    let (BindingPattern::BindingIdentifier(target), BindingPattern::BindingIdentifier(source)) = (
        &arrow.params.items[0].pattern,
        &arrow.params.items[1].pattern,
    ) else {
        return None;
    };
    // Babel's params list is [a, b]; a defaulted param is an
    // AssignmentPattern there, not an Identifier.
    if arrow.params.items.iter().any(|p| p.initializer.is_some()) {
        return None;
    }
    // `for (var k in source)` among the body's statements (an expression
    // body is the one statement `[init.body]`, which is never a for-in).
    let oxc_ast::ast::ArrowFunctionBody::FunctionBody(body) = &arrow.body else {
        return None;
    };
    let for_in = body.statements.iter().find_map(|b| match b {
        Statement::ForInStatement(f)
            if matches!(unparen(&f.right), Expression::Identifier(r) if r.name == source.name) =>
        {
            Some(f)
        }
        _ => None,
    });
    let for_in = for_in?;
    let key = match &for_in.left {
        ForStatementLeft::VariableDeclaration(v) => match v.declarations.first().map(|d| &d.id) {
            Some(BindingPattern::BindingIdentifier(k)) => k.name.as_str(),
            _ => return None,
        },
        ForStatementLeft::AssignmentTargetIdentifier(k) => k.name.as_str(),
        _ => return None,
    };
    // The single call expression the loop body consists of.
    let inner = match &for_in.body {
        Statement::ExpressionStatement(e) => Some(&e.expression),
        Statement::BlockStatement(b) => match b.body.first() {
            Some(Statement::ExpressionStatement(e)) => Some(&e.expression),
            _ => None,
        },
        _ => None,
    }?;
    let Expression::CallExpression(call) = unparen(inner) else {
        return None;
    };
    if call.optional || call.arguments.len() != 3 {
        return None;
    }
    let arg_ident = |i: usize| match call.arguments[i].as_expression().map(unparen) {
        Some(Expression::Identifier(x)) => Some(x.name.as_str()),
        _ => None,
    };
    if arg_ident(0) != Some(target.name.as_str()) || arg_ident(1) != Some(key) {
        return None;
    }
    installs_lazy_getter(&call.arguments[2], source.name.as_str(), key).then(|| id.name.to_string())
}

/// `get: source[key]` — the lazy-getter certificate.
fn installs_lazy_getter(descriptor: &Argument<'_>, source: &str, key: &str) -> bool {
    let Some(Expression::ObjectExpression(obj)) = descriptor.as_expression().map(unparen) else {
        return false;
    };
    obj.properties.iter().any(|prop| {
        let ObjectPropertyKind::ObjectProperty(p) = prop else {
            return false;
        };
        if p.method || p.kind != PropertyKind::Init {
            return false; // babel ObjectMethod
        }
        let key_is_get = match &p.key {
            PropertyKey::StaticIdentifier(k) => k.name == "get",
            PropertyKey::Identifier(k) => k.name == "get",
            _ => false,
        };
        if !key_is_get {
            return false;
        }
        let (object, property) = match unparen(&p.value) {
            Expression::ComputedMemberExpression(m) if !m.optional => {
                (&m.object, unparen(&m.expression))
            }
            Expression::StaticMemberExpression(m) if !m.optional => {
                return matches!(unparen(&m.object), Expression::Identifier(o) if o.name == source)
                    && m.property.name == key;
            }
            _ => return false,
        };
        matches!(unparen(object), Expression::Identifier(o) if o.name == source)
            && matches!(property, Expression::Identifier(k) if k.name == key)
    })
}

/// `identifyExportRegistrar`: the first top-level declarator with the
/// registrar's shape.
pub fn identify_export_registrar(stmts: &[Statement<'_>]) -> Option<String> {
    stmts.iter().find_map(|s| match s {
        Statement::VariableDeclaration(decl) => decl.declarations.iter().find_map(registrar_name),
        _ => None,
    })
}

/// `bundleLoadOrderFacts`: facts for the bundle's statements, admitting the
/// structurally-verified lazy-init wrapper as pure and the export registrar
/// as target-writing. `registrar_exemption_disabled` is the
/// `--disable registrar-exemption` kill switch (the pre-049 behaviour).
pub fn bundle_load_order_facts(
    stmts: &[Statement<'_>],
    code: &str,
    registrar_exemption_disabled: bool,
) -> Vec<LoadOrderFacts> {
    let mut opts = LoadOrderOptions::default();
    if let Some(lazy) = identify_bun_lazy_init(code) {
        opts.pure_call_names.insert(lazy);
    }
    if !registrar_exemption_disabled && let Some(reg) = identify_export_registrar(stmts) {
        opts.target_writing_call_names.insert(reg);
    }
    analyze_load_order(stmts, &opts)
}

// ---------------------------------------------------------------------------
// The constrained order
// ---------------------------------------------------------------------------

/// Running dependence state across one pass in bundle order.
struct DepState<'f> {
    last_writer: std::collections::HashMap<&'f str, usize>,
    readers: std::collections::HashMap<&'f str, Vec<usize>>,
    last_barrier: Option<usize>,
    since_barrier: Vec<usize>,
}

/// `buildDependencies`: predecessors per slot (slot → list, parallel to
/// the position of the slot in `slots`). Every edge points forward.
fn build_dependencies(
    slots: &[usize],
    facts: &[LoadOrderFacts],
) -> std::collections::HashMap<usize, Vec<usize>> {
    let mut preds: std::collections::HashMap<usize, Vec<usize>> =
        slots.iter().map(|&s| (s, Vec::new())).collect();
    let add = |from: usize, to: usize, preds: &mut std::collections::HashMap<usize, Vec<usize>>| {
        if from != to {
            preds.get_mut(&to).expect("slot").push(from);
        }
    };
    let mut st = DepState {
        last_writer: std::collections::HashMap::new(),
        readers: std::collections::HashMap::new(),
        last_barrier: None,
        since_barrier: Vec::new(),
    };
    for &s in slots {
        let f = &facts[s];
        if f.hoisted {
            continue;
        }
        // Barrier edges.
        if let Some(b) = st.last_barrier {
            add(b, s, &mut preds);
        }
        if f.effects {
            for p in std::mem::take(&mut st.since_barrier) {
                add(p, s, &mut preds);
            }
            st.last_barrier = Some(s);
        } else {
            st.since_barrier.push(s);
        }
        // Data edges: RAW, WAR, WAW.
        for n in &f.reads {
            if let Some(&w) = st.last_writer.get(n.as_str()) {
                add(w, s, &mut preds);
            }
            st.readers.entry(n.as_str()).or_default().push(s);
        }
        for n in &f.writes {
            if let Some(&w) = st.last_writer.get(n.as_str()) {
                add(w, s, &mut preds);
            }
            for r in st
                .readers
                .insert(n.as_str(), Vec::new())
                .unwrap_or_default()
            {
                add(r, s, &mut preds);
            }
            st.last_writer.insert(n.as_str(), s);
        }
    }
    preds
}

/// `orderRespectingLoadOrder`: `slots` as close to `desired` as the
/// load-time dependencies allow — greedy topological scheduling, taking
/// the ready statement the desired order wants soonest (ties never occur:
/// ranks are a permutation; the index breaks them as the TS heap does).
pub fn order_respecting_load_order(
    slots: &[usize],
    desired: &[usize],
    facts: &[LoadOrderFacts],
) -> Vec<usize> {
    if slots.len() < 2 {
        return slots.to_vec();
    }
    let mut rank: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    for (i, &d) in desired.iter().enumerate() {
        rank.insert(d, i);
    }
    for &s in slots {
        let next = rank.len();
        rank.entry(s).or_insert(next);
    }
    let preds = build_dependencies(slots, facts);
    let mut indegree: std::collections::HashMap<usize, usize> =
        slots.iter().map(|&s| (s, 0)).collect();
    let mut successors: std::collections::HashMap<usize, Vec<usize>> =
        slots.iter().map(|&s| (s, Vec::new())).collect();
    // Slot order (a HashMap's order is not observable here, but keep it
    // deterministic): indegrees are sums and the heap orders by rank.
    for &to in slots {
        for &from in &preds[&to] {
            successors.get_mut(&from).expect("slot").push(to);
            *indegree.get_mut(&to).expect("slot") += 1;
        }
    }
    let mut heap = std::collections::BTreeSet::new();
    for &s in slots {
        if indegree[&s] == 0 {
            heap.insert((rank[&s], s));
        }
    }
    let mut out = Vec::with_capacity(slots.len());
    while let Some((r, s)) = heap.iter().next().copied() {
        heap.remove(&(r, s));
        out.push(s);
        for &nxt in &successors[&s] {
            let left = indegree.get_mut(&nxt).expect("slot");
            *left -= 1;
            if *left == 0 {
                heap.insert((rank[&nxt], nxt));
            }
        }
    }
    // Unreachable for a forward-only edge set, but never emit a partial file.
    if out.len() == slots.len() {
        out
    } else {
        slots.to_vec()
    }
}

#[cfg(test)]
mod load_order_test;
