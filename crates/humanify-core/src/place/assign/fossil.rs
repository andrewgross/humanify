//! Fossil-guided statement assignment — TS `src/split/fossil-assign.ts`
//! (exp070): emit the file layout the bundle RECORDS instead of
//! approximating one.
//!
//! - each fossil module ([`crate::twins::fossil`]) becomes ONE file;
//! - a module matched to the prior release ([`super::fossil_match`])
//!   inherits its prior file path VERBATIM;
//! - an unmatched module is named by its MODULE NAME, read back from its
//!   lazy-init wrapper (`initColorUtils` → `color-utils.js`): the naming
//!   stage's module step (`naming::module_names`) asked the model what the
//!   file is for and named the wrapper so, and the wrapper's name is the
//!   single record of it ([`module_stem_of_wrapper`]). A wrapper that does
//!   not carry one (a barrel's, a refused answer's) falls back to the
//!   mechanical [`module_stem`] — never a guess and never a position. The
//!   same stems name the inferred folders. (The warm-hop mint namer this
//!   replaced went away with it, 2026-10-06.)
//! - the eager zone (entry tail, no fossil) goes to `src/index.js`.
//!
//! Folder hierarchy is INFERRED from the import DAG ([`infer_fossil_placements`]):
//! barrels, dominant-importer nesting, co-importer groups, importer
//! consensus, then small-folder collapse — each pass sequential in module
//! order, exactly as the TS mutates its placement array.

use std::collections::{HashMap, HashSet};

use humanify_model::js::{cmp_utf16, utf16_len};
use serde_json::Value;

use super::fossil_match::{FossilSignature, match_fossil_modules};
use crate::place::ledger::{FossilLedgerModule, StableSplitLedger};
use crate::place::stems::{module_stem_of_wrapper, stem_of};
use crate::place::trail::{PlacementTrail, TrailEntry};
use crate::twins::fossil::{FossilExtract, FossilModule, declared_names, extract_fossil_modules};

/// The eager zone's file: the program's entry file (exp074).
pub const FOSSIL_BOOTSTRAP_FILE: &str = "src/index.js";

/// Files-per-folder below which a folder dissolves into its parent (exp076).
pub const MIN_FOLDER_FILES: usize = 2;
/// Share of a file's importers that must agree on a folder to move it.
const CONSENSUS: f64 = 0.5;
/// Consensus passes: a moved file becomes evidence for its own importers.
const CONSENSUS_PASSES: usize = 3;

/// `FossilFolderSignal`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FossilFolderSignal {
    Barrel,
    Anchor,
    DominantImporter,
    CoImporter,
    Flat,
}

/// `FossilPlacement`: a module's proposed folder + file before collision
/// resolution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FossilPlacement {
    pub folder: String,
    pub file: String,
    pub signal: FossilFolderSignal,
}

/// `FossilAssignment.stats` (the counters the run log prints).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FossilStats {
    pub modules: usize,
    pub inherited_files: usize,
    pub fresh_named_files: usize,
    /// Fresh files named by their module name (read back from the
    /// wrapper), not the mechanical stem.
    pub module_named_files: usize,
    pub eager_statements: usize,
    pub match_tiers: Vec<(String, usize)>,
    pub hoisted_singletons: usize,
}

/// `FossilAssignment`.
#[derive(Clone, Debug, Default)]
pub struct FossilAssignment {
    /// File per wrapper statement, parallel to the body.
    pub assignment: Vec<String>,
    /// Every fresh module with its final file — the next hop's match targets.
    pub fossil_modules: Vec<FossilLedgerModule>,
    pub stats: FossilStats,
}

/// A JS `Set<string>` in insertion order (the `used` path set: its order
/// feeds the mint namer's sibling lists).
#[derive(Default)]
struct OrderedSet {
    order: Vec<String>,
    set: HashSet<String>,
}

impl OrderedSet {
    fn has(&self, s: &str) -> bool {
        self.set.contains(s)
    }
    fn add(&mut self, s: &str) {
        if self.set.insert(s.to_string()) {
            self.order.push(s.to_string());
        }
    }
}

fn node_type(v: &Value) -> &str {
    v.get("type").and_then(Value::as_str).unwrap_or("")
}

/// oxc keeps `ParenthesizedExpression` nodes Babel drops (lesson 2).
fn unparen(v: &Value) -> &Value {
    let mut cur = v;
    while node_type(cur) == "ParenthesizedExpression" {
        match cur.get("expression") {
            Some(next) => cur = next,
            None => break,
        }
    }
    cur
}

