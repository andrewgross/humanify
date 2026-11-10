//! Scorer 15's COPY CREDIT (exp214): a claim the truth calls wrong is
//! credited — in its own column, never as `right` — when the code is
//! provably the CLAIMED package's own code and the truth's package
//! provably ships a copy of it (a renamed fork, a re-published or bundled
//! copy, a vendored file). The truth is the corpus app's source map, so it
//! names the package that shipped the bytes; when that package copied
//! them, a claim naming the package they came from is not a mistake.
//!
//! Similar-looking code is never enough. Every credit needs ALL of:
//!
//! 1. the unit has a statement print, and the label's library index holds
//!    that print in a file of the CLAIMED package (a unit row, owner =
//!    the claimed package);
//! 2. the same file's text — the index's literal table, read from the
//!    claimed package's PUBLISHED files, not from a source map — holds at
//!    least `OWN_SHARE` of the unit's literals (strings, property names,
//!    numbers, regexes), and the unit has `MIN_EVIDENCE` distinctive ones.
//!    A print alone blurs strings: two generated loaders with different
//!    package names print alike, and their literals tell them apart;
//! 3. the truth's own file (the corpus map's `sourcesContent`) holds at
//!    least `TRUTH_SHARE` of them too — the bytes really are that file's
//!    (otherwise the label is a map defect, `defect15`, not a copy);
//! 4. the claimed package does not ship the TRUTH's code instead (its file
//!    is not under a directory named for the truth's package, and its
//!    registry manifest does not depend on the truth's package);
//! 5. a LINK says the truth's package carries a copy of the claimed one:
//!    - `dependency`: the truth's package@version declares the claimed
//!      package (a dist that inlines its dependency);
//!    - `fork`: the truth's name extends the claimed one's
//!      (`hyperdht` → `hyperdht-hugin`, `rambda` → `rambdax`,
//!      `react-is` → `@jest/react-is-18`), and the truth's registry
//!      manifest is known and does not develop against the claimed
//!      package (an extension such as `micromark-extension-frontmatter`
//!      lists micromark as a devDependency: no fork);
//!    - `vendored`: the truth's file sits in a `vendor/` (or alike)
//!      directory under the claimed package's name
//!      (`lib/vendor/import-meta-resolve.js`);
//!    - `same-file`: the truth's file and the claimed package's file share
//!      a non-generic basename, and the claimed file holds at least
//!      `SAME_FILE_SHARE` of the truth file's distinctive literals (both
//!      ship the same file: VS Code's `path.js` in prompt-tsx and monaco).

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::io::BufRead;
use std::path::{Path, PathBuf};

use humanify_ag_core::libindex::format::{LiteralFileRow, LiteralRow};
use humanify_ag_core::libindex::table::{LITERALS_FILE, LiteralTable, PACKAGES_FILE, Source};
use serde_json::Value;

use super::defect15::{Lits, distinctive};
use super::index::Index;

/// The unit's literals the claimed package's file must hold.
pub const OWN_SHARE: f64 = 0.9;
/// The unit's literals the truth's own file must hold.
pub const TRUTH_SHARE: f64 = 0.8;
/// Distinctive literals a unit needs before any credit.
pub const MIN_EVIDENCE: usize = 3;
/// `same-file`: the claimed file holds this share of the truth file's
/// distinctive literals, of which there are at least `SAME_FILE_EVIDENCE`.
pub const SAME_FILE_SHARE: f64 = 0.8;
pub const SAME_FILE_EVIDENCE: usize = 20;
/// `fork`: the claimed package's unscoped name has at least this many
/// characters (a short stem extends into unrelated names).
pub const FORK_STEM: usize = 5;
const VENDOR_DIRS: [&str; 7] = [
    "vendor",
    "vendored",
    "vendors",
    "third_party",
    "third-party",
    "node_modules",
    "bundled",
];
const GENERIC_STEMS: [&str; 12] = [
    "index", "main", "lib", "mod", "bundle", "browser", "node", "cjs", "esm", "dist", "module",
    "umd",
];
const DEP_KEYS: [&str; 3] = ["dependencies", "optionalDependencies", "peerDependencies"];
const WRAPPER: [&str; 4] = ["P:exports", "P:__esModule", "S:__esModule", "P:default"];

/// The library index's literal table and its file provenance.
pub struct LiteralIndex {
    table: LiteralTable,
    packages: Vec<String>,
    /// (owner, file) -> literal file ids.
    by_file: BTreeMap<(String, String), Vec<u32>>,
    /// owner -> literal file ids.
    by_owner: BTreeMap<String, Vec<u32>>,
}

