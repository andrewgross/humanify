//! Immutable literals: the parts of a module a minifier never renames —
//! the library index's bundler-independent evidence (exp095
//! immutable-literals; docs/design/library-index.md §7).
//!
//! One extractor for both sides of a lookup: the offline builder reads npm
//! SOURCE files ([`LiteralMode::Source`]), the pipeline will read a
//! bundled module ([`LiteralMode::Bundled`]). The rules are the lab's
//! (`immutable-literals/scripts/lits.mjs`), ported:
//!
//! - `S:<text>` a string: a string literal, a template chunk, a folded
//!   `"a" + "b"` (the whole and its parts);
//! - `P:<name>` a property name: an object or class key, a destructured
//!   key, a member access `.x` / `["x"]`, and (source mode) an ESM export
//!   or imported name, which a bundler prints as a key;
//! - `N:<number>` a number by value (`1e3` is `N:1000`), a bigint as
//!   `N:<digits>n`;
//! - `R:/pattern/flags` a regex.
//!
//! Normalised so both sides agree: a value longer than 120 UTF-16 units is
//! kept as `S#`/`P#`/`R#` + the first 16 hex of its sha1 (it still matches
//! exactly); a Node builtin's name takes the `node:` spelling (bundlers
//! write either); `0` and `1` are dropped (every module has them);
//! `"undefined"` reads as `"u"` (minifiers turn `typeof x == "undefined"`
//! into `typeof x > "u"`); a directive (`"use strict"`) is not a string.
//!
//! Source mode drops what bundling removes — the specifier of a bundled
//! `require("./x")`, `import … from "./x"`, `export … from "./x"` and a
//! constant `import("./x")` (each becomes a call to the module's factory)
//! — and keeps a builtin's, which the bundle still names.
//!
//! NOT the same question as `modules::vendor_pairing`'s literal feature:
//! that one compares a module with its own previous release inside one run
//! (token-based, strings of 5+ characters and long identifiers, IDF over
//! the prior tree). This one is what survives bundling, read from source.

use std::collections::BTreeSet;

use oxc_ast::AstKind;
use oxc_ast::ast::{
    BindingPattern, Declaration, Expression, ImportDeclarationSpecifier, ModuleExportName,
    PropertyKey,
};
use oxc_span::Span;
use oxc_syntax::operator::BinaryOperator;
use sha1::{Digest, Sha1};

use crate::babel_view::unparen;
use crate::finish::scaffold::is_builtin_name;
use crate::ingest::Ingest;

// The extractor's version is `hash::forms::LIBRARY_LITERALS_VERSION` (the
// one registry of persisted fingerprint stamps): a change to any rule above
// is a new version (the index manifest records it; the pipeline refuses a
// mismatch).

/// Values longer than this many UTF-16 units are kept as a hash prefix.
const LONG: usize = 120;

/// Where the code comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LiteralMode {
    /// An npm file as published.
    Source,
    /// A module inside a bundle (minified or not).
    Bundled,
}

/// A literal set with the lab's two set-level rules.
#[derive(Default)]
struct LitSet(BTreeSet<String>);

impl LitSet {
    fn add(&mut self, lit: String) {
        match lit.as_str() {
            "N:0" | "N:1" => {}
            "S:undefined" => {
                self.0.insert("S:u".to_string());
            }
            _ => {
                self.0.insert(lit);
            }
        }
    }

    /// A string-valued literal of `kind` (`S`, `P`, `R`), normalised.
    fn text(&mut self, kind: char, value: &str) {
        let value = if kind == 'S' && is_builtin_name(value) {
            format!("node:{}", value.strip_prefix("node:").unwrap_or(value))
        } else {
            value.to_string()
        };
        if value.encode_utf16().count() > LONG {
            let digest = Sha1::digest(value.as_bytes());
            let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
            self.add(format!("{kind}#{}", &hex[..16]));
        } else {
            self.add(format!("{kind}:{value}"));
        }
    }

    fn number(&mut self, value: f64) {
        self.add(format!("N:{}", humanify_model::js::number_to_string(value)));
    }
}

/// The constant string an expression folds to (`"a"`, `` `a` ``,
/// `"a" + "b"`), if any.
fn const_str(expr: &Expression<'_>) -> Option<String> {
    match unparen(expr) {
        Expression::StringLiteral(s) => Some(s.value.to_string()),
        Expression::TemplateLiteral(t) if t.expressions.is_empty() && t.quasis.len() == 1 => {
            let q = &t.quasis[0].value;
            Some(q.cooked.as_ref().unwrap_or(&q.raw).to_string())
        }
        Expression::BinaryExpression(b) if b.operator == BinaryOperator::Addition => {
            Some(const_str(&b.left)? + &const_str(&b.right)?)
        }
        _ => None,
    }
}

