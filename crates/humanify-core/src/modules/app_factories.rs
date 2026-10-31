//! Which extracted CommonJS factories are the APP's own modules (exp172).
//!
//! Bun and esbuild wrap every CommonJS module as `var X = HELPER(fn)`,
//! app or library, and the unpack used to write every one under
//! `vendor/`. An app written in CommonJS therefore landed wholly in
//! `vendor/` (exp169 phase 0: six bun-dev apps, 388 KB of their own code).
//! This module is the ONE owner of the question "is this factory the
//! app's own module?". The unpack asks it once, before any name is chosen,
//! and keeps every factory it accepts in the app text, where the naming,
//! the split and the placement treat it as app code.
//!
//! Precision over recall: a factory is accepted only when the evidence
//! agrees, and everything uncertain stays in `vendor/`, which is the old
//! behaviour. The evidence:
//!
//! - **Library evidence** (any one is enough to refuse): a package the
//!   index claims on the module's statements or places it under (names
//!   mode), a package the module declares itself (`modules::identity`), a
//!   source path under `node_modules/`. A library never requires app code,
//!   so everything a library module requires is library too (the downward
//!   closure).
//! - **App seeds**: the ENTRY factories — when the app text outside the
//!   factories is only glue (helper declarations, `module.exports = {}`,
//!   and bare calls of factories: `X();`, `module.exports = X();`,
//!   `export default X();`), the called factories ARE the app's entry, a
//!   CommonJS module the bundler wrapped; and a module the index
//!   attributes to the app's OWN package. A source path the bundler kept
//!   (esbuild's unminified wrapper key) is library evidence under
//!   `node_modules/` and nothing otherwise: the e2e fixtures stand their
//!   vendored dependencies in as local `src/vendor/*.cjs` files, and the
//!   key is recorded metadata, never a behavioural switch on its own.
//! - **The own package**: a package P whose modules and the entry's
//!   unattributed modules require each other (the app's code calls into
//!   P's and P's calls back), or whose inlined `package.json` the entry's
//!   unattributed modules require (a CLI reading its own version). P is
//!   refused when any module of another package requires one of P's (a
//!   library depends on it). The bun-dev apps are published packages the
//!   index knows; a private app is not in the index, and then only the
//!   entry and the modules above it are accepted.
//! - **Upward closure**: a factory reachable from the seeds that requires
//!   an accepted module is accepted (libraries never require app code).
//!   Nothing is accepted DOWNWARD: an unattributed module the app
//!   requires may be an unindexed library (exp172 phase 0: 45 KB of them
//!   on bun-dev against 37 KB of app), so it stays in `vendor/`.
//! - **Placement soundness**: an accepted module required by a factory
//!   that stays in `vendor/` is refused, to a fixpoint — a vendor file
//!   cannot reach an app binding.
//!
//! Bundler-neutral: the facts come from the wrapper grammar's
//! classification, the scope plan's reference graph and the identity
//! readers; nothing here names a bundler.

use std::collections::{BTreeSet, HashSet};

use oxc_ast::ast::{
    AssignmentTarget, ExportDefaultDeclarationKind, Expression, Statement, VariableDeclaration,
};
use serde::Serialize;

/// What the unpack knows about one extracted factory, gathered before any
/// name is chosen.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FactoryEvidence {
    /// The packages library evidence names: the index's claims on the
    /// module's statements and its placement path (names mode), its
    /// declared identity's package.
    pub packages: BTreeSet<String>,
    /// The package an inlined `package.json` module declares (its declared
    /// identity's member is `package.json`).
    pub manifest_of: Option<String>,
    /// The module's original source path, when the bundler kept it.
    pub source_path: Option<String>,
    /// Never accepted: an app text asset (already written under
    /// `src/_assets/`), or a factory whose references cannot be planned.
    pub excluded: bool,
    /// The factories this module's body requires (indices into the same
    /// list).
    pub requires: Vec<usize>,
}

/// Why a factory was accepted as an app module.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AppReason {
    /// The app text runs it (the app's entry).
    Entry,
    /// The index or its declared identity attributes it to the app's own
    /// package.
    OwnPackage,
    /// It requires an accepted app module.
    RequiresApp,
}

/// The decision for one bundle.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AppDecision {
    /// The packages read as the app's own.
    pub own_packages: Vec<String>,
    /// Every accepted factory (index) with its reason, in index order.
    pub modules: Vec<(usize, AppReason)>,
}

fn is_node_modules_path(path: &str) -> bool {
    path.split(['/', '\\'])
        .any(|segment| segment == "node_modules")
}