impl LiteralIndex {
    /// The literal table of index `dir`; None when the index has none.
    pub fn open(dir: &Path) -> Result<Option<LiteralIndex>, String> {
        let bin = dir.join(LITERALS_FILE);
        let rows = dir.join("literals.files.jsonl");
        if !bin.is_file() || !rows.is_file() {
            return Ok(None);
        }
        let ptext = std::fs::read_to_string(dir.join(PACKAGES_FILE))
            .map_err(|e| format!("{}: {e}", dir.join(PACKAGES_FILE).display()))?;
        let packages: Vec<String> =
            serde_json::from_str(&ptext).map_err(|e| format!("{PACKAGES_FILE}: {e}"))?;
        let fh = std::fs::File::open(&rows).map_err(|e| format!("{}: {e}", rows.display()))?;
        let mut files = Vec::new();
        for line in std::io::BufReader::new(fh).lines() {
            let line = line.map_err(|e| format!("{}: {e}", rows.display()))?;
            if line.trim().is_empty() {
                continue;
            }
            let r: LiteralFileRow =
                serde_json::from_str(&line).map_err(|e| format!("{}: {e}", rows.display()))?;
            files.push((r.owner, r.file));
        }
        let table = LiteralTable::open(Source::open(&bin)?)?;
        Ok(Some(LiteralIndex::with(table, packages, files)))
    }

    /// A literal index over `rows` (file id = row position), in memory.
    pub fn from_rows(rows: &[LiteralRow]) -> LiteralIndex {
        let mut packages: Vec<String> = rows.iter().map(|r| r.owner.clone()).collect();
        packages.sort();
        packages.dedup();
        let table = LiteralTable::open(Source::Memory(LiteralTable::encode(rows, &packages)))
            .expect("an encoded literal table opens");
        let files = rows
            .iter()
            .map(|r| (r.owner.clone(), r.file.clone()))
            .collect();
        LiteralIndex::with(table, packages, files)
    }

    fn with(table: LiteralTable, packages: Vec<String>, files: Vec<(String, String)>) -> Self {
        let mut by_file: BTreeMap<(String, String), Vec<u32>> = BTreeMap::new();
        let mut by_owner: BTreeMap<String, Vec<u32>> = BTreeMap::new();
        for (i, k) in files.into_iter().enumerate() {
            by_owner.entry(k.0.clone()).or_default().push(i as u32);
            by_file.entry(k).or_default().push(i as u32);
        }
        LiteralIndex {
            table,
            packages,
            by_file,
            by_owner,
        }
    }

    /// The most of `lits` any one published file of `owner` holds.
    pub fn owner_best_held(&self, owner: &str, lits: &[&str]) -> usize {
        let Some(ids) = self.by_owner.get(owner) else {
            return 0;
        };
        let want: Vec<u32> = lits.iter().filter_map(|l| self.table.resolve(l)).collect();
        ids.iter()
            .map(|&f| {
                let have = self.table.file_literals(f);
                want.iter()
                    .filter(|w| have.binary_search(w).is_ok())
                    .count()
            })
            .max()
            .unwrap_or(0)
    }

    /// The most of `lits` any one version of (owner, file) holds.
    pub fn best_held(&self, owner: &str, file: &str, lits: &[&str]) -> usize {
        let Some(ids) = self.by_file.get(&(owner.to_string(), file.to_string())) else {
            return 0;
        };
        let want: Vec<u32> = lits.iter().filter_map(|l| self.table.resolve(l)).collect();
        ids.iter()
            .filter(|&&f| {
                self.packages
                    .get(self.table.file_owner(f) as usize)
                    .map(String::as_str)
                    == Some(owner)
            })
            .map(|&f| {
                let have = self.table.file_literals(f);
                want.iter()
                    .filter(|w| have.binary_search(w).is_ok())
                    .count()
            })
            .max()
            .unwrap_or(0)
    }
}

/// The npm registry-manifest cache (`library npm`'s), read once per name.
pub struct Registry {
    dir: PathBuf,
    cache: RefCell<BTreeMap<String, Option<Value>>>,
}

impl Registry {
    pub fn new(dir: &Path) -> Registry {
        Registry {
            dir: dir.to_path_buf(),
            cache: RefCell::new(BTreeMap::new()),
        }
    }

    /// `name@version`'s manifest (`latest` when no version); None when
    /// the cache does not hold it or npm had none.
    pub fn manifest(&self, name: &str, version: Option<&str>) -> Option<Value> {
        let v = version.unwrap_or("latest");
        let key = format!("{name}@{v}");
        if let Some(hit) = self.cache.borrow().get(&key) {
            return hit.clone();
        }
        let path = super::npm::metadata_path(&self.dir, name, v);
        let doc = std::fs::read_to_string(path)
            .ok()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            .filter(|d| d.get("__error").is_none());
        self.cache.borrow_mut().insert(key, doc.clone());
        doc
    }
}

