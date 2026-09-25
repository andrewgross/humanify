//! Carry a TS-era prior's vendor names by CONTENT (WP5.6e; exp046/047:
//! match vendor by content, never by path or hash bytes).
//!
//! A vendor manifest's `structuralHash` is the cross-release join key of
//! the carry-over (`name_cjs_factories`) and of the manifest order
//! (`order_by_prior_manifest`). The TS wrote those bytes with its own
//! function; the Rust's never equal them, so a TS-written prior manifest
//! (no `hashVersion`, [`super::FACTORY_HASH_VERSION`]) joins NOTHING by hash
//! and every carried name would mint again.
//!
//! The re-key reads the prior tree itself: each prior entry's vendor FILE
//! holds its factory function (the unpack wrote the raw body; the relink
//! wrapped it as `exports.f = __commonJS(F)` and turned every factory
//! reference into `<requireBound>.f`). [`prior_file_content_key`] undoes
//! the relink's `.f`, and [`fresh_content_keys`] reads the fresh bundle's
//! factory bodies with the unpack's own `require` rewrite — then BOTH sides
//! hash the same form through [`vendor_content_key`]: the factory function
//! alone, under the canonical blurred serializer, with every free
//! identifier that is not a known global made a SLOT. Those are the
//! references to the rest of the bundle — other factories (the fresh body's
//! minified factory vars, the prior file's `lib_<hash8>` identifiers) and
//! bundle-level helpers the unpack leaves free — whose spellings are
//! release-specific either way. The key is symmetric by construction; it is
//! not (and need not be) the classification's `structuralHash`.
//!
//! [`rekey_prior_by_content`] then translates the prior's names and
//! manifest entries onto the FRESH structural hashes, group for group, and
//! only where the correspondence is exact: a fresh hash group carries the
//! names of the prior content group whose members are exactly its own, in
//! the prior group's bundle order (`hashOrdinal`). Anything ambiguous mints.

use std::collections::{BTreeMap, HashMap, HashSet};

use oxc_allocator::Allocator;
use oxc_ast::AstKind;
use oxc_ast::ast::{BindingPattern, Expression, Statement};
use oxc_span::GetSpan;

use super::known_globals::is_known_global;
use super::vendor_names::PriorManifestEntry;
use super::{FactoryRecord, factory_structural_hash};
use crate::babel_view::unparen;
use crate::hash::serialize::SymbolTables;
use crate::ingest::Ingest;
use crate::unpack::bun::rewrite_require_calls;

/// One keying pass: `(F)` with `declared` declared around it. The hash,
/// and the free identifiers that are not known globals (sorted).
fn keyed(factory: &str, declared: &[String]) -> Option<(String, Vec<String>)> {
    let mut src = String::with_capacity(factory.len() + 16 + declared.len() * 8);
    if !declared.is_empty() {
        src.push_str("var ");
        src.push_str(&declared.join(","));
        src.push_str(";\n");
    }
    src.push('(');
    src.push_str(factory);
    src.push_str("\n);");
    let allocator = Allocator::default();
    let ingest = Ingest::parse_unambiguous(&allocator, &src);
    if !ingest.errors.is_empty() {
        return None;
    }
    let Some(Statement::ExpressionStatement(stmt)) = ingest.program.body.last() else {
        return None;
    };
    let mut free: Vec<String> = ingest
        .semantic()
        .scoping()
        .root_unresolved_references()
        .keys()
        .map(|s| s.to_string())
        .filter(|s| !is_known_global(s))
        .collect();
    free.sort_unstable();
    let tables = SymbolTables::build(ingest.semantic());
    Some((
        factory_structural_hash(unparen(&stmt.expression), &tables)?,
        free,
    ))
}

/// The content key of one factory FUNCTION's text: its blurred canonical
/// hash with every free identifier that is not a known global declared
/// around it (so a reference to the rest of the bundle keys by position,
/// not by its release-specific spelling) and every known global verbatim.
/// None when the text does not parse as one function.
pub fn vendor_content_key(factory: &str) -> Option<String> {
    let (hash, free) = keyed(factory, &[])?;
    if free.is_empty() {
        return Some(hash);
    }
    keyed(factory, &free).map(|(hash, _)| hash)
}

/// The fresh side: every classified factory's content key, in record
/// order — its raw body with the unpack's `REQ(` → `require(` rewrite.
pub fn fresh_content_keys(
    code: &str,
    factories: &[FactoryRecord],
    require_var: Option<&str>,
) -> Vec<Option<String>> {
    crate::par::map_ordered(factories, |f| {
        let raw = &code[f.body_span.start as usize..f.body_span.end as usize];
        match require_var {
            Some(req) => vendor_content_key(&rewrite_require_calls(raw, req)),
            None => vendor_content_key(raw),
        }
    })
}

