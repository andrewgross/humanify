//! Scorer 15's source-map DEFECT check (exp214): a truth label is a
//! defect when the file the corpus bundle's own source map files a unit
//! under cannot contain that unit — the unit's distinctive literals (the
//! strings, property names and regexes no minifier renames, as the library
//! index's literal extractor reads them) are almost all absent from that
//! file's text. Such a unit's true package is unknown, so scorer 15 takes
//! its bytes out of BOTH sides of precision and recall (`<defect>`) and
//! lists it, instead of charging a claim against a label that is wrong.
//!
//! The check reads only frozen inputs: the corpus bundle, and the text
//! of the labelled file as the corpus map's `sourcesContent` holds it.
//! Both sides go through ONE extractor (`humanify_ag_core::libindex::
//! literals`), which decodes escapes and folds constant concatenations,
//! so a JSON table escaped in one place and raw in the other still agrees.
//! A file that does not parse, or a unit with too little evidence, is
//! never a defect: the check can only take a label away when the text
//! proves it wrong.

use std::collections::{BTreeMap, BTreeSet};

use humanify_ag_core::libindex::literals::{LiteralMode, code_literals, json_literals};

use super::pypath;
use super::truth::Truth;

/// A unit needs at least this many distinctive literals to be judged.
pub const MIN_EVIDENCE: usize = 8;
/// A defect: the labelled files hold at most this share of them.
pub const LACK: f64 = 0.2;
/// A distinctive literal's value has at least this many characters.
const MIN_VALUE_CHARS: usize = 4;
/// What the bundler's CommonJS / ESM interop writes into every module
/// (`module.exports`, `__esModule`, `.default`): no source file need hold it.
const WRAPPER: [&str; 4] = ["P:exports", "P:__esModule", "S:__esModule", "P:default"];

/// A literal set (the extractor's `S:` / `P:` / `N:` / `R:` strings).
pub type Lits = BTreeSet<String>;

/// The literal set of a bundle unit's text; a body statement that only
/// parses inside a function (`return …`) is read inside one.
pub fn unit_literals(code: &str) -> Option<Lits> {
    code_literals(code, LiteralMode::Bundled).or_else(|| {
        code_literals(
            &format!("(function(){{\n{code}\n}});"),
            LiteralMode::Bundled,
        )
    })
}

/// The literal set of a source file's text (a `.json` file as JSON).
pub fn file_literals(path: &str, text: &str) -> Option<Lits> {
    if path.ends_with(".json") {
        json_literals(text).or_else(|| code_literals(text, LiteralMode::Source))
    } else {
        code_literals(text, LiteralMode::Source)
    }
}

/// The distinctive literals: strings, property names and regexes of 4+
/// characters (and the extractor's hashed long values). Numbers are not
/// evidence: every file holds small ones; neither is the interop the
/// bundler wraps every module in.
pub fn distinctive(lits: &Lits) -> Vec<&str> {
    lits.iter()
        .map(String::as_str)
        .filter(|l| !WRAPPER.contains(l))
        .filter(|l| {
            if l.starts_with("S#") || l.starts_with("P#") || l.starts_with("R#") {
                return true;
            }
            match l.split_once(':') {
                Some(("S" | "P" | "R", v)) => v.chars().count() >= MIN_VALUE_CHARS,
                _ => false,
            }
        })
        .collect()
}

/// How a unit's evidence fared against the labelled file(s).
#[derive(Clone, Debug, PartialEq)]
pub struct Evidence {
    /// Distinctive literals of the unit.
    pub evidence: usize,
    /// How many of them the labelled file(s) hold.
    pub held: usize,
    /// Up to five that the labelled file(s) lack, in order.
    pub absent: Vec<String>,
}

impl Evidence {
    pub fn share(&self) -> f64 {
        if self.evidence == 0 {
            0.0
        } else {
            self.held as f64 / self.evidence as f64
        }
    }

    /// The rule: enough evidence, and the label's files hold almost none.
    pub fn is_defect(&self) -> bool {
        self.evidence >= MIN_EVIDENCE && self.share() <= LACK
    }
}

/// The unit's distinctive literals against the union of the labelled
/// files' literal sets.
pub fn judge(unit: &Lits, files: &[&Lits]) -> Evidence {
    judge_with(unit, |l| files.iter().any(|f| f.contains(l)))
}