/// Does `manifest` declare `dep` (any dependency kind, bundled included)?
pub fn declares(manifest: &Value, dep: &str) -> bool {
    DEP_KEYS
        .iter()
        .any(|k| manifest.get(*k).and_then(|d| d.get(dep)).is_some())
        || ["bundleDependencies", "bundledDependencies"]
            .iter()
            .any(|k| {
                manifest
                    .get(*k)
                    .and_then(Value::as_array)
                    .is_some_and(|a| a.iter().any(|x| x.as_str() == Some(dep)))
            })
}

/// Does `manifest` list `dep` as a devDependency?
fn develops_against(manifest: &Value, dep: &str) -> bool {
    manifest
        .get("devDependencies")
        .and_then(|d| d.get(dep))
        .is_some()
}

fn scope(p: &str) -> Option<&str> {
    p.strip_prefix('@')
        .and_then(|r| r.split_once('/'))
        .map(|x| x.0)
}

fn unscoped(p: &str) -> &str {
    match scope(p) {
        Some(_) => p.split_once('/').map_or(p, |x| x.1),
        None => p,
    }
}

/// A file name without its extensions (`rough.esm.js` -> `rough`).
fn stem(seg: &str) -> &str {
    seg.split('.').next().unwrap_or(seg)
}

/// `fork`: `truth`'s name extends `claimed`'s (same scope, or `claimed`
/// unscoped), and `claimed`'s stem is long enough to mean something.
pub fn fork_name(claimed: &str, truth: &str) -> bool {
    let (c, t) = (unscoped(claimed), unscoped(truth));
    claimed != truth
        && c.chars().count() >= FORK_STEM
        && t.starts_with(c)
        && (scope(claimed).is_none() || scope(claimed) == scope(truth))
}

/// `vendored`: `truth_file` has a vendor-like directory followed by a
/// segment named for `claimed` (its unscoped name, or its full scoped
/// name over two segments).
pub fn vendored_path(truth_file: &str, claimed: &str) -> bool {
    let segs: Vec<&str> = truth_file.split('/').collect();
    segs.iter().enumerate().any(|(i, s)| {
        VENDOR_DIRS.contains(s)
            && match (segs.get(i + 1), segs.get(i + 2)) {
                (Some(a), b) if scope(claimed).is_some() && a.starts_with('@') => {
                    b.is_some_and(|b| format!("{a}/{}", stem(b)) == claimed)
                }
                (Some(a), _) => stem(a) == unscoped(claimed),
                _ => false,
            }
    })
}

/// The reverse direction: the claimed package's file sits under a
/// directory (or is a file) named for the truth's package — the claimed
/// package ships the TRUTH's code (chalk's `vendor/supports-color`).
pub fn ships_truth(claimed_file: &str, truth: &str) -> bool {
    let t = unscoped(truth);
    claimed_file.split('/').any(|s| s == t || stem(s) == t)
}

fn basename(f: &str) -> &str {
    f.rsplit('/').next().unwrap_or(f)
}

/// `same-file`'s name half: one basename, and not a generic one.
pub fn same_basename(truth_file: &str, claimed_file: &str) -> bool {
    let (a, b) = (basename(truth_file), basename(claimed_file));
    a == b && !GENERIC_STEMS.contains(&stem(a))
}

/// What a credit rests on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Link {
    Dependency,
    Fork,
    Vendored,
    SameFile,
}

impl Link {
    pub fn name(self) -> &'static str {
        match self {
            Link::Dependency => "dependency",
            Link::Fork => "fork",
            Link::Vendored => "vendored",
            Link::SameFile => "same-file",
        }
    }
}

/// A credited claim's evidence.
#[derive(Clone, Debug, PartialEq)]
pub struct Credit {
    pub link: Link,
    /// The claimed package's file holding the unit.
    pub own_file: String,
    pub own_share: f64,
    pub truth_share: f64,
    pub evidence: usize,
}

/// The verdict on one wrong claim.
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    Credited(Credit),
    /// Why not (a fixed reason, for the listing's tally).
    Refused(&'static str),
}

/// One wrong claim, as the scorer sees it.
pub struct Claim<'a> {
    pub claimed: &'a str,
    pub truth: &'a str,
    pub truth_version: Option<&'a str>,
    pub truth_file: Option<&'a str>,
    /// The unit's whole print, when it has one.
    pub print: Option<u64>,
    pub unit_lits: Option<&'a Lits>,
    /// The truth's voted files' literal sets (None: unreadable).
    pub truth_lits: Option<&'a [Lits]>,
}