/// A byte splice list applied to `text[start..end)` (offsets absolute).
fn splice(text: &str, start: usize, end: usize, mut edits: Vec<(usize, usize, String)>) -> String {
    edits.sort_by_key(|e| e.0);
    let mut out = String::with_capacity(end - start);
    let mut cursor = start;
    for (s, e, r) in edits {
        if s < cursor {
            return String::new();
        }
        out.push_str(&text[cursor..s]);
        out.push_str(&r);
        cursor = e;
    }
    out.push_str(&text[cursor..end]);
    out
}

/// `const X = require(...)` declarators at the top of a program.
fn require_bound_names<'a>(program: &'a oxc_ast::ast::Program<'a>) -> HashSet<&'a str> {
    let mut out = HashSet::new();
    for stmt in &program.body {
        let Statement::VariableDeclaration(decl) = stmt else {
            continue;
        };
        for d in &decl.declarations {
            let (BindingPattern::BindingIdentifier(id), Some(Expression::CallExpression(call))) =
                (&d.id, &d.init)
            else {
                continue;
            };
            if matches!(&call.callee, Expression::Identifier(c) if c.name == "require") {
                out.insert(id.name.as_str());
            }
        }
    }
    out
}

/// The factory function of a relinked vendor file: the first argument of
/// `exports.f = <helper>(F)`.
fn relinked_factory<'a>(program: &'a oxc_ast::ast::Program<'a>) -> Option<&'a Expression<'a>> {
    program.body.iter().find_map(|stmt| {
        let Statement::ExpressionStatement(es) = stmt else {
            return None;
        };
        let Expression::AssignmentExpression(assign) = &es.expression else {
            return None;
        };
        let target = assign.left.as_member_expression()?;
        let oxc_ast::ast::MemberExpression::StaticMemberExpression(m) = target else {
            return None;
        };
        if !matches!(&m.object, Expression::Identifier(o) if o.name == "exports")
            || m.property.name != "f"
        {
            return None;
        }
        let Expression::CallExpression(call) = &assign.right else {
            return None;
        };
        let f = unparen(call.arguments.first()?.as_expression()?);
        matches!(
            f,
            Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_)
        )
        .then_some(f)
    })
}

/// The prior side: one vendor file's content key. A relinked file
/// (`exports.f = __commonJS(F)`, the runnable tree) has every
/// `<requireBound>.f` reference turned back into the bare identifier; an
/// unlinked one (the review tree, or a factory nothing references) IS the
/// raw body. None when neither shape is found.
pub fn prior_file_content_key(file_text: &str) -> Option<String> {
    let allocator = Allocator::default();
    let ingest = Ingest::parse_unambiguous(&allocator, file_text);
    if !ingest.errors.is_empty() {
        return None;
    }
    if let Some(f) = relinked_factory(ingest.program) {
        let bound = require_bound_names(ingest.program);
        let span = f.span();
        let (start, end) = (span.start as usize, span.end as usize);
        let mut edits = Vec::new();
        for node in ingest.semantic().nodes().iter() {
            let AstKind::StaticMemberExpression(m) = node.kind() else {
                continue;
            };
            let ms = m.span;
            if (ms.start as usize) < start || (ms.end as usize) > end || m.property.name != "f" {
                continue;
            }
            if let Expression::Identifier(o) = &m.object
                && bound.contains(o.name.as_str())
            {
                edits.push((ms.start as usize, ms.end as usize, o.name.to_string()));
            }
        }
        let text = splice(file_text, start, end, edits);
        return (!text.is_empty())
            .then(|| vendor_content_key(&text))
            .flatten();
    }
    match ingest.program.body.as_slice() {
        [Statement::ExpressionStatement(es)]
            if matches!(
                unparen(&es.expression),
                Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_)
            ) =>
        {
            let span = unparen(&es.expression).span();
            vendor_content_key(&file_text[span.start as usize..span.end as usize])
        }
        _ => None,
    }
}

/// One TS-era prior manifest entry, as the re-key reads it.
#[derive(Clone, Debug)]
pub struct TsEraEntry {
    pub name: String,
    /// The TS `structuralHash` (groups the prior's bundle-order ordinals).
    pub ts_hash: String,
    /// `hashOrdinal` (usize::MAX when absent — array order then).
    pub ordinal: usize,
    /// The vendor file's content key (None: unreadable / unrecognized).
    pub key: Option<String>,
}