/// The decision. `entries`: the factories the app text runs, when that
/// text is only glue ([`entry_factories`]); None otherwise.
pub fn decide(evidence: &[FactoryEvidence], entries: Option<&[usize]>) -> AppDecision {
    let n = evidence.len();
    let mut required_by: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (i, e) in evidence.iter().enumerate() {
        for &j in &e.requires {
            if j < n && j != i {
                required_by[j].push(i);
            }
        }
    }
    let entries: Vec<usize> = entries
        .unwrap_or_default()
        .iter()
        .copied()
        .filter(|&i| i < n && !evidence[i].excluded)
        .collect();
    let own = own_packages(evidence, &entries, &required_by);
    let library = library_closure(evidence, &own);
    let region = reach(evidence, &entries, |i| !library[i] && !evidence[i].excluded);
    let mut reason: Vec<Option<AppReason>> = vec![None; n];
    for &i in &entries {
        if region.contains(&i) {
            reason[i] = Some(AppReason::Entry);
        }
    }
    for &i in &region {
        if reason[i].is_some() {
            continue;
        }
        let e = &evidence[i];
        if !e.packages.is_empty() && e.packages.iter().all(|p| own.contains(p)) {
            reason[i] = Some(AppReason::OwnPackage);
        }
    }
    // Upward closure inside the region: requiring app code makes app code.
    loop {
        let mut changed = false;
        for &i in &region {
            if reason[i].is_none() && evidence[i].requires.iter().any(|&j| reason[j].is_some()) {
                reason[i] = Some(AppReason::RequiresApp);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    // Placement soundness: no vendor file may require an app module.
    loop {
        let mut changed = false;
        for i in 0..n {
            if reason[i].is_some() && required_by[i].iter().any(|&r| reason[r].is_none()) {
                reason[i] = None;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    AppDecision {
        own_packages: own.into_iter().collect(),
        modules: reason
            .into_iter()
            .enumerate()
            .filter_map(|(i, r)| r.map(|r| (i, r)))
            .collect(),
    }
}

/// Every factory reachable from `seeds` through factories `pass` admits
/// (a seed it refuses is not entered).
fn reach(
    evidence: &[FactoryEvidence],
    seeds: &[usize],
    pass: impl Fn(usize) -> bool,
) -> BTreeSet<usize> {
    let mut seen = BTreeSet::new();
    let mut stack: Vec<usize> = seeds.to_vec();
    while let Some(i) = stack.pop() {
        if i >= evidence.len() || seen.contains(&i) || !pass(i) {
            continue;
        }
        seen.insert(i);
        stack.extend(evidence[i].requires.iter().copied());
    }
    seen
}

/// Library evidence for a module, `own` the app's own packages.
fn library_evidence(e: &FactoryEvidence, own: &BTreeSet<String>) -> bool {
    e.packages.iter().any(|p| !own.contains(p))
        || e.source_path.as_deref().is_some_and(is_node_modules_path)
}

/// Library evidence and everything it requires.
fn library_closure(evidence: &[FactoryEvidence], own: &BTreeSet<String>) -> Vec<bool> {
    let seeds: Vec<usize> = (0..evidence.len())
        .filter(|&i| library_evidence(&evidence[i], own))
        .collect();
    let mut library = vec![false; evidence.len()];
    for i in reach(evidence, &seeds, |_| true) {
        library[i] = true;
    }
    library
}

/// The app's own packages (see the module doc).
fn own_packages(
    evidence: &[FactoryEvidence],
    entries: &[usize],
    required_by: &[Vec<usize>],
) -> BTreeSet<String> {
    let none = BTreeSet::new();
    // The entry's unattributed modules.
    let front = reach(evidence, entries, |i| {
        let e = &evidence[i];
        !e.excluded && e.packages.is_empty() && !library_evidence(e, &none)
    });
    let mut candidates: BTreeSet<String> = BTreeSet::new();
    for &u in &front {
        for &j in &evidence[u].requires {
            let e = &evidence[j];
            if let Some(p) = &e.manifest_of {
                candidates.insert(p.clone());
            }
            // Mutual: P requires the front module that requires P.
            for p in &e.packages {
                let calls_back = required_by[u]
                    .iter()
                    .any(|&r| evidence[r].packages.contains(p));
                if calls_back {
                    candidates.insert(p.clone());
                }
            }
        }
    }
    candidates
        .into_iter()
        .filter(|p| {
            // Refused when another package's module requires one of P's.
            !evidence.iter().enumerate().any(|(i, e)| {
                e.packages.contains(p)
                    && required_by[i]
                        .iter()
                        .any(|&r| evidence[r].packages.iter().any(|q| q != p))
            })
        })
        .collect()
}

/// The factories the app text RUNS, when that text is only glue — the
/// bundle's entry is itself a wrapped CommonJS module. `statements` are
/// the container's statements outside every factory declarator (a
/// statement that also declares factories is handed whole; `factory_at`
/// recognizes those declarators), `factory_of` resolves an identifier
/// reference to the factory it names, `require_alias` is the bundle's
/// require variable (declared by a `createRequire(…)` call). None when any
/// statement is something else, or nothing is run.
pub fn entry_factories<'a>(
    statements: impl IntoIterator<Item = &'a Statement<'a>>,
    factory_at: impl Fn(oxc_span::Span) -> bool,
    factory_of: impl Fn(&oxc_ast::ast::IdentifierReference<'_>) -> Option<usize>,
    require_alias: Option<&str>,
) -> Option<Vec<usize>> {
    let mut entries: Vec<usize> = Vec::new();
    let run = |e: &Expression<'_>| -> Option<usize> {
        let Expression::CallExpression(call) = e.without_parentheses() else {
            return None;
        };
        if !call.arguments.is_empty() {
            return None;
        }
        let Expression::Identifier(id) = call.callee.without_parentheses() else {
            return None;
        };
        factory_of(id)
    };
    for statement in statements {
        match statement {
            Statement::EmptyStatement(_)
            | Statement::FunctionDeclaration(_)
            | Statement::ImportDeclaration(_) => {}
            Statement::VariableDeclaration(d) => {
                if !glue_declaration(d, &factory_at, require_alias) {
                    return None;
                }
            }
            Statement::ExpressionStatement(s) => {
                if let Some(i) = run(&s.expression) {
                    entries.push(i);
                    continue;
                }
                let Expression::AssignmentExpression(a) = s.expression.without_parentheses() else {
                    return None;
                };
                if !a.operator.is_assign() || !is_module_exports(&a.left) {
                    return None;
                }
                match a.right.without_parentheses() {
                    Expression::ObjectExpression(o) if o.properties.is_empty() => {}
                    right => entries.push(run(right)?),
                }
            }
            Statement::ExportDefaultDeclaration(d) => {
                let ExportDefaultDeclarationKind::CallExpression(_) = &d.declaration else {
                    return None;
                };
                entries.push(run(d.declaration.as_expression()?)?);
            }
            _ => return None,
        }
    }
    entries.sort_unstable();
    entries.dedup();
    (!entries.is_empty()).then_some(entries)
}

/// `module.exports` as an assignment target.
fn is_module_exports(target: &AssignmentTarget<'_>) -> bool {
    let AssignmentTarget::StaticMemberExpression(m) = target else {
        return false;
    };
    m.property.name == "exports"
        && matches!(m.object.without_parentheses(), Expression::Identifier(id) if id.name == "module")
}

/// A declaration that runs nothing: every declarator is a factory, has no
/// initializer, declares the require alias, or is initialized by a value
/// that evaluates without calling anything (a function, a literal, a
/// name, a member read) — the bundler's helper prologue.
fn glue_declaration(
    d: &VariableDeclaration<'_>,
    factory_at: &impl Fn(oxc_span::Span) -> bool,
    require_alias: Option<&str>,
) -> bool {
    d.declarations.iter().all(|decl| {
        if factory_at(decl.span) {
            return true;
        }
        if require_alias.is_some_and(|alias| {
            decl.id
                .get_binding_identifier()
                .is_some_and(|id| id.name == alias)
        }) {
            return true;
        }
        decl.init.as_ref().is_none_or(inert_value)
    })
}

/// A value that evaluates without calling anything.
fn inert_value(e: &Expression<'_>) -> bool {
    match e.without_parentheses() {
        Expression::ArrowFunctionExpression(_)
        | Expression::FunctionExpression(_)
        | Expression::Identifier(_)
        | Expression::BooleanLiteral(_)
        | Expression::NullLiteral(_)
        | Expression::NumericLiteral(_)
        | Expression::StringLiteral(_) => true,
        Expression::StaticMemberExpression(m) => inert_value(&m.object),
        Expression::ObjectExpression(o) => o.properties.is_empty(),
        _ => false,
    }
}

/// The app factories as a set (callers that only filter).
pub fn accepted(decision: &AppDecision) -> HashSet<usize> {
    decision.modules.iter().map(|&(i, _)| i).collect()
}

#[cfg(test)]
#[path = "app_factories_test.rs"]
mod app_factories_test;