/// The unit's distinctive literals against any notion of "held".
pub fn judge_with(unit: &Lits, holds: impl Fn(&str) -> bool) -> Evidence {
    let ev = distinctive(unit);
    let mut held = 0;
    let mut absent = Vec::new();
    for l in &ev {
        if holds(l) {
            held += 1;
        } else if absent.len() < 5 {
            absent.push((*l).to_string());
        }
    }
    Evidence {
        evidence: ev.len(),
        held,
        absent,
    }
}

/// The sources of the corpus map that vote `label` inside [s, e), most
/// bytes first (ties: lower source index first).
pub fn label_sources(truth: &Truth, s: i64, e: i64, label: &str) -> Vec<usize> {
    let segs = &truth.segments;
    let mut k = segs.partition_point(|x| x.0 <= s).saturating_sub(1);
    let mut by: BTreeMap<usize, i64> = BTreeMap::new();
    while k < segs.len() && segs[k].0 < e {
        let a = segs[k].0.max(s);
        let b = if k + 1 < segs.len() { segs[k + 1].0 } else { e }.min(e);
        if let Ok(si) = usize::try_from(segs[k].1)
            && b > a
            && truth.sources.get(si).is_some_and(|x| x.label == label)
        {
            *by.entry(si).or_default() += b - a;
        }
        k += 1;
    }
    let mut out: Vec<(usize, i64)> = by.into_iter().collect();
    out.sort_by(|x, y| y.1.cmp(&x.1).then(x.0.cmp(&y.0)));
    out.into_iter().map(|x| x.0).collect()
}

/// The value a literal spells (`S:abc` -> `abc`; a hashed one: None).
fn literal_value(l: &str) -> Option<&str> {
    l.split_once(':').map(|x| x.1)
}

/// What a package's files hold: the union of the literal sets of those
/// that parse, and every file's raw text (searched for a literal's value:
/// a lenient "held" — a `require("./x.node")` specifier the source-mode
/// extractor drops, or a file that does not parse, can only keep a label,
/// never take one away).
#[derive(Default)]
pub struct PackageText {
    pub lits: Lits,
    raw: Vec<String>,
}

impl PackageText {
    pub fn holds(&self, l: &str) -> bool {
        self.lits.contains(l)
            || literal_value(l).is_some_and(|v| self.raw.iter().any(|t| t.contains(v)))
    }
}

/// Installed files larger than this are not read.
const MAX_FILE: u64 = 16 << 20;
const EXTS: [&str; 4] = [".js", ".cjs", ".mjs", ".json"];

/// The installed package directory a source path lies in (its innermost
/// `node_modules/<name>` or `node_modules/@scope/<name>`).
pub fn package_root(path: &str) -> Option<String> {
    let k = path
        .rfind("/node_modules/")
        .map(|k| k + 1)
        .or_else(|| path.starts_with("node_modules/").then_some(0))?;
    let rest = &path[k + "node_modules/".len()..];
    let parts: Vec<&str> = rest.split('/').collect();
    let n = if parts[0].starts_with('@') { 2 } else { 1 };
    (parts.len() > n).then(|| {
        format!(
            "{}{}",
            &path[..k + "node_modules/".len()],
            parts[..n].join("/")
        )
    })
}

/// A package directory's own code and JSON files (not its nested
/// `node_modules`), sorted.
fn package_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<std::path::PathBuf> = read.flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if p.is_symlink() || name == "node_modules" {
            continue;
        }
        if p.is_dir() {
            package_files(&p, out);
        } else if EXTS.iter().any(|x| name.ends_with(x))
            && p.metadata().is_ok_and(|m| m.len() <= MAX_FILE)
        {
            out.push(p);
        }
    }
}

/// One entry's texts: the bundle and the map's sources, with each
/// source's literal set read once (None: no text, or it does not parse).
pub struct EntryTexts {
    pub bundle: Vec<u8>,
    /// The directory the map's source paths are relative to (the bundle's
    /// build directory on this host).
    base: String,
    /// installed package root -> its files' texts (None: not on disk).
    installed: BTreeMap<String, Option<Vec<PackageText>>>,
    paths: Vec<String>,
    contents: Vec<Option<String>>,
    lits: BTreeMap<usize, Option<Lits>>,
}

impl EntryTexts {
    pub fn new(
        bundle: Vec<u8>,
        base: &str,
        paths: Vec<String>,
        contents: Vec<Option<String>>,
    ) -> EntryTexts {
        EntryTexts {
            bundle,
            base: base.to_string(),
            installed: BTreeMap::new(),
            paths,
            contents,
            lits: BTreeMap::new(),
        }
    }

