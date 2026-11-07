//! APP vs VENDOR: every change-vs-noise KPI split by surface (TS:
//! `034/surfaces.ts`, exp170). Each split is a PARTITION of the frozen
//! field: `app + vendor + crossBoundary = old` (mints: `+ unattributed`);
//! a unit that changed surface between v-1 and v goes to `crossBoundary`,
//! once, as a move. No line is counted here: statement lines are the
//! statement owner's, file charges the composition's and the vendor
//! scorer's rows; this module only attributes.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use humanify_ag_model::js::locale_compare;

use super::compose::Tally;
use super::stmts::{FileStmt, Stmt, file_statements, line_count};
use super::text::{list_js, read_opt};
use super::vendor::VendorChurn;

/// The emitted tree's vendor root; everything else is app.
const VENDOR_ROOT: &str = "vendor/";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Side {
    App,
    Vendor,
}

pub fn side_of(file: &str) -> Side {
    if file.starts_with(VENDOR_ROOT) {
        Side::Vendor
    } else {
        Side::App
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Split3<T> {
    pub app: T,
    pub vendor: T,
    pub cross: T,
}

impl<T> Split3<T> {
    fn at(&mut self, side: Side, crossed: bool) -> &mut T {
        match (crossed, side) {
            (true, _) => &mut self.cross,
            (false, Side::App) => &mut self.app,
            (false, Side::Vendor) => &mut self.vendor,
        }
    }
}

/// The split ledger's fields the scorer reads (`.humanify/split-ledger.json`).
#[derive(Clone, Debug, Default)]
pub struct Ledger {
    /// Per declared name, the ordered file list of its declarations.
    pub name_to_files: Vec<(String, Vec<String>)>,
    pub order: Option<Vec<String>>,
    /// The PIPELINE's statement hashes (its own split key), bundle order.
    pub hashes: Option<Vec<String>>,
}

impl Ledger {
    pub fn read(path: &Path) -> Result<Ledger, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let v: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        let strings = |v: &serde_json::Value| -> Option<Vec<String>> {
            v.as_array().map(|a| {
                a.iter()
                    .map(|x| x.as_str().unwrap_or_default().to_string())
                    .collect()
            })
        };
        let name_to_files = v["nameToFiles"]
            .as_object()
            .map(|m| {
                m.iter()
                    .map(|(k, files)| (k.clone(), strings(files).unwrap_or_default()))
                    .collect()
            })
            .unwrap_or_default();
        Ok(Ledger {
            name_to_files,
            order: strings(&v["order"]),
            hashes: strings(&v["hashes"]),
        })
    }
}

// ---------------------------------------------------------------- statements

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatementKind {
    Clean,
    Noise,
    Novel,
}

/// THE classification behind clean / noise / novel: CLEAN iff the exact
/// text is present under its hash in the prior, NOISE if only the hash is,
/// NOVEL if the hash is new.
pub fn statement_kinds(fresh: &[Stmt], prior: &[Stmt]) -> Vec<StatementKind> {
    let mut by_hash: HashMap<&str, HashSet<&str>> = HashMap::new();
    for s in prior {
        by_hash.entry(&s.hash).or_default().insert(&s.text);
    }
    fresh
        .iter()
        .map(|s| match by_hash.get(s.hash.as_str()) {
            None => StatementKind::Novel,
            Some(texts) if texts.contains(s.text.as_str()) => StatementKind::Clean,
            Some(_) => StatementKind::Noise,
        })
        .collect()
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StatementCounts {
    pub total: usize,
    pub unchanged_clean: usize,
    pub unchanged_churned: usize,
    pub novel: usize,
    pub naming_noise_lines: usize,
    pub real_lines: usize,
}

impl StatementCounts {
    pub fn add(&mut self, kind: StatementKind, lines: usize) {
        self.total += 1;
        match kind {
            StatementKind::Clean => self.unchanged_clean += 1,
            StatementKind::Noise => {
                self.unchanged_churned += 1;
                self.naming_noise_lines += lines;
            }
            StatementKind::Novel => {
                self.novel += 1;
                self.real_lines += lines;
            }
        }
    }
}

/// The statement axis split by the file each statement is filed in, plus
/// the cross moves' direction counts. None when a ledger's order does not
/// cover its bundle one-to-one (refused, never approximated).
pub fn statement_split(
    fresh: &[Stmt],
    prior: &[Stmt],
    kinds: &[StatementKind],
    fresh_order: Option<&[String]>,
    prior_order: Option<&[String]>,
) -> Option<(Split3<StatementCounts>, usize, usize)> {
    let (fo, po) = (fresh_order?, prior_order?);
    if fo.len() != fresh.len() || po.len() != prior.len() {
        return None;
    }
    let mut prior_sides: HashMap<&str, HashSet<Side>> = HashMap::new();
    for (s, file) in prior.iter().zip(po) {
        prior_sides
            .entry(&s.hash)
            .or_default()
            .insert(side_of(file));
    }
    let mut out = Split3::<StatementCounts>::default();
    let (mut to_vendor, mut to_app) = (0, 0);
    for ((s, file), &kind) in fresh.iter().zip(fo).zip(kinds) {
        let side = side_of(file);
        let crossed = kind != StatementKind::Novel
            && prior_sides
                .get(s.hash.as_str())
                .is_some_and(|was| !was.contains(&side));
        if crossed {
            match side {
                Side::Vendor => to_vendor += 1,
                Side::App => to_app += 1,
            }
        }
        out.at(side, crossed).add(kind, s.lines);
    }
    Some((out, to_vendor, to_app))
}

// --------------------------------------------------------------------- names

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NameCounts {
    pub fresh_names: usize,
    pub novel_names: usize,
    pub same_name_moved_file: usize,
}

/// reloc / newName, split by each fresh name's home file
/// (`nameToFiles[name][0]`).
pub fn name_split(fresh: &Ledger, prior: &Ledger) -> Split3<NameCounts> {
    let prior_home: HashMap<&str, &str> = prior
        .name_to_files
        .iter()
        .filter_map(|(k, v)| Some((k.as_str(), v.first()?.as_str())))
        .collect();
    let mut out = Split3::<NameCounts>::default();
    for (name, files) in &fresh.name_to_files {
        let file = files.first().map_or("", String::as_str);
        let prior_file = prior_home.get(name.as_str()).copied();
        let side = side_of(file);
        let crossed = prior_file.is_some_and(|p| side_of(p) != side);
        let b = out.at(side, crossed);
        b.fresh_names += 1;
        match prior_file {
            None => b.novel_names += 1,
            Some(p) if p != file => b.same_name_moved_file += 1,
            Some(_) => {}
        }
    }
    out
}

// ------------------------------------------------------------------- relocSt

/// The statements the tree churn can compare: a hash occurring exactly
/// once in each ledger, with its prior and fresh file (ONE definition, for
/// the tree churn and its split).
pub fn unique_statement_pairs(fresh: &Ledger, prior: &Ledger) -> Vec<(String, String)> {
    let empty = Vec::new();
    let fh = fresh.hashes.as_ref().unwrap_or(&empty);
    let ph = prior.hashes.as_ref().unwrap_or(&empty);
    let fo = fresh.order.as_ref().unwrap_or(&empty);
    let po = prior.order.as_ref().unwrap_or(&empty);
    fn count(hs: &[String]) -> HashMap<&str, usize> {
        let mut m: HashMap<&str, usize> = HashMap::new();
        for h in hs {
            *m.entry(h.as_str()).or_default() += 1;
        }
        m
    }
    let (fc, pc) = (count(fh), count(ph));
    let prior_file: HashMap<&str, &str> = ph
        .iter()
        .enumerate()
        .filter(|(_, h)| pc[h.as_str()] == 1)
        .filter_map(|(i, h)| Some((h.as_str(), po.get(i)?.as_str())))
        .collect();
    fh.iter()
        .enumerate()
        .filter(|(_, h)| fc[h.as_str()] == 1)
        .filter_map(|(i, h)| {
            let p = prior_file.get(h.as_str())?;
            Some((p.to_string(), fo.get(i).cloned().unwrap_or_default()))
        })
        .collect()
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RelocCounts {
    pub statements_compared: usize,
    pub relocated_statements: usize,
}

pub fn reloc_split(fresh: &Ledger, prior: &Ledger) -> Split3<RelocCounts> {
    let mut out = Split3::<RelocCounts>::default();
    for (p, f) in unique_statement_pairs(fresh, prior) {
        let side = side_of(&f);
        let b = out.at(side, side_of(&p) != side);
        b.statements_compared += 1;
        if p != f {
            b.relocated_statements += 1;
        }
    }
    out
}

// --------------------------------------------------------------------- mints

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MintCounts {
    pub app: usize,
    pub vendor: usize,
    pub unattributed: usize,
}

/// The mint census attributed only as far as the bundle's binding counts
/// force it: k census copies, s app and v vendor bindings of a name give at
/// least k-v app and k-s vendor; the rest is unattributed, never guessed.
pub fn mint_split(census: &[String], fresh: &[Stmt], order: &[String]) -> MintCounts {
    let mut declared: HashMap<(Side, &str), usize> = HashMap::new();
    for (s, file) in fresh.iter().zip(order) {
        for n in &s.declared {
            *declared.entry((side_of(file), n.as_str())).or_default() += 1;
        }
    }
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for n in census {
        *counts.entry(n).or_default() += 1;
    }
    let mut out = MintCounts::default();
    for (name, k) in counts {
        let s = declared.get(&(Side::App, name)).copied().unwrap_or(0);
        let v = declared.get(&(Side::Vendor, name)).copied().unwrap_or(0);
        if s + v == 0 {
            out.unattributed += k;
            continue;
        }
        let app = k.min(k.saturating_sub(v));
        let vendor = (k - app).min(k.saturating_sub(s));
        out.app += app;
        out.vendor += vendor;
        out.unattributed += k - app - vendor;
    }
    out
}

// ---------------------------------------------------------------- tree lines

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AppLines {
    pub churn_lines: i64,
    pub churn_lines_clean: i64,
    pub real: i64,
    pub real_clean: i64,
    pub file_add_remove: i64,
    pub noise: i64,
    pub naming: i64,
    pub alias: i64,
    pub reorder: i64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VendorLines {
    pub churn_lines: i64,
    pub noise: i64,
    pub real: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrossMove {
    pub from: String,
    pub to: String,
    pub statements: usize,
    pub lines: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CrossLines {
    pub churn_lines: usize,
    pub app_side: usize,
    pub app_side_from_files: usize,
    pub vendor_side: usize,
    pub to_vendor: (usize, usize),
    pub to_app: (usize, usize),
    pub capped: usize,
    pub refused: usize,
    pub unparsed_vendor_files: usize,
    pub moves: Vec<CrossMove>,
}

/// The layout figures the split reads (the composition's tally, derived).
#[derive(Clone, Copy, Debug, Default)]
pub struct LayoutIn {
    pub churn_lines: i64,
    pub churn_lines_clean: i64,
    pub real: i64,
    pub real_clean: i64,
    pub file_add_remove: i64,
    pub noise: i64,
    pub naming: i64,
    pub alias: i64,
    pub reorder: i64,
}

/// A statement instance one side lost or gained, by tree-relative file.
#[derive(Clone, Debug)]
struct Instance {
    file: String,
    lines: usize,
}

#[derive(Default)]
struct Flows {
    out: BTreeMap<String, Vec<Instance>>,
    inn: BTreeMap<String, Vec<Instance>>,
}

/// Per hash, the instances `prior` has beyond `fresh` (out) and vice versa
/// (in); exact-text copies matched first, so the instances reported are
/// the ones that genuinely left or arrived.
fn file_flows(rel: &str, prior: &[FileStmt], fresh: &[FileStmt], flows: &mut Flows) {
    let group = |ss: &[FileStmt]| {
        let mut m: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (i, s) in ss.iter().enumerate() {
            m.entry(s.hash.clone()).or_default().push(i);
        }
        m
    };
    let (p, f) = (group(prior), group(fresh));
    let surplus = |a: &[usize], aa: &[FileStmt], b: Option<&Vec<usize>>, bb: &[FileStmt]| {
        let k = a.len().saturating_sub(b.map_or(0, Vec::len));
        if k == 0 {
            return Vec::new();
        }
        let mut texts: HashMap<&str, usize> = HashMap::new();
        for &j in b.into_iter().flatten() {
            *texts.entry(bb[j].text.as_str()).or_default() += 1;
        }
        let (mut unmatched, mut matched) = (Vec::new(), Vec::new());
        for &i in a {
            match texts.get_mut(aa[i].text.as_str()) {
                Some(n) if *n > 0 => {
                    *n -= 1;
                    matched.push(i);
                }
                _ => unmatched.push(i),
            }
        }
        unmatched.extend(matched);
        unmatched.truncate(k);
        unmatched
    };
    for (h, ss) in &p {
        for i in surplus(ss, prior, f.get(h), fresh) {
            flows.out.entry(h.clone()).or_default().push(Instance {
                file: rel.to_string(),
                lines: line_count(&prior[i].text),
            });
        }
    }
    for (h, ss) in &f {
        for i in surplus(ss, fresh, p.get(h), prior) {
            flows.inn.entry(h.clone()).or_default().push(Instance {
                file: rel.to_string(),
                lines: line_count(&fresh[i].text),
            });
        }
    }
}

/// What the composition charged one src/ file, and in which bucket.
#[derive(Clone, Copy, Debug)]
enum Cap {
    Real(usize),
    File(usize),
}

/// A differing src/ file's statements on both sides and its charge.
type FilePair = (Vec<FileStmt>, Vec<FileStmt>, Cap);

/// src/ flows over every file that differs, with each file's charge.
fn src_flows(
    prior_src: &Path,
    fresh_src: &Path,
    per_file_real: &HashMap<String, usize>,
) -> Result<(Flows, HashMap<String, Cap>), String> {
    let mut files: Vec<String> = list_js(prior_src)?;
    files.extend(list_js(fresh_src)?);
    files.sort();
    files.dedup();
    let parsed =
        humanify_ag_core::par::map_ordered(&files, |rel| -> Result<Option<FilePair>, String> {
            let prior = read_opt(&prior_src.join(rel))?;
            let fresh = read_opt(&fresh_src.join(rel))?;
            if prior == fresh {
                return Ok(None);
            }
            let stmts = |t: &Option<String>, side: &str| match t {
                Some(t) => file_statements(t, &format!("{side} src/{rel}")),
                None => Ok(Vec::new()),
            };
            let cap = match (&prior, &fresh) {
                (Some(_), Some(_)) => Cap::Real(per_file_real.get(rel).copied().unwrap_or(0)),
                (Some(t), None) | (None, Some(t)) => Cap::File(t.split('\n').count()),
                (None, None) => Cap::Real(0),
            };
            Ok(Some((
                stmts(&prior, "prior")?,
                stmts(&fresh, "fresh")?,
                cap,
            )))
        });
    let mut flows = Flows::default();
    let mut caps = HashMap::new();
    for (rel, r) in files.iter().zip(parsed) {
        if let Some((p, f, cap)) = r? {
            let tree_rel = format!("src/{rel}");
            file_flows(&tree_rel, &p, &f, &mut flows);
            caps.insert(tree_rel, cap);
        }
    }
    Ok((flows, caps))
}

/// vendor/ flows over the files the vendor scorer charged as REAL, with
/// each file's real charge (its rows) as the cap.
fn vendor_flows(
    prior_vendor: &Path,
    fresh_vendor: &Path,
    raw: &VendorChurn,
) -> (Flows, HashMap<String, usize>, usize) {
    let mut flows = Flows::default();
    let mut cap: HashMap<String, usize> = HashMap::new();
    let mut unparsed = 0;
    let parse = |dir: &Path, rel: &Option<String>| -> Option<Vec<FileStmt>> {
        match rel {
            None => Some(Vec::new()),
            Some(r) => {
                let text = std::fs::read_to_string(dir.join(r)).ok()?;
                file_statements(&text, r).ok()
            }
        }
    };
    for row in raw.file_rows.iter().filter(|r| r.bucket.is_real()) {
        let (Some(prior), Some(fresh)) = (
            parse(prior_vendor, &row.prior),
            parse(fresh_vendor, &row.fresh),
        ) else {
            unparsed += 1;
            continue;
        };
        let key = format!(
            "vendor/{}",
            row.fresh
                .as_ref()
                .or(row.prior.as_ref())
                .cloned()
                .unwrap_or_default()
        );
        let total = cap.get(&key).copied().unwrap_or(0) + row.lines;
        cap.insert(key.clone(), total);
        if let (Some(p), Some(f)) = (&row.prior, &row.fresh) {
            if p != f {
                cap.insert(format!("vendor/{p}"), total);
            }
            file_flows(&key, &prior, &fresh, &mut flows);
        } else {
            file_flows(&key, &prior, &fresh, &mut flows);
        }
    }
    (flows, cap, unparsed)
}

/// The smallest statement a content pairing trusts: a one-line shape is
/// the same hash in unrelated code. Refused lines are reported.
const MIN_PAIR_LINES: usize = 2;

/// Net flow of each hash from one surface to the other, paired in order.
fn pair_flows(from: &Flows, to: &Flows, mut on_pair: impl FnMut(&Instance, &Instance)) -> usize {
    let mut refused = 0;
    for (h, gone) in &from.out {
        let Some(arrived) = to.inn.get(h) else {
            continue;
        };
        let net = |xs: Option<&Vec<Instance>>| xs.map_or(0, Vec::len);
        let k = gone
            .len()
            .saturating_sub(net(from.inn.get(h)))
            .min(arrived.len().saturating_sub(net(to.out.get(h))));
        for i in 0..k {
            if gone[i].lines < MIN_PAIR_LINES || arrived[i].lines < MIN_PAIR_LINES {
                refused += gone[i].lines + arrived[i].lines;
            } else {
                on_pair(&gone[i], &arrived[i]);
            }
        }
    }
    refused
}

/// The tree-line split: statements that crossed the src/ <-> vendor/
/// boundary paired by content (net flow per hash) and taken out of BOTH
/// surfaces' real charge into crossBoundary, each side's take capped at
/// what that file was charged, so the partition is exact.
#[allow(clippy::too_many_arguments)]
pub fn boundary_lines(
    prior_src: &Path,
    fresh_src: &Path,
    prior_vendor: &Path,
    fresh_vendor: &Path,
    per_file_real: &HashMap<String, usize>,
    layout: &LayoutIn,
    vendor_raw: &VendorChurn,
) -> Result<(AppLines, VendorLines, CrossLines), String> {
    let (src, src_cap) = src_flows(prior_src, fresh_src, per_file_real)?;
    let (ven, ven_cap, unparsed) = vendor_flows(prior_vendor, fresh_vendor, vendor_raw);
    let mut app_take: BTreeMap<String, usize> = BTreeMap::new();
    let mut vendor_take: BTreeMap<String, usize> = BTreeMap::new();
    let mut moves: BTreeMap<(String, String), CrossMove> = BTreeMap::new();
    let mut cross = CrossLines {
        unparsed_vendor_files: unparsed,
        ..CrossLines::default()
    };
    let mut record = |a: &Instance, b: &Instance, to_vendor: bool, cross: &mut CrossLines| {
        let (app, ven) = if to_vendor { (a, b) } else { (b, a) };
        *app_take.entry(app.file.clone()).or_default() += app.lines;
        *vendor_take.entry(ven.file.clone()).or_default() += ven.lines;
        let dir = if to_vendor {
            &mut cross.to_vendor
        } else {
            &mut cross.to_app
        };
        dir.0 += 1;
        dir.1 += a.lines + b.lines;
        let m = moves
            .entry((a.file.clone(), b.file.clone()))
            .or_insert_with(|| CrossMove {
                from: a.file.clone(),
                to: b.file.clone(),
                statements: 0,
                lines: 0,
            });
        m.statements += 1;
        m.lines += a.lines + b.lines;
    };
    let mut c = CrossLines::default();
    let refused_a = pair_flows(&src, &ven, |a, b| record(a, b, true, &mut c));
    let refused_b = pair_flows(&ven, &src, |a, b| record(a, b, false, &mut c));
    cross.to_vendor = c.to_vendor;
    cross.to_app = c.to_app;
    cross.refused = refused_a + refused_b;
    let (mut app_real, mut app_file, mut wanted) = (0, 0, 0);
    for (file, lines) in &app_take {
        wanted += lines;
        match src_cap.get(file).copied().unwrap_or(Cap::Real(0)) {
            Cap::File(n) => app_file += (*lines).min(n),
            Cap::Real(n) => app_real += (*lines).min(n),
        }
    }
    let mut vendor_real = 0;
    for (file, lines) in &vendor_take {
        wanted += lines;
        vendor_real += (*lines).min(ven_cap.get(file).copied().unwrap_or(0));
    }
    cross.app_side = app_real + app_file;
    cross.app_side_from_files = app_file;
    cross.vendor_side = vendor_real;
    cross.churn_lines = cross.app_side + cross.vendor_side;
    cross.capped = wanted - cross.churn_lines;
    let mut list: Vec<CrossMove> = moves.into_values().collect();
    list.sort_by(|x, y| {
        y.lines
            .cmp(&x.lines)
            .then_with(|| locale_compare(&x.from, &y.from))
    });
    cross.moves = list;
    let l = layout;
    let (side, real_take, file_take) = (cross.app_side as i64, app_real as i64, app_file as i64);
    let app = AppLines {
        churn_lines: l.churn_lines - side,
        churn_lines_clean: l.churn_lines_clean - side,
        real: l.real - real_take,
        real_clean: l.real_clean - real_take,
        file_add_remove: l.file_add_remove - file_take,
        noise: l.noise,
        naming: l.naming,
        alias: l.alias,
        reorder: l.reorder,
    };
    let vendor = VendorLines {
        churn_lines: vendor_raw.total_lines as i64 - vendor_real as i64,
        noise: vendor_raw.noise_lines as i64,
        real: vendor_raw.real_lines as i64 - vendor_real as i64,
    };
    Ok((app, vendor, cross))
}

/// The layout figures from a composition tally and the line-level views.
pub fn layout_in(t: &Tally, build_constant_lines: usize) -> LayoutIn {
    let i = |n: usize| n as i64;
    let noise = i(t.naming + t.alias + t.reorder);
    let churn = noise + i(t.real + t.file_add_remove);
    let soft = i(build_constant_lines + t.spelling_identical_lines);
    LayoutIn {
        churn_lines: churn,
        churn_lines_clean: churn - soft,
        real: i(t.real),
        real_clean: i(t.real) - soft,
        file_add_remove: i(t.file_add_remove),
        noise,
        naming: i(t.naming),
        alias: i(t.alias),
        reorder: i(t.reorder),
    }
}

#[cfg(test)]
#[path = "surfaces_test.rs"]
mod surfaces_test;