/// `moduleStem`: the first hoisted declaration's stem (function/class lead
/// the segment by construction), else the first declared var that is not
/// the init itself (the init def is always the LAST declaration).
pub fn module_stem(module: &FossilModule, body: &[Value]) -> String {
    for &i in &module.statements {
        let stmt = &body[i];
        if matches!(node_type(stmt), "FunctionDeclaration" | "ClassDeclaration")
            && let Some(name) = stmt
                .get("id")
                .and_then(|id| id.get("name"))
                .and_then(Value::as_str)
        {
            return stem_of(name);
        }
    }
    // A library import never names a file (finding #91): `pathModule23 =
    // require("path")` says nothing about what the module is for, and its
    // number is the bundle's, not the file's.
    let imports = library_import_names(module, body);
    if let Some(first) = module.declared.iter().find(|n| !imports.contains(*n)) {
        return stem_of(first);
    }
    match module.hashes.first() {
        Some(h) => format!("module-{}", crate::detect::js_text::js_prefix(h, 8)),
        None => "module-empty".to_string(),
    }
}

/// The names the module's statements bind to a library import — a
/// declarator or plain `=` assignment whose value is `require("<package>")`
/// (`naming::plumbing::is_library_specifier`). Any such write counts: a
/// stem skip only needs "this name came from an import".
fn library_import_names(module: &FossilModule, body: &[Value]) -> HashSet<String> {
    let mut out = HashSet::new();
    for &i in &module.statements {
        collect_library_imports(&body[i], &mut out);
    }
    out
}

fn collect_library_imports(v: &Value, out: &mut HashSet<String>) {
    match v {
        Value::Array(items) => items.iter().for_each(|i| collect_library_imports(i, out)),
        Value::Object(o) => {
            let target = match node_type(v) {
                "VariableDeclarator" => o.get("id").zip(o.get("init")),
                "AssignmentExpression"
                    if o.get("operator").and_then(Value::as_str) == Some("=") =>
                {
                    o.get("left").zip(o.get("right"))
                }
                _ => None,
            };
            if let Some((id, value)) = target
                && node_type(id) == "Identifier"
                && is_library_require(value)
                && let Some(name) = id.get("name").and_then(Value::as_str)
            {
                out.insert(name.to_string());
            }
            o.values().for_each(|c| collect_library_imports(c, out));
        }
        _ => {}
    }
}

/// `require("<package>")` in the ESTree JSON.
fn is_library_require(v: &Value) -> bool {
    let v = unparen(v);
    let callee_is_require = v
        .get("callee")
        .map(unparen)
        .is_some_and(|c| node_type(c) == "Identifier" && c.get("name") == Some(&"require".into()));
    let spec = match v
        .get("arguments")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
    {
        Some([arg]) if node_type(arg) == "Literal" => arg.get("value").and_then(Value::as_str),
        _ => None,
    };
    node_type(v) == "CallExpression"
        && callee_is_require
        && spec.is_some_and(crate::naming::plumbing::is_library_specifier)
}

/// Statements of a Babel block body: the ESTree JSON puts directive
/// prologues in `body` (a `directive` field); Babel keeps them apart.
fn babel_block_statements(block: &Value) -> Vec<&Value> {
    block
        .get("body")
        .and_then(Value::as_array)
        .map(|b| b.iter().filter(|s| s.get("directive").is_none()).collect())
        .unwrap_or_default()
}

/// A zero-arg call of a plain identifier, as an expression statement.
fn is_init_call_statement(s: &Value) -> bool {
    if node_type(s) != "ExpressionStatement" {
        return false;
    }
    let Some(e) = s.get("expression").map(unparen) else {
        return false;
    };
    node_type(e) == "CallExpression"
        && e.get("arguments")
            .and_then(Value::as_array)
            .is_some_and(Vec::is_empty)
        && e.get("callee")
            .map(unparen)
            .is_some_and(|c| node_type(c) == "Identifier")
}

