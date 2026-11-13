//! One name per binding of the linked program. Every chunk had its own
//! top-level scope; the linked program has one, so a top-level name keeps
//! its spelling only when nothing else can see it differently: no earlier
//! chunk claimed it, no chunk reads it as a global, it is not one of the
//! CommonJS wrapper's parameters, and no chunk that imports the binding
//! declares the name in a nested scope (where the import's references
//! would be captured). Anything else gets a fresh name that no chunk
//! uses anywhere.

use std::collections::{BTreeMap, BTreeSet};

use super::analyse::{Chunk, Imported, Spec};
use super::graph::{Order, Target, resolve_sym};

/// The CommonJS wrapper's parameters (and two names a binding can't take).
const RESERVED: [&str; 7] = [
    "exports",
    "require",
    "module",
    "__filename",
    "__dirname",
    "arguments",
    "eval",
];

/// The bindings one external import declaration becomes, Bun's CommonJS
/// shape: named and namespace imports read `ns = require(m)` (a named
/// import's references become `ns.<name>`), a default import reads
/// `.default` of `__toESM(require(m))` — `{default: require(m)}` when no
/// `__toESM` is written before the chunk.
pub(super) struct External {
    pub ns: Option<String>,
    pub default: Option<String>,
    /// The `__toESM` helper's linked name, when the default uses it.
    pub to_esm: Option<String>,
}

pub(super) struct Names {
    /// Every binding's linked name — for an external import's local, the
    /// expression its references become.
    pub of: BTreeMap<Target, String>,
    /// Per (chunk, import declaration index), its external bindings.
    pub external: BTreeMap<(usize, usize), External>,
    /// Per chunk (written position), its init's name.
    pub init: BTreeMap<usize, String>,
    pub esm_helper: String,
    /// The chunk whose `__toESM` the external default imports call.
    pub to_esm_chunk: Option<usize>,
    /// Bun's `__export` (defines a namespace object's getters).
    pub export_helper: String,
    pub renamed: usize,
    /// `var x = import.meta.require` declarators dropped: every reference
    /// to x, in every chunk, reads the wrapper's `require` instead.
    pub require_aliases: BTreeSet<(usize, oxc_semantic::SymbolId)>,
    taken: BTreeSet<String>,
    claimed: BTreeSet<String>,
}

impl Names {
    /// `base` when nothing uses it, else `base$k` for the first free k.
    fn fresh(&mut self, base: &str) -> String {
        let mut candidate = base.to_string();
        let mut k = 0;
        while self.taken.contains(&candidate) || self.claimed.contains(&candidate) {
            k += 1;
            candidate = format!("{base}${k}");
        }
        self.claimed.insert(candidate.clone());
        candidate
    }

    pub fn name(&self, t: Target) -> &str {
        &self.of[&t]
    }
}

/// Which chunks reference each binding through an import.
fn importers(chunks: &[Chunk]) -> Result<BTreeMap<Target, BTreeSet<usize>>, String> {
    let mut out: BTreeMap<Target, BTreeSet<usize>> = BTreeMap::new();
    for (c, chunk) in chunks.iter().enumerate() {
        for s in chunk.syms.iter().filter(|s| s.is_import) {
            out.entry(resolve_sym(chunks, c, s.id)?)
                .or_default()
                .insert(c);
        }
    }
    Ok(out)
}

pub(super) fn assign(
    chunks: &[Chunk],
    order: &Order,
    needs_init: &[bool],
    needs_ns: &[bool],
) -> Result<Names, String> {
    let mut globals = BTreeSet::new();
    let mut taken = BTreeSet::new();
    for c in chunks {
        globals.extend(c.unresolved.iter().cloned());
        taken.extend(c.unresolved.iter().cloned());
        taken.extend(c.nested_names.iter().cloned());
        taken.extend(c.syms.iter().map(|s| s.name.clone()));
    }
    let importers = importers(chunks)?;
    let mut names = Names {
        of: BTreeMap::new(),
        external: BTreeMap::new(),
        init: BTreeMap::new(),
        esm_helper: String::new(),
        to_esm_chunk: None,
        export_helper: String::new(),
        renamed: 0,
        require_aliases: require_aliases(chunks)?,
        taken,
        claimed: BTreeSet::new(),
    };
    for &c in &order.chunks {
        for s in &chunks[c].syms {
            let target = resolve_sym(chunks, c, s.id)?;
            if s.is_import || target != Target::Sym(c, s.id) {
                continue; // an import: named where it is defined, or below
            }
            if names.require_aliases.contains(&(c, s.id)) {
                names.of.insert(target, "require".to_string());
                continue;
            }
            let free = !names.claimed.contains(&s.name)
                && !globals.contains(&s.name)
                && !RESERVED.contains(&s.name.as_str())
                && importers.get(&target).is_none_or(|by| {
                    by.iter()
                        .all(|&i| !chunks[i].nested_names.contains(&s.name))
                });
            let name = if free {
                names.claimed.insert(s.name.clone());
                s.name.clone()
            } else {
                names.renamed += 1;
                names.fresh(&s.name)
            };
            names.of.insert(target, name);
        }
    }
    externals(chunks, order, &mut names);
    names.esm_helper = names.fresh("__esm");
    names.export_helper = names.fresh("__export");
    for (k, &c) in order.chunks.iter().enumerate() {
        if needs_init[c] {
            let n = names.fresh(&format!("__chunkInit{k}"));
            names.init.insert(c, n);
        }
        if needs_ns[c] {
            let n = names.fresh(&format!("__chunkNs{k}"));
            names.of.insert(Target::Ns(c), n);
        }
        if chunks[c]
            .exports
            .iter()
            .any(|(_, t)| matches!(t, super::analyse::ExportTarget::DefaultExpr))
        {
            let n = names.fresh(&format!("__chunkDefault{k}"));
            names.of.insert(Target::Default(c), n);
        }
    }
    Ok(names)
}