/// Spans whose strings are NOT strings of the module (directives, keys,
/// bundled specifiers), merged into disjoint sorted intervals.
#[derive(Default)]
struct Excluded(Vec<(u32, u32)>);

impl Excluded {
    fn push(&mut self, span: Span) {
        self.0.push((span.start, span.end));
    }

    fn seal(&mut self) {
        self.0.sort_unstable();
        let mut merged: Vec<(u32, u32)> = Vec::with_capacity(self.0.len());
        for (s, e) in self.0.drain(..) {
            match merged.last_mut() {
                Some(last) if s <= last.1 => last.1 = last.1.max(e),
                _ => merged.push((s, e)),
            }
        }
        self.0 = merged;
    }

    fn contains(&self, span: Span) -> bool {
        let i = self.0.partition_point(|&(s, _)| s <= span.start);
        i > 0 && self.0[i - 1].1 >= span.end
    }
}

/// The first pass: names (`P:`) and the spans whose strings are not the
/// module's strings.
struct Names<'s> {
    lits: &'s mut LitSet,
    excluded: Excluded,
    source: bool,
}

impl Names<'_> {
    fn key(&mut self, key: &PropertyKey<'_>, computed: bool) {
        if computed {
            if let Some(expr) = key.as_expression()
                && let Some(s) = const_str(expr)
            {
                self.lits.text('P', &s);
                self.excluded.push(oxc_span::GetSpan::span(expr));
            }
            return;
        }
        match key {
            PropertyKey::StaticIdentifier(id) => self.lits.text('P', &id.name),
            PropertyKey::StringLiteral(s) => {
                self.lits.text('P', &s.value);
                self.excluded.push(s.span);
            }
            // A number key is read as a number by the second pass; a
            // `#private` name is renamed by minifiers.
            _ => {}
        }
    }

    fn export_name(&mut self, name: &ModuleExportName<'_>) {
        match name {
            ModuleExportName::IdentifierName(n) => self.lits.text('P', &n.name),
            ModuleExportName::IdentifierReference(r) => self.lits.text('P', &r.name),
            ModuleExportName::StringLiteral(s) => {
                self.lits.text('P', &s.value);
                self.excluded.push(s.span);
            }
        }
    }

    fn pattern_names(&mut self, pattern: &BindingPattern<'_>) {
        match pattern {
            BindingPattern::BindingIdentifier(id) => self.lits.text('P', &id.name),
            BindingPattern::ObjectPattern(o) => {
                for p in &o.properties {
                    self.pattern_names(&p.value);
                }
                if let Some(rest) = &o.rest {
                    self.pattern_names(&rest.argument);
                }
            }
            BindingPattern::ArrayPattern(a) => {
                for p in a.elements.iter().flatten() {
                    self.pattern_names(p);
                }
                if let Some(rest) = &a.rest {
                    self.pattern_names(&rest.argument);
                }
            }
            BindingPattern::AssignmentPattern(a) => self.pattern_names(&a.left),
        }
    }

    fn declared_names(&mut self, decl: &Declaration<'_>) {
        match decl {
            Declaration::VariableDeclaration(v) => {
                for d in &v.declarations {
                    self.pattern_names(&d.id);
                }
            }
            Declaration::FunctionDeclaration(f) => {
                if let Some(id) = &f.id {
                    self.lits.text('P', &id.name);
                }
            }
            Declaration::ClassDeclaration(c) => {
                if let Some(id) = &c.id {
                    self.lits.text('P', &id.name);
                }
            }
            _ => {}
        }
    }

    /// A bundled specifier (not a builtin): its string is gone after
    /// bundling.
    fn bundled_specifier(&mut self, expr: &Expression<'_>) {
        if let Some(s) = const_str(expr)
            && !is_builtin_name(&s)
        {
            self.excluded.push(oxc_span::GetSpan::span(expr));
        }
    }

    fn visit(&mut self, kind: &AstKind<'_>) {
        match kind {
            AstKind::Directive(d) => self.excluded.push(d.span),
            AstKind::ObjectProperty(p) => self.key(&p.key, p.computed),
            AstKind::BindingProperty(p) => self.key(&p.key, p.computed),
            AstKind::AssignmentTargetPropertyIdentifier(p) => {
                self.lits.text('P', &p.binding.name);
            }
            AstKind::AssignmentTargetPropertyProperty(p) => self.key(&p.name, p.computed),
            AstKind::MethodDefinition(m) => self.key(&m.key, m.computed),
            AstKind::PropertyDefinition(m) => self.key(&m.key, m.computed),
            AstKind::AccessorProperty(m) => self.key(&m.key, m.computed),
            AstKind::StaticMemberExpression(m) => self.lits.text('P', &m.property.name),
            AstKind::ComputedMemberExpression(m) => {
                if let Some(s) = const_str(&m.expression) {
                    self.lits.text('P', &s);
                    self.excluded.push(oxc_span::GetSpan::span(&m.expression));
                }
            }
            _ if self.source => self.visit_module_syntax(kind),
            _ => {}
        }
    }

    /// Source mode: ESM syntax and `require` — what bundling removes and
    /// what it adds.
    fn visit_module_syntax(&mut self, kind: &AstKind<'_>) {
        match kind {
            AstKind::ImportDeclaration(d) => {
                for s in d.specifiers.iter().flatten() {
                    if let ImportDeclarationSpecifier::ImportSpecifier(s) = s {
                        self.export_name(&s.imported);
                    }
                }
                if !is_builtin_name(&d.source.value) {
                    self.excluded.push(d.source.span);
                }
            }
            AstKind::ExportNamedDeclaration(d) => {
                for s in &d.specifiers {
                    self.export_name(&s.exported);
                }
            }
            AstKind::ExportFromDeclaration(d) => {
                for s in &d.specifiers {
                    self.export_name(&s.exported);
                }
                self.excluded.push(d.source.span);
            }
            AstKind::ExportAllDeclaration(d) => {
                if let Some(e) = &d.exported {
                    self.export_name(e);
                }
                self.excluded.push(d.source.span);
            }
            AstKind::ExportDeclaration(d) => self.declared_names(&d.declaration),
            AstKind::ExportDefaultDeclaration(_) => self.lits.text('P', "default"),
            AstKind::ImportExpression(e) => self.bundled_specifier(&e.source),
            AstKind::CallExpression(c) => {
                if matches!(&c.callee, Expression::Identifier(id) if id.name == "require")
                    && c.arguments.len() == 1
                    && let Some(arg) = c.arguments[0].as_expression()
                {
                    self.bundled_specifier(arg);
                }
            }
            _ => {}
        }
    }
}