/// `initBodyIsOnlyInitCalls` (fossil-map.ts): the first declarator whose
/// init is a call taking a block-bodied function decides — a non-empty
/// body of nothing but zero-arg init calls (a barrel's re-export index).
pub fn init_body_is_only_init_calls(stmt: &Value) -> bool {
    if node_type(stmt) != "VariableDeclaration" {
        return false;
    }
    let declarations = stmt
        .get("declarations")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    for d in declarations {
        let Some(init) = d.get("init").filter(|v| !v.is_null()).map(unparen) else {
            continue;
        };
        if node_type(init) != "CallExpression" {
            continue;
        }
        let Some(func) = init
            .get("arguments")
            .and_then(Value::as_array)
            .and_then(|a| a.first())
            .map(unparen)
        else {
            continue;
        };
        let is_fn = matches!(
            node_type(func),
            "ArrowFunctionExpression" | "FunctionExpression"
        );
        let Some(block) = func
            .get("body")
            .filter(|b| node_type(b) == "BlockStatement")
        else {
            continue;
        };
        if !is_fn {
            continue;
        }
        let stmts = babel_block_statements(block);
        return !stmts.is_empty() && stmts.iter().all(|s| is_init_call_statement(s));
    }
    false
}

/// `[...new Set(list)]` over module indexes.
fn dedup_usize(list: &[usize]) -> Vec<usize> {
    let mut seen = HashSet::new();
    list.iter().copied().filter(|x| seen.insert(*x)).collect()
}

/// `computeImporters`: module → the (deduplicated) modules importing it,
/// self-imports skipped, in module order.
fn compute_importers(modules: &[FossilModule]) -> Vec<Vec<usize>> {
    let mut rev: Vec<Vec<usize>> = vec![Vec::new(); modules.len()];
    for (i, m) in modules.iter().enumerate() {
        for &imp in &m.imports {
            if imp == i {
                continue;
            }
            if let Some(list) = rev.get_mut(imp)
                && list.last() != Some(&i)
            {
                list.push(i);
            }
        }
    }
    rev
}

fn placed(folder: String, file: String, signal: FossilFolderSignal) -> Option<FossilPlacement> {
    Some(FossilPlacement {
        folder,
        file,
        signal,
    })
}

/// Signal 1 (`placeBarrels`): barrels anchor folders; fan-out members
/// whose importers all sit inside the barrel's reach join them.
fn place_barrels(
    modules: &[FossilModule],
    body: &[Value],
    stems: &[String],
    importers: &[Vec<usize>],
    placements: &mut [Option<FossilPlacement>],
) {
    for (b, m) in modules.iter().enumerate() {
        let fan_out: Vec<usize> = dedup_usize(&m.imports)
            .into_iter()
            .filter(|&x| x != b)
            .collect();
        if fan_out.len() < 2
            || m.declared.len() > 2
            || !init_body_is_only_init_calls(&body[m.init_index])
            || placements[b].is_some()
        {
            continue;
        }
        let folder = format!("src/{}", stems[b]);
        placements[b] = placed(
            folder.clone(),
            format!("{}.js", stems[b]),
            FossilFolderSignal::Barrel,
        );
        let mut reach: HashSet<usize> = fan_out.iter().copied().collect();
        reach.insert(b);
        for &member in &fan_out {
            if placements[member].is_some() {
                continue;
            }
            let contained = importers[member].iter().all(|imp| reach.contains(imp));
            if contained {
                placements[member] = placed(
                    folder.clone(),
                    format!("{}.js", stems[member]),
                    FossilFolderSignal::Barrel,
                );
            }
        }
    }
}

/// `dominantImporterParents`: parent[i] = the unique importer of i (else
/// none), cycles broken in module order.
fn dominant_importer_parents(modules: &[FossilModule]) -> Vec<Option<usize>> {
    let n = modules.len();
    let mut importers: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (i, m) in modules.iter().enumerate() {
        for &imp in &m.imports {
            if let Some(list) = importers.get_mut(imp) {
                list.push(i);
            }
        }
    }
    let mut parent: Vec<Option<usize>> = (0..n)
        .map(|i| {
            let imps: Vec<usize> = dedup_usize(&importers[i])
                .into_iter()
                .filter(|&x| x != i)
                .collect();
            (imps.len() == 1).then(|| imps[0])
        })
        .collect();
    for i in 0..n {
        let mut seen: HashSet<usize> = HashSet::from([i]);
        let mut p = parent[i];
        while let Some(cur) = p {
            if !seen.insert(cur) {
                parent[i] = None;
                break;
            }
            p = parent[cur];
        }
    }
    parent
}

