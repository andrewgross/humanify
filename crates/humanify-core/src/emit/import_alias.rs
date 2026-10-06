//! Which alias does a file use for a module it imports — the ONE owner of
//! the `const <alias> = require("./m.js")` name in the runnable tree.
//!
//! Each IMPORTING file chooses its own alias for each module it requires
//! (finding #88, 2026-10-06; decision A of the alias investigation). The
//! ladder is the one the emit always had, run per importer:
//!
//! 0. the alias THIS importer wrote for that module in the prior tree (read
//!    from the prior file's own require line), when it is still legal, not
//!    a name the file uses, and no other module this file imports wants it;
//! 1. the basename's camelCase, widening up the path, then the sanitized
//!    path, then the path-hashed form (never collides);
//!
//! a name two modules want at the same rung goes to neither (both widen),
//! and a name is free only if it is a legal binding, unclaimed IN THIS
//! FILE, not a wrapper parameter, and not a name THIS FILE uses.
//!
//! What changed (the WIDENING class, exp057 §2): the alias used to be one
//! per module tree-wide, so a name in ANY importer — or another file
//! anywhere with the same basename — widened the alias in EVERY importer.
//! One shadowing local in `parse-tool-rule.js` made `validatePathVal`
//! `srcValidatePathVal` in all 91 importers on 2.1.216. Now a clash in one
//! importer moves only that importer's alias.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use sha2::{Digest, Sha256};

use crate::rename::validated::target::is_valid_rename_target;

/// importer path → module path → the alias the importer's require line
/// binds (read from a prior tree).
pub type PriorImportAliases = HashMap<String, HashMap<String, String>>;

/// importer file id → module file id → alias.
pub type ImportAliases = BTreeMap<usize, BTreeMap<usize, String>>;

/// `camelFromSegments`: `a-b/c-d` → `aBCD`.
fn camel_from_segments(segments: &[&str]) -> String {
    let joined = segments.join("-");
    let mut out = String::new();
    for (i, w) in joined
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .enumerate()
    {
        let mut chars = w.chars();
        let first = chars.next().expect("non-empty word");
        if i == 0 {
            out.push(first.to_ascii_lowercase());
        } else {
            out.push(first.to_ascii_uppercase());
        }
        out.push_str(chars.as_str());
    }
    out
}

/// `nsCandidates(file)`: the basename's camelCase, widening up the path,
/// then the sanitized path, then the path-hashed form (never collides).
fn ns_candidates(file: &str) -> Vec<String> {
    let stem = file.strip_suffix(".js").unwrap_or(file);
    let parts: Vec<&str> = stem.split('/').filter(|p| !p.is_empty()).collect();
    let mut out = Vec::new();
    for take in 1..=parts.len() {
        out.push(camel_from_segments(&parts[parts.len() - take..]));
    }
    // `file.replace(/[^A-Za-z0-9_$]/g, "_")` — per UTF-16 code unit.
    let sanitized: String = file
        .chars()
        .flat_map(|c| {
            let keep = c.is_ascii_alphanumeric() || c == '_' || c == '$';
            let n = if keep { 1 } else { c.len_utf16() };
            std::iter::repeat_n(if keep { c } else { '_' }, n)
        })
        .collect();
    let hash = Sha256::digest(file.as_bytes());
    let hex: String = hash.iter().map(|b| format!("{b:02x}")).collect();
    out.push(sanitized.clone());
    out.push(format!("{sanitized}_{}", &hex[..8]));
    out
}

/// Where an alias can collide: per importing file, the modules it
/// requires and the names it uses (binding or reference positions).
pub(super) struct ImportScope {
    /// The wrapper's parameters: taken in every file.
    pub always_taken: HashSet<String>,
    pub names_by_file: HashMap<usize, HashSet<String>>,
    /// importer → the modules it reads or writes across files.
    pub imports: BTreeMap<usize, BTreeSet<usize>>,
}

/// One importer's choice, rung by rung.
struct Importer<'a> {
    taken: &'a HashSet<String>,
    always_taken: &'a HashSet<String>,
    claimed: HashSet<String>,
    chosen: BTreeMap<usize, String>,
}