/// The second pass: strings, numbers and regexes outside the excluded
/// spans.
fn visit_values(lits: &mut LitSet, excluded: &Excluded, code: &str, kind: &AstKind<'_>) {
    match kind {
        AstKind::StringLiteral(s) if !excluded.contains(s.span) => lits.text('S', &s.value),
        AstKind::TemplateLiteral(t) if !excluded.contains(t.span) => {
            for q in &t.quasis {
                let v = q.value.cooked.as_ref().unwrap_or(&q.value.raw);
                if !v.is_empty() {
                    lits.text('S', v);
                }
            }
        }
        AstKind::BinaryExpression(b)
            if b.operator == BinaryOperator::Addition && !excluded.contains(b.span) =>
        {
            let folded = const_str(&b.left).and_then(|l| Some(l + &const_str(&b.right)?));
            if let Some(s) = folded {
                lits.text('S', &s);
            }
        }
        AstKind::NumericLiteral(n) => lits.number(n.value),
        AstKind::BigIntLiteral(b) => lits.add(format!("N:{}n", b.value)),
        AstKind::RegExpLiteral(r) => {
            lits.text('R', &code[r.span.start as usize..r.span.end as usize]);
        }
        _ => {}
    }
}

/// The literal set of a piece of JavaScript; None when it does not parse
/// (the index records the file as unread rather than guess from tokens).
pub fn code_literals(code: &str, mode: LiteralMode) -> Option<BTreeSet<String>> {
    let allocator = oxc_allocator::Allocator::default();
    let ingest = Ingest::parse_unambiguous(&allocator, code);
    if !ingest.errors.is_empty() {
        return None;
    }
    let mut lits = LitSet::default();
    let nodes = ingest.semantic().nodes();
    let mut names = Names {
        lits: &mut lits,
        excluded: Excluded::default(),
        source: mode == LiteralMode::Source,
    };
    for node in nodes.iter() {
        names.visit(&node.kind());
    }
    let mut excluded = std::mem::take(&mut names.excluded);
    excluded.seal();
    for node in nodes.iter() {
        visit_values(&mut lits, &excluded, code, &node.kind());
    }
    Some(lits.0)
}

/// The literal set of a JSON file (an npm package's `.json` files are
/// bundled as objects): keys are properties, values strings and numbers.
pub fn json_literals(text: &str) -> Option<BTreeSet<String>> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    let mut lits = LitSet::default();
    let mut stack = vec![&value];
    while let Some(v) = stack.pop() {
        match v {
            serde_json::Value::String(s) => lits.text('S', s),
            serde_json::Value::Number(n) => {
                if let Some(f) = n.as_f64() {
                    lits.number(f);
                }
            }
            serde_json::Value::Array(items) => stack.extend(items),
            serde_json::Value::Object(map) => {
                for (k, item) in map {
                    lits.text('P', k);
                    stack.push(item);
                }
            }
            _ => {}
        }
    }
    Some(lits.0)
}