/// Signal 2 (`placeByDominantImporter`): unique-importer nesting under
/// stem-named anchor folders.
fn place_by_dominant_importer(
    modules: &[FossilModule],
    stems: &[String],
    placements: &mut [Option<FossilPlacement>],
) {
    let parent = dominant_importer_parents(modules);
    let has_children: HashSet<usize> = parent.iter().flatten().copied().collect();
    let anchor_of = |i: usize| {
        let mut cur = i;
        while let Some(p) = parent[cur] {
            cur = p;
        }
        cur
    };
    for i in 0..modules.len() {
        if placements[i].is_some() {
            continue;
        }
        let anchor = anchor_of(i);
        if !(has_children.contains(&anchor) || anchor != i) {
            continue;
        }
        let file = format!("{}.js", stems[i]);
        placements[i] = if anchor == i {
            placed(
                format!("src/{}", stems[anchor]),
                file,
                FossilFolderSignal::Anchor,
            )
        } else {
            let p = parent[i].expect("a non-anchor has a parent");
            let sub = if p == anchor {
                String::new()
            } else {
                format!("/{}", stems[p])
            };
            placed(
                format!("src/{}{sub}", stems[anchor]),
                file,
                FossilFolderSignal::DominantImporter,
            )
        };
    }
}

/// Signal 3 (`placeCoImporterGroups`): identical importer sets group; the
/// folder is named by the first member in stem order.
fn place_co_importer_groups(
    stems: &[String],
    importers: &[Vec<usize>],
    placements: &mut [Option<FossilPlacement>],
) {
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut at: HashMap<Vec<usize>, usize> = HashMap::new();
    for i in 0..stems.len() {
        if placements[i].is_some() {
            continue;
        }
        let mut set = importers[i].clone();
        set.sort_unstable();
        if set.len() < 2 {
            continue;
        }
        match at.get(&set) {
            Some(&slot) => groups[slot].push(i),
            None => {
                at.insert(set, groups.len());
                groups.push(vec![i]);
            }
        }
    }
    for members in groups.iter().filter(|m| m.len() >= 2) {
        let lead = *members
            .iter()
            .min_by(|&&a, &&b| cmp_utf16(&stems[a], &stems[b]).then(a.cmp(&b)))
            .expect("a group has members");
        let folder = format!("src/{}-shared", stems[lead]);
        for &i in members {
            placements[i] = placed(
                folder.clone(),
                format!("{}.js", stems[i]),
                FossilFolderSignal::CoImporter,
            );
        }
    }
}

/// `consensusFolder`: the folder a flat file's importers agree on — ties
/// break by folder name so the outcome never depends on walk order.
fn consensus_folder(
    i: usize,
    placements: &[FossilPlacement],
    importers: &[Vec<usize>],
) -> Option<String> {
    if placements[i].folder != "src" {
        return None;
    }
    let ups = &importers[i];
    if ups.is_empty() {
        return None;
    }
    let mut votes: Vec<(&str, usize)> = Vec::new();
    for &u in ups {
        let f = placements[u].folder.as_str();
        if f == "src" {
            continue;
        }
        match votes.iter_mut().find(|(k, _)| *k == f) {
            Some((_, n)) => *n += 1,
            None => votes.push((f, 1)),
        }
    }
    // `b[1] - a[1] || (a[0] < b[0] ? -1 : 1)`: the keys are distinct, so
    // the name order is total.
    votes.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| cmp_utf16(a.0, b.0)));
    let (folder, count) = votes.first()?;
    (*count as f64 / ups.len() as f64 >= CONSENSUS).then(|| folder.to_string())
}

/// Signal 4 (`placeByImporterConsensus`): a flat-root file whose importers
/// mostly live in one folder moves in with them.
fn place_by_importer_consensus(placements: &mut [FossilPlacement], importers: &[Vec<usize>]) {
    for _ in 0..CONSENSUS_PASSES {
        let mut moved = 0;
        for i in 0..placements.len() {
            if let Some(folder) = consensus_folder(i, placements, importers) {
                placements[i].folder = folder;
                placements[i].signal = FossilFolderSignal::CoImporter;
                moved += 1;
            }
        }
        if moved == 0 {
            break;
        }
    }
}

/// `folder.slice(0, folder.lastIndexOf("/"))` — with JS's `-1` meaning
/// "drop the last UTF-16 unit" when there is no slash.
fn js_parent_slice(folder: &str) -> String {
    match folder.rfind('/') {
        Some(cut) => folder[..cut].to_string(),
        None => {
            let mut s = folder.to_string();
            s.pop();
            s
        }
    }
}