/// The first `__toESM` helper written, with its position.
fn to_esm_helper(chunks: &[Chunk], order: &Order, names: &Names) -> Option<(String, usize)> {
    order.chunks.iter().enumerate().find_map(|(pos, &c)| {
        let helper = chunks[c].to_esm?;
        names
            .of
            .get(&Target::Sym(c, helper))
            .map(|n| (n.clone(), pos))
    })
}

/// Name every external import declaration's bindings ([`External`]) and
/// give each of its locals the expression its references become.
fn externals(chunks: &[Chunk], order: &Order, names: &mut Names) {
    let helper = to_esm_helper(chunks, order, names);
    names.to_esm_chunk = helper.as_ref().map(|(_, pos)| order.chunks[*pos]);
    for (pos, &c) in order.chunks.iter().enumerate() {
        for (k, decl) in chunks[c].imports.iter().enumerate() {
            if !matches!(decl.spec, Spec::External(_)) || decl.specifiers.is_empty() {
                continue;
            }
            let local_name = |sym| {
                chunks[c]
                    .syms
                    .iter()
                    .find(|s| s.id == sym)
                    .map_or("m", |s| s.name.as_str())
            };
            let mut ext = External {
                ns: None,
                default: None,
                to_esm: None,
            };
            for (imported, local) in &decl.specifiers {
                let expr = match imported {
                    Imported::Default => {
                        default_of(&mut ext, names, local_name(*local), &helper, pos)
                    }
                    Imported::Name(n) if n == "default" => {
                        default_of(&mut ext, names, local_name(*local), &helper, pos)
                    }
                    Imported::Namespace => ns_of(&mut ext, names, local_name(*local)),
                    Imported::Name(n) => format!(
                        "{}{}",
                        ns_of(&mut ext, names, local_name(*local)),
                        member(n)
                    ),
                };
                names.of.insert(Target::Sym(c, *local), expr);
            }
            names.external.insert((c, k), ext);
        }
    }
}

fn ns_of(ext: &mut External, names: &mut Names, base: &str) -> String {
    if ext.ns.is_none() {
        ext.ns = Some(names.fresh(base));
    }
    ext.ns.clone().unwrap_or_default()
}

fn default_of(
    ext: &mut External,
    names: &mut Names,
    base: &str,
    helper: &Option<(String, usize)>,
    pos: usize,
) -> String {
    if ext.default.is_none() {
        ext.default = Some(names.fresh(base));
        ext.to_esm = helper
            .as_ref()
            .filter(|(_, at)| *at < pos)
            .map(|(n, _)| n.clone());
    }
    format!("{}.default", ext.default.clone().unwrap_or_default())
}

/// `.name`, or `["name"]` for a name that is not an identifier.
fn member(name: &str) -> String {
    let ident = name
        .chars()
        .next()
        .is_some_and(|c| c.is_alphabetic() || c == '_' || c == '$')
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '$');
    if ident {
        format!(".{name}")
    } else {
        format!("[{}]", serde_json::to_string(name).unwrap_or_default())
    }
}

/// The `var x = import.meta.require` bindings that may become the
/// wrapper's `require`: every reference to them, in the declaring chunk
/// and in every chunk that imports them, reads `require` unshadowed.
fn require_aliases(chunks: &[Chunk]) -> Result<BTreeSet<(usize, oxc_semantic::SymbolId)>, String> {
    let mut candidates: BTreeMap<Target, bool> = BTreeMap::new();
    for (c, chunk) in chunks.iter().enumerate() {
        for st in &chunk.stmts {
            let super::analyse::StmtKind::Var(ds) = &st.kind else {
                continue;
            };
            for sym in ds
                .iter()
                .filter(|d| d.require_alias)
                .flat_map(|d| d.names.iter())
            {
                let safe = chunk.syms.iter().any(|s| s.id == *sym && s.require_safe);
                candidates.insert(Target::Sym(c, *sym), safe);
            }
        }
    }
    for (c, chunk) in chunks.iter().enumerate() {
        for s in chunk.syms.iter().filter(|s| s.is_import) {
            if let Some(safe) = candidates.get_mut(&resolve_sym(chunks, c, s.id)?) {
                *safe &= s.require_safe;
            }
        }
    }
    Ok(candidates
        .into_iter()
        .filter_map(|(t, safe)| match t {
            Target::Sym(c, s) if safe => Some((c, s)),
            _ => None,
        })
        .collect())
}