/// What a credit reads: the label's index, its literal table and the
/// registry cache.
pub struct Judge<'a> {
    pub ix: &'a Index,
    pub lits: &'a LiteralIndex,
    pub registry: &'a Registry,
}

fn held_share(lits: &[&str], sets: &[Lits]) -> f64 {
    if lits.is_empty() {
        return 0.0;
    }
    let n = lits
        .iter()
        .filter(|l| sets.iter().any(|s| s.contains(**l)))
        .count();
    n as f64 / lits.len() as f64
}

impl Judge<'_> {
    /// The claimed package's files that hold print `h` (index unit rows).
    fn own_files(&self, h: u64, claimed: &str) -> Vec<String> {
        let mut out: Vec<String> = self
            .ix
            .rows_of(h)
            .filter(|r| self.ix.name(r.owner) == claimed)
            .map(|r| self.ix.name(r.file).to_string())
            .collect();
        out.sort();
        out.dedup();
        out
    }

    /// The claimed files holding at least `OWN_SHARE` of `lits`, with
    /// their shares (in file order).
    fn holding(&self, files: &[String], claimed: &str, lits: &[&str]) -> Vec<(String, f64)> {
        files
            .iter()
            .map(|f| {
                let n = self.lits.best_held(claimed, f, lits);
                (f.clone(), n as f64 / lits.len() as f64)
            })
            .filter(|x| x.1 >= OWN_SHARE)
            .collect()
    }

    /// The link that makes the truth's package a carrier of the claimed
    /// one's code, if any (checked in `Link` order), and the claimed file
    /// it rests on (`same-file`: the file shipped by both; else the
    /// first holding file).
    fn link<'o>(&self, c: &Claim, own: &'o [(String, f64)]) -> Option<(Link, &'o (String, f64))> {
        let first = own.first()?;
        let manifest = self.registry.manifest(c.truth, c.truth_version);
        if manifest.as_ref().is_some_and(|m| declares(m, c.claimed)) {
            return Some((Link::Dependency, first));
        }
        // A fork does not build against its upstream: a truth package that
        // develops against the claimed one (`micromark-extension-*` and
        // micromark) is an extension, and an unknown manifest proves nothing.
        if fork_name(c.claimed, c.truth)
            && manifest
                .as_ref()
                .is_some_and(|m| !develops_against(m, c.claimed))
        {
            return Some((Link::Fork, first));
        }
        let tf = c.truth_file?;
        if vendored_path(tf, c.claimed) {
            return Some((Link::Vendored, first));
        }
        let tl: Vec<&str> = c
            .truth_lits?
            .iter()
            .flat_map(distinctive)
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        if tl.len() < SAME_FILE_EVIDENCE {
            return None;
        }
        own.iter()
            .find(|(f, _)| {
                same_basename(tf, f)
                    && self.lits.best_held(c.claimed, f, &tl) as f64
                        >= SAME_FILE_SHARE * tl.len() as f64
            })
            .map(|o| (Link::SameFile, o))
    }

    /// The verdict on one wrong claim (the module doc's five steps).
    pub fn judge(&self, c: &Claim) -> Outcome {
        let Some(h) = c.print else {
            return Outcome::Refused("no-print");
        };
        let Some(unit) = c.unit_lits else {
            return Outcome::Refused("unit-unreadable");
        };
        let all: Vec<&str> = unit
            .iter()
            .map(String::as_str)
            .filter(|l| !WRAPPER.contains(l))
            .collect();
        let evidence = distinctive(unit).len();
        if evidence < MIN_EVIDENCE {
            return Outcome::Refused("few-literals");
        }
        let files = self.own_files(h, c.claimed);
        let own = self.holding(&files, c.claimed, &all);
        if own.is_empty() {
            return Outcome::Refused(if files.is_empty() {
                "print-not-in-claimed"
            } else {
                "literals-not-in-claimed"
            });
        }
        let Some(tl) = c.truth_lits else {
            return Outcome::Refused("truth-file-unreadable");
        };
        let truth_share = held_share(&all, tl);
        if truth_share < TRUTH_SHARE {
            return Outcome::Refused("truth-file-lacks");
        }
        if own.iter().any(|(f, _)| ships_truth(f, c.truth))
            || self
                .registry
                .manifest(c.claimed, None)
                .is_some_and(|m| declares(&m, c.truth))
        {
            return Outcome::Refused("claimed-ships-truth");
        }
        match self.link(c, &own) {
            Some((link, (own_file, own_share))) => Outcome::Credited(Credit {
                link,
                own_file: own_file.clone(),
                own_share: *own_share,
                truth_share,
                evidence,
            }),
            None => Outcome::Refused("no-link"),
        }
    }
}

#[cfg(test)]
#[path = "copy_credit_test.rs"]
mod copy_credit_test;