/// `collapseSmallFolders`: dissolve folders under the floor into their
/// parent, twice, so single-child chains unwind.
fn collapse_small_folders(placements: Vec<FossilPlacement>, min: usize) -> Vec<FossilPlacement> {
    let mut current = placements;
    for _ in 0..2 {
        let mut counts: HashMap<String, usize> = HashMap::new();
        for p in &current {
            *counts.entry(p.folder.clone()).or_default() += 1;
        }
        for p in &mut current {
            if counts.get(&p.folder).copied().unwrap_or(0) >= min {
                continue;
            }
            let parent = js_parent_slice(&p.folder);
            p.folder = if utf16_len(&parent) >= 3 {
                parent
            } else {
                "src".to_string()
            };
        }
    }
    current
}

/// A module's file stem: its module name read back from its wrapper
/// (`initColorUtils` → `color-utils`, [`module_stem_of_wrapper`]) — the
/// flag says so — else the mechanical [`module_stem`].
pub fn named_module_stem(module: &FossilModule, body: &[Value]) -> (String, bool) {
    match module_stem_of_wrapper(&module.init_name) {
        Some(stem) => (stem, true),
        None => (module_stem(module, body), false),
    }
}

/// `inferFossilPlacements`: the folder hierarchy inferred from the import
/// DAG, one placement per module, over each module's file stem
/// ([`named_module_stem`]).
fn infer_fossil_placements(
    modules: &[FossilModule],
    body: &[Value],
    stems: &[String],
    min_folder_files: usize,
) -> Vec<FossilPlacement> {
    let importers = compute_importers(modules);
    let mut placements: Vec<Option<FossilPlacement>> = vec![None; modules.len()];
    place_barrels(modules, body, stems, &importers, &mut placements);
    place_by_dominant_importer(modules, stems, &mut placements);
    place_co_importer_groups(stems, &importers, &mut placements);
    let mut settled: Vec<FossilPlacement> = placements
        .into_iter()
        .zip(stems)
        .map(|(p, stem)| {
            p.unwrap_or_else(|| FossilPlacement {
                folder: "src".to_string(),
                file: format!("{stem}.js"),
                signal: FossilFolderSignal::Flat,
            })
        })
        .collect();
    place_by_importer_consensus(&mut settled, &importers);
    collapse_small_folders(settled, min_folder_files)
}

/// `folderOfPath`: `src/a/b/foo.js` → `src/a/b`; the root for a bare name.
fn folder_of_path(file: &str) -> String {
    match file.rfind('/') {
        Some(cut) if cut > 0 => file[..cut].to_string(),
        _ => "src".to_string(),
    }
}

/// `hoistSingletonFolders` (exp076): a fresh file alone in its folder —
/// counting the FINAL tree, inherited paths included — moves up a level,
/// unless the hoist would collide. Returns the number hoisted.
fn hoist_singleton_folders(
    file_of_module: &[Option<String>],
    placements: &mut [FossilPlacement],
    used: &OrderedSet,
) -> usize {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for (i, file) in file_of_module.iter().enumerate() {
        let folder = match file {
            Some(f) => folder_of_path(f),
            None => placements[i].folder.clone(),
        };
        *counts.entry(folder).or_default() += 1;
    }
    let mut hoisted = 0;
    for (i, file) in file_of_module.iter().enumerate() {
        if file.is_some() {
            continue;
        }
        let folder = placements[i].folder.clone();
        if folder == "src" || counts.get(&folder).copied() != Some(1) {
            continue;
        }
        let parent = folder_of_path(&folder);
        let target = format!("{parent}/{}", placements[i].file);
        if used.has(&target) {
            continue;
        }
        placements[i].folder = parent.clone();
        counts.insert(folder, 0);
        *counts.entry(parent).or_default() += 1;
        hoisted += 1;
    }
    hoisted
}

/// `claimPath`: the first free variant of a path — base, then -2, -3…
fn claim_path(base: &str, used: &mut OrderedSet) -> String {
    if !used.has(base) {
        used.add(base);
        return base.to_string();
    }
    // `base.lastIndexOf(".js")` — every base here ends in `.js`.
    let stem = &base[..base.rfind(".js").unwrap_or(base.len())];
    let mut n = 2;
    loop {
        let candidate = format!("{stem}-{n}.js");
        if !used.has(&candidate) {
            used.add(&candidate);
            return candidate;
        }
        n += 1;
    }
}