    /// The bundle text of [s, e) (byte offsets), lossily decoded.
    pub fn unit_text(&self, s: i64, e: i64) -> String {
        let n = self.bundle.len() as i64;
        let (a, b) = (s.clamp(0, n) as usize, e.clamp(0, n) as usize);
        String::from_utf8_lossy(&self.bundle[a..b.max(a)]).into_owned()
    }

    pub fn path(&self, si: usize) -> &str {
        self.paths.get(si).map_or("", String::as_str)
    }

    /// Source `si` has (non-empty) text in the map.
    pub fn has_text(&self, si: usize) -> bool {
        matches!(self.contents.get(si), Some(Some(t)) if !t.is_empty())
    }

    pub fn sources(&self) -> usize {
        self.contents.len()
    }

    /// Source `si`'s literal set, read once.
    pub fn source_lits(&mut self, si: usize) -> Option<&Lits> {
        if !self.lits.contains_key(&si) {
            let lits = match self.contents.get(si) {
                Some(Some(text)) if !text.is_empty() => file_literals(self.path(si), text),
                _ => None,
            };
            self.lits.insert(si, lits);
        }
        self.lits.get(&si).and_then(Option::as_ref)
    }

    /// The literal sets of `sources`; None when any of them has no text
    /// or does not parse (the check cannot judge then).
    pub fn all_lits(&mut self, sources: &[usize]) -> Option<Vec<Lits>> {
        let mut out = Vec::with_capacity(sources.len());
        for &si in sources {
            out.push(self.source_lits(si)?.clone());
        }
        Some(out)
    }

    /// The union of what the sources `sources` hold (`PackageText`).
    pub fn package_text(&mut self, sources: &[usize]) -> PackageText {
        let mut out = PackageText::default();
        for &si in sources {
            if let Some(l) = self.source_lits(si) {
                out.lits.extend(l.iter().cloned());
            }
            if let Some(Some(t)) = self.contents.get(si) {
                out.raw.push(t.clone());
            }
        }
        out
    }

    /// The installed package directories of `sources` (on this host).
    pub fn roots(&self, sources: &[usize]) -> Vec<String> {
        let mut out: Vec<String> = sources
            .iter()
            .filter_map(|&si| {
                package_root(&pypath::normpath(&pypath::join(&self.base, self.path(si))))
            })
            .collect();
        out.sort();
        out.dedup();
        out
    }

    /// The most of `unit`'s evidence one installed file under `roots`
    /// holds (a share); None when none of the roots is on disk.
    pub fn installed_best(&mut self, roots: &[String], unit: &Lits) -> Option<f64> {
        let mut best: Option<f64> = None;
        for root in roots {
            if !self.installed.contains_key(root) {
                let dir = std::path::Path::new(root);
                let files = dir.is_dir().then(|| {
                    let mut ps = Vec::new();
                    package_files(dir, &mut ps);
                    ps.iter()
                        .filter_map(|p| {
                            let text = std::fs::read_to_string(p).ok()?;
                            let path = p.to_string_lossy();
                            Some(match file_literals(&path, &text) {
                                Some(lits) => PackageText {
                                    lits,
                                    raw: vec![text],
                                },
                                None => PackageText {
                                    lits: Lits::new(),
                                    raw: vec![text],
                                },
                            })
                        })
                        .collect()
                });
                self.installed.insert(root.clone(), files);
            }
            if let Some(files) = &self.installed[root] {
                let b = files
                    .iter()
                    .map(|f| judge_with(unit, |l| f.holds(l)).share())
                    .fold(0.0, f64::max);
                best = Some(best.unwrap_or(0.0).max(b));
            }
        }
        best
    }

    /// The other source of the map holding the most of `unit`'s evidence
    /// (path, held share), for the defect listing; None when none holds
    /// half of it.
    pub fn best_holder(&mut self, unit: &Lits, not: &[usize]) -> Option<(String, f64)> {
        let mut best: Option<(usize, f64)> = None;
        for si in 0..self.sources() {
            if not.contains(&si) {
                continue;
            }
            let Some(l) = self.source_lits(si) else {
                continue;
            };
            let share = judge(unit, &[l]).share();
            if share >= 0.5 && best.is_none_or(|b| share > b.1) {
                best = Some((si, share));
            }
        }
        best.map(|(si, share)| (self.path(si).to_string(), share))
    }
}

#[cfg(test)]
#[path = "defect15_test.rs"]
mod defect15_test;