/// What the re-key did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RekeyStats {
    /// Prior entries / entries whose vendor file yielded a content key.
    pub prior_entries: usize,
    pub prior_keyed: usize,
    /// Fresh structural-hash groups (and their factories) that joined a
    /// prior content group — the names the cascade may carry.
    pub groups_joined: usize,
    pub factories_joined: usize,
    /// Prior content groups whose bundle order is not recoverable (several
    /// TS hash groups under one key with differing names) — never carried.
    pub prior_groups_ambiguous: usize,
}

/// The re-keyed carry: the cascade's `prior_names` and the ordering pass's
/// prior entries, both on the FRESH structural hashes.
#[derive(Clone, Debug, Default)]
pub struct Rekeyed {
    pub names: HashMap<String, Vec<String>>,
    pub factories: Vec<PriorManifestEntry>,
    pub stats: RekeyStats,
}

/// Prefix of a prior entry's hash that joined nothing: it can never equal
/// a Rust structural hash (16 lowercase hex).
const UNJOINED: &str = "ts-era:";

/// Translate a TS-era prior onto the fresh structural hashes by content.
///
/// `fresh` is every classified factory in BUNDLE order: (structural hash,
/// content key). `prior` is the prior manifest's entries in its array
/// order (the order `order_by_prior_manifest` reads).
pub fn rekey_prior_by_content(fresh: &[(String, Option<String>)], prior: &[TsEraEntry]) -> Rekeyed {
    let mut stats = RekeyStats {
        prior_entries: prior.len(),
        prior_keyed: prior.iter().filter(|e| e.key.is_some()).count(),
        ..RekeyStats::default()
    };
    // Prior content groups, each resolved to its names in bundle order.
    let mut groups: BTreeMap<&str, Vec<(usize, &TsEraEntry)>> = BTreeMap::new();
    for (idx, e) in prior.iter().enumerate() {
        if let Some(k) = &e.key {
            groups.entry(k.as_str()).or_default().push((idx, e));
        }
    }
    let mut prior_names: HashMap<&str, Vec<String>> = HashMap::new();
    for (key, mut members) in groups {
        let one_ts_group = members
            .iter()
            .all(|(_, e)| e.ts_hash == members[0].1.ts_hash);
        let one_name = members.iter().all(|(_, e)| e.name == members[0].1.name);
        if !one_ts_group && !one_name {
            stats.prior_groups_ambiguous += 1;
            continue;
        }
        members.sort_by_key(|(idx, e)| (e.ordinal, *idx));
        prior_names.insert(key, members.iter().map(|(_, e)| e.name.clone()).collect());
    }
    // Fresh: per structural-hash group, its members' keys; per key, its size.
    let mut hash_groups: Vec<(&str, Vec<Option<&str>>)> = Vec::new();
    let mut at: HashMap<&str, usize> = HashMap::new();
    let mut key_count: HashMap<&str, usize> = HashMap::new();
    for (hash, key) in fresh {
        let slot = *at.entry(hash.as_str()).or_insert_with(|| {
            hash_groups.push((hash.as_str(), Vec::new()));
            hash_groups.len() - 1
        });
        hash_groups[slot].1.push(key.as_deref());
        if let Some(k) = key {
            *key_count.entry(k.as_str()).or_default() += 1;
        }
    }
    let mut out = Rekeyed::default();
    let mut key_to_hash: HashMap<&str, &str> = HashMap::new();
    for (hash, keys) in &hash_groups {
        let Some(Some(k)) = keys.first() else {
            continue;
        };
        let exact = keys.iter().all(|x| *x == Some(*k)) && key_count.get(k) == Some(&keys.len());
        if !exact {
            continue;
        }
        // The ORDER join is the class correspondence alone (the TS's pass 1
        // joined equal hashes whether or not a name carried); the NAME join
        // also needs the prior group's names in a known order.
        key_to_hash.insert(k, hash);
        if let Some(names) = prior_names.get(k) {
            out.names.insert(hash.to_string(), names.clone());
            stats.groups_joined += 1;
            stats.factories_joined += keys.len();
        }
    }
    out.factories = prior
        .iter()
        .map(|e| PriorManifestEntry {
            name: e.name.clone(),
            structural_hash: match e.key.as_deref().and_then(|k| key_to_hash.get(k)) {
                Some(h) => h.to_string(),
                None => format!("{UNJOINED}{}", e.ts_hash),
            },
        })
        .collect();
    out.stats = stats;
    out
}

#[cfg(test)]
#[path = "vendor_content/vendor_content_test.rs"]
mod vendor_content_test;