/// The per-module inputs the matcher reads, fresh side.
fn fresh_signatures(
    extract: &FossilExtract,
    stems: &[String],
    tokens: &[Vec<String>],
) -> Vec<FossilSignature> {
    extract
        .modules
        .iter()
        .enumerate()
        .map(|(i, m)| FossilSignature {
            hashes: m.hashes.clone(),
            imports: m.imports.clone(),
            stem: Some(stems[i].clone()),
            tokens: Some(tokens[i].clone()),
            declared: Some(m.declared.clone()),
        })
        .collect()
}

/// A prior ledger path's file stem — `src/a/b/foo.js` → `foo`.
fn prior_file_stem(file: &str) -> String {
    let base = &file[file.rfind('/').map_or(0, |c| c + 1)..];
    base.strip_suffix(".js").unwrap_or(base).to_string()
}

/// The prior side's signatures. The stem the same-file-name tier compares
/// is the prior module's RECORDED mechanical stem — its file carries the
/// module's name, which a fresh mechanical stem never equals — else (a
/// ledger written before the field) its file stem.
fn prior_signatures(prior_modules: &[FossilLedgerModule]) -> Vec<FossilSignature> {
    prior_modules
        .iter()
        .map(|m| FossilSignature {
            hashes: m.hashes.clone(),
            imports: m.imports.clone(),
            stem: Some(
                m.mechanical_stem
                    .clone()
                    .unwrap_or_else(|| prior_file_stem(&m.file)),
            ),
            tokens: m.tokens.clone(),
            declared: m.declared.clone(),
        })
        .collect()
}

/// What [`assign_fossil`] needs beside the body.
pub struct FossilOptions<'t> {
    pub min_folder_files: usize,
    /// The placement trail to record into (`--diagnostics`), if armed.
    pub trail: Option<&'t mut PlacementTrail>,
}

impl Default for FossilOptions<'_> {
    fn default() -> Self {
        FossilOptions {
            min_folder_files: MIN_FOLDER_FILES,
            trail: None,
        }
    }
}

/// `assignFossil`.
pub fn assign_fossil(
    body: &[Value],
    spans: &[(u32, u32)],
    hashes: &[String],
    prior: Option<&StableSplitLedger>,
    options: FossilOptions,
) -> Result<FossilAssignment, String> {
    let extract = extract_fossil_modules(body, hashes)?;
    if extract.modules.is_empty() {
        return Err(
            "fossil split: the bundle records no module fossils (no __esm init \
definitions) — fix detection or run with --disable fossil-split"
                .into(),
        );
    }
    // A module's identity is its hash multiset: a ledger hashed by another
    // function (the TS era) has no modules this run can match.
    let prior_modules: &[FossilLedgerModule] = prior
        .filter(|p| p.hashes_current())
        .and_then(|p| p.fossil_modules.as_deref())
        .unwrap_or(&[]);
    // The matcher's stems are MECHANICAL on both sides (the prior's are
    // recorded in its ledger); the files and folders take module names.
    let mechanical_stems: Vec<String> = extract
        .modules
        .iter()
        .map(|m| module_stem(m, body))
        .collect();
    let (file_stems, module_named): (Vec<String>, Vec<bool>) = extract
        .modules
        .iter()
        .map(|m| named_module_stem(m, body))
        .unzip();
    let fresh_tokens: Vec<Vec<String>> = extract
        .modules
        .iter()
        .map(|m| super::tokens::module_tokens(m, body))
        .collect();
    let matched = match_fossil_modules(
        &prior_signatures(prior_modules),
        &fresh_signatures(&extract, &mechanical_stems, &fresh_tokens),
    );

    let mut used = OrderedSet::default();
    used.add(FOSSIL_BOOTSTRAP_FILE);
    let mut file_of_module: Vec<Option<String>> = vec![None; extract.modules.len()];
    for &(fresh_idx, prior_idx) in &matched.matches {
        let file = prior_modules[prior_idx].file.clone();
        used.add(&file);
        file_of_module[fresh_idx] = Some(file);
    }
    let mut placements = infer_fossil_placements(
        &extract.modules,
        body,
        &file_stems,
        options.min_folder_files,
    );
    let hoisted = hoist_singleton_folders(&file_of_module, &mut placements, &used);
    let mut fresh_named = 0;
    let mut module_named_files = 0;
    for i in 0..extract.modules.len() {
        if file_of_module[i].is_some() {
            continue;
        }
        let file = claim_path(
            &format!("{}/{}", placements[i].folder, placements[i].file),
            &mut used,
        );
        file_of_module[i] = Some(file);
        fresh_named += 1;
        module_named_files += usize::from(module_named[i]);
    }
    let final_file: Vec<String> = file_of_module
        .into_iter()
        .enumerate()
        .map(|(i, f)| {
            f.ok_or(format!(
                "fossil split: module {i} was never assigned a file"
            ))
        })
        .collect::<Result<_, _>>()?;

    let mut assignment = vec![String::new(); body.len()];
    for (i, module) in extract.modules.iter().enumerate() {
        for &s in &module.statements {
            assignment[s] = final_file[i].clone();
        }
    }
    for &s in &extract.eager_zone {
        assignment[s] = FOSSIL_BOOTSTRAP_FILE.to_string();
    }
    if let Some(trail) = options.trail {
        record_fossil_trail(
            trail,
            &FossilTrailInput {
                extract: &extract,
                body,
                spans,
                hashes,
                prior_modules,
                matched: &matched.pair_tiers,
                final_file: &final_file,
            },
        );
    }
    Ok(FossilAssignment {
        assignment,
        fossil_modules: extract
            .modules
            .iter()
            .zip(fresh_tokens)
            .enumerate()
            .map(|(i, (m, tokens))| FossilLedgerModule {
                file: final_file[i].clone(),
                hashes: m.hashes.clone(),
                imports: m.imports.clone(),
                declared: Some(m.declared.clone()),
                tokens: Some(tokens),
                source_path: m.source_path.clone(),
                mechanical_stem: Some(mechanical_stems[i].clone()),
            })
            .collect(),
        stats: FossilStats {
            modules: extract.modules.len(),
            inherited_files: matched.matches.len(),
            fresh_named_files: fresh_named,
            module_named_files,
            eager_statements: extract.eager_zone.len(),
            match_tiers: matched.tiers,
            hoisted_singletons: hoisted,
        },
    })
}