impl Importer<'_> {
    fn is_free(&self, name: &str) -> bool {
        is_valid_rename_target(name)
            && !self.claimed.contains(name)
            && !self.taken.contains(name)
            && !self.always_taken.contains(name)
    }

    /// Give each pending module its `want` when no other pending module
    /// wants the same name and it is free; the rest stay pending.
    fn rung(&mut self, pending: Vec<usize>, want: impl Fn(usize) -> Option<String>) -> Vec<usize> {
        let wants: Vec<(usize, Option<String>)> =
            pending.into_iter().map(|m| (m, want(m))).collect();
        let mut count: HashMap<&str, usize> = HashMap::new();
        for (_, w) in &wants {
            if let Some(w) = w {
                *count.entry(w.as_str()).or_insert(0) += 1;
            }
        }
        let mut granted: Vec<(usize, String)> = Vec::new();
        let mut next = Vec::new();
        for (m, w) in &wants {
            match w {
                Some(w) if !w.is_empty() && count[w.as_str()] == 1 && self.is_free(w) => {
                    granted.push((*m, w.clone()));
                }
                _ => next.push(*m),
            }
        }
        for (m, w) in granted {
            self.claimed.insert(w.clone());
            self.chosen.insert(m, w);
        }
        next
    }
}

/// Every importer's alias for every module it imports.
pub(super) fn build_import_aliases(
    files: &[String],
    scope: &ImportScope,
    prior: Option<&PriorImportAliases>,
) -> Result<ImportAliases, String> {
    let empty = HashSet::new();
    let mut candidates: HashMap<usize, Vec<String>> = HashMap::new();
    let mut out = ImportAliases::new();
    for (&importer, modules) in &scope.imports {
        for &m in modules {
            candidates
                .entry(m)
                .or_insert_with(|| ns_candidates(&files[m]));
        }
        let mut chooser = Importer {
            taken: scope.names_by_file.get(&importer).unwrap_or(&empty),
            always_taken: &scope.always_taken,
            claimed: HashSet::new(),
            chosen: BTreeMap::new(),
        };
        let mut pending: Vec<usize> = modules.iter().copied().collect();
        if let Some(carried) = prior.and_then(|p| p.get(&files[importer])) {
            pending = chooser.rung(pending, |m| carried.get(&files[m]).cloned());
        }
        let max_tier = pending
            .iter()
            .map(|m| candidates[m].len())
            .max()
            .unwrap_or(0);
        for tier in 0..max_tier {
            if pending.is_empty() {
                break;
            }
            pending = chooser.rung(pending, |m| candidates[&m].get(tier).cloned());
        }
        if let Some(&m) = pending.first() {
            return Err(format!(
                "runnable emit: no free namespace variable for {} in {}",
                files[m], files[importer]
            ));
        }
        out.insert(importer, chooser.chosen);
    }
    Ok(out)
}

/// The module path a relative `require` in `importer` resolves to (the
/// inverse of [`super::paths::compute_relative_import_path`]); None when
/// it climbs out of the tree.
fn resolve_relative(importer: &str, rel: &str) -> Option<String> {
    let mut parts: Vec<&str> = importer.split('/').collect();
    parts.pop();
    for seg in rel.split('/') {
        match seg {
            "." | "" => {}
            ".." => {
                parts.pop()?;
            }
            s => parts.push(s),
        }
    }
    Some(parts.join("/"))
}

/// The (module, alias) pairs an emitted file's require lines bind — both
/// forms the emit writes: the header `const a = require("./m.js");` and
/// the on-first-use `const a = new Proxy({}, { get: (_, k) =>
/// require("./m.js")[k], … });`. Only relative requires count.
pub fn parse_require_aliases(importer: &str, text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in text.lines() {
        let Some(rest) = line.strip_prefix("const ") else {
            continue;
        };
        let Some((alias, rhs)) = rest.split_once(" = ") else {
            continue;
        };
        let rel = if let Some(r) = rhs.strip_prefix("require(\"") {
            r.strip_suffix("\");").filter(|r| !r.contains('"'))
        } else if let Some(r) = rhs.strip_prefix("new Proxy({}, { get: (_, k) => require(\"") {
            r.split_once("\")[k]").map(|(r, _)| r)
        } else {
            None
        };
        let Some(rel) = rel.filter(|r| r.starts_with("./") || r.starts_with("../")) else {
            continue;
        };
        if let Some(module) = resolve_relative(importer, rel) {
            out.push((module, alias.to_string()));
        }
    }
    out
}

/// The prior tree's aliases for this run's files: each file's own require
/// lines, read through `read` (path relative to the tree root → text).
pub fn read_prior_import_aliases(
    files: &[String],
    read: impl Fn(&str) -> Option<String>,
) -> PriorImportAliases {
    let mut out = PriorImportAliases::new();
    for f in files {
        let Some(text) = read(f) else {
            continue;
        };
        let mut by_module = HashMap::new();
        for (module, alias) in parse_require_aliases(f, &text) {
            by_module.entry(module).or_insert(alias);
        }
        if !by_module.is_empty() {
            out.insert(f.clone(), by_module);
        }
    }
    out
}

#[cfg(test)]
mod import_alias_test;