struct FossilTrailInput<'a> {
    extract: &'a FossilExtract,
    body: &'a [Value],
    spans: &'a [(u32, u32)],
    hashes: &'a [String],
    prior_modules: &'a [FossilLedgerModule],
    matched: &'a HashMap<usize, &'static str>,
    final_file: &'a [String],
}

/// `recordFossilTrail`: every wrapper statement into the placement trail.
/// `priorFile` is hash-keyed: the ONE prior module file holding this
/// statement's hash (ambiguous = no evidence).
fn record_fossil_trail(trail: &mut PlacementTrail, input: &FossilTrailInput) {
    // hash → Some(the one prior file) | None (ambiguous).
    let mut prior_hash_file: HashMap<&str, Option<&str>> = HashMap::new();
    for m in input.prior_modules {
        for h in &m.hashes {
            match prior_hash_file.get(h.as_str()) {
                None => {
                    prior_hash_file.insert(h, Some(&m.file));
                }
                Some(Some(cur)) if *cur != m.file => {
                    prior_hash_file.insert(h, None);
                }
                Some(_) => {}
            }
        }
    }
    for (i, module) in input.extract.modules.iter().enumerate() {
        let placed_by = match input.matched.get(&i) {
            Some(tier) => format!("fossil:{tier}"),
            None => "fossil-fresh".to_string(),
        };
        for &s in &module.statements {
            let prior_file = prior_hash_file
                .get(input.hashes[s].as_str())
                .copied()
                .flatten()
                .map(str::to_string);
            let from = prior_file.as_ref().map(|_| "hash");
            trail.record(TrailEntry {
                index: s,
                span: Some(input.spans[s]),
                names: declared_names(&input.body[s]),
                placed_by: placed_by.clone(),
                file: input.final_file[i].clone(),
                prior_file,
                prior_file_from: from,
                ..TrailEntry::default()
            });
        }
    }
    for &s in &input.extract.eager_zone {
        trail.record(TrailEntry {
            index: s,
            span: Some(input.spans[s]),
            names: declared_names(&input.body[s]),
            placed_by: "fossil-eager".to_string(),
            file: FOSSIL_BOOTSTRAP_FILE.to_string(),
            ..TrailEntry::default()
        });
    }
}

#[cfg(test)]
mod fossil_test;
