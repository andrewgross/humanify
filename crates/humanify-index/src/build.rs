//! The build: plan (as-of selection + which shards exist), then install,
//! bundle and fingerprint what is missing, then merge
//! (docs/design/library-index.md §2).
//!
//! Every fingerprint comes from humanify-core: the module key from
//! `modules::vendor_content::bundle_content_keys` with the pieces
//! `toolchain::resolve_toolchain` chooses for the slice's bundler, the
//! literal sets from `libindex::literals`. This file only moves files
//! around, runs npm and the bundlers, and reads sourcemaps.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

use humanify_core::libindex::format::{
    PackageRecord, RefusedRecord, RegistrySnapshot, SliceRecord, fingerprint_versions,
};
use humanify_core::libindex::literals::{LiteralMode, code_literals, json_literals};
use humanify_core::libindex::{IndexSlice, format::compare_versions};

use crate::asof::{VersionPolicy, normalize_time, select_versions};
use crate::merge::{MergeInput, merge};
use crate::recipe::{
    MAX_SOURCE_BYTES, RECIPE_VERSION, Recipe, bundler_package, entry_specifiers, is_source_file,
    is_sponge_file, unresolved_specifiers,
};
use crate::registry;
use crate::shard::{
    Shard, ShardInputs, ShardKind, ShardLiteral, ShardModule, ShardNotes, ShardUnit, read_current,
    write_shard,
};
use crate::sourcemap::{SourceMap, lexical, owner_cached};

/// The two app entries every slice builds: (tag, entry file).
const ENTRIES: [(&str, &str); 2] = [("req", "entries/req.cjs"), ("dyn", "entries/dyn.mjs")];

/// A module span must take at least this share of its mapping segments
/// from one source file to be attributed (the lab's "pure" spans).
const PURITY: f64 = 0.9;

/// Units smaller than this are not indexed (the lab's unit votes counted
/// units of >= 20 nodes; the lookup's own floor is higher).
pub const UNIT_ROW_MIN_NODES: u32 = 20;

pub struct BuildConfig {
    /// `name` or `name@version` (a pin), one per entry.
    pub packages: Vec<String>,
    /// Normalised (`asof::parse_as_of`).
    pub as_of: String,
    pub registry: PathBuf,
    pub store: PathBuf,
    pub work: PathBuf,
    pub out: Option<PathBuf>,
    pub slices: Vec<Recipe>,
    pub jobs: usize,
    pub offline: bool,
    /// Which versions of each package to index.
    pub policy: VersionPolicy,
    /// `name@version` specs indexed at exactly these versions (no policy
    /// picks).
    pub pinned: Vec<String>,
}

/// One `package@version` to index.
#[derive(Clone, Debug)]
pub struct Task {
    pub name: String,
    pub version: String,
    pub role: String,
    /// Its publish time: npm resolves its dependencies as of THIS time.
    pub published: String,
    pub missing: Vec<ShardInputs>,
    pub reused: Vec<ShardInputs>,
}

#[derive(Debug)]
pub struct Plan {
    pub as_of: String,
    pub tasks: Vec<Task>,
    pub refused: Vec<RefusedRecord>,
    pub registry: RegistrySnapshot,
    pub slices: Vec<SliceRecord>,
}

/// `name` / `name@version` (a scope's leading `@` is not a separator).
pub fn parse_package_spec(spec: &str) -> (String, Option<String>) {
    match spec.rfind('@') {
        Some(at) if at > 0 => (spec[..at].to_string(), Some(spec[at + 1..].to_string())),
        _ => (spec.to_string(), None),
    }
}

/// Specs grouped by package, first-seen order, pins kept.
fn group_specs(specs: &[String]) -> Vec<(String, Vec<String>)> {
    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    for spec in specs.iter().map(|s| s.trim()).filter(|s| !s.is_empty()) {
        let (name, pin) = parse_package_spec(spec);
        let slot = match out.iter().position(|(n, _)| *n == name) {
            Some(i) => i,
            None => {
                out.push((name, Vec::new()));
                out.len() - 1
            }
        };
        out[slot].1.extend(pin);
    }
    out
}

fn note_fetch(snapshot: &mut RegistrySnapshot, fetched: Option<&str>) {
    let Some(t) = fetched.and_then(normalize_time) else {
        return;
    };
    if snapshot.earliest_fetch.as_ref().is_none_or(|e| t < *e) {
        snapshot.earliest_fetch = Some(t.clone());
    }
    if snapshot.latest_fetch.as_ref().is_none_or(|l| t > *l) {
        snapshot.latest_fetch = Some(t);
    }
}

/// The slice records: each bundler's version, its publish time, the
/// newest version at the as-of date, and whether the one used is newer.
fn slice_records(
    cfg: &BuildConfig,
    snapshot: &mut RegistrySnapshot,
) -> Result<Vec<SliceRecord>, String> {
    let mut out = Vec::new();
    for recipe in &cfg.slices {
        let version = recipe.bundler_version()?;
        let package = bundler_package(recipe.slice);
        let dated = registry::load(&cfg.registry, package, cfg.offline).ok();
        let (published, newest) = match &dated {
            Some((p, sha)) => {
                snapshot.packuments.insert(package.to_string(), sha.clone());
                note_fetch(snapshot, p.fetched_at.as_deref());
                let newest = select_versions(p, &cfg.as_of, &[], VersionPolicy::AsOf)
                    .selected
                    .first()
                    .map(|s| s.version.clone());
                (p.time.get(&version).and_then(|t| normalize_time(t)), newest)
            }
            None => (None, None),
        };
        out.push(SliceRecord {
            slice: recipe.slice.name().to_string(),
            bundler: package.to_string(),
            bundler_after_as_of: published.as_ref().is_some_and(|t| *t > cfg.as_of),
            version,
            published,
            newest_at_as_of: newest,
        });
    }
    Ok(out)
}

/// The shard inputs of one `package@version`: literals, then each slice.
fn shard_inputs(name: &str, version: &str, slices: &[SliceRecord]) -> Vec<ShardInputs> {
    let f = fingerprint_versions();
    // The literal rows are the files these builds contain, so the
    // bundlers that decide them are part of the shard's key.
    let builds: Vec<String> = slices
        .iter()
        .map(|s| format!("{}@{}", s.bundler, s.version))
        .collect();
    let mut out = vec![ShardInputs {
        kind: ShardKind::Literals,
        package: name.to_string(),
        version: version.to_string(),
        bundler: (!builds.is_empty()).then(|| builds.join("+")),
        recipe: RECIPE_VERSION.to_string(),
        fingerprint: f.literals.clone(),
    }];
    for s in slices {
        let Some(slice) = IndexSlice::from_name(&s.slice) else {
            continue;
        };
        out.push(ShardInputs {
            kind: ShardKind::Modules(slice),
            package: name.to_string(),
            version: version.to_string(),
            bundler: Some(format!("{}@{}", s.bundler, s.version)),
            recipe: RECIPE_VERSION.to_string(),
            fingerprint: format!("{}+{}", f.module, f.units),
        });
    }
    out
}

/// What an as-of build would index and which shards it would reuse.
pub fn plan(cfg: &BuildConfig) -> Result<Plan, String> {
    let mut snapshot = RegistrySnapshot::default();
    let slices = slice_records(cfg, &mut snapshot)?;
    let mut tasks = Vec::new();
    let mut refused = Vec::new();
    // `--pinned` specs join a listed package's pins, or are indexed at
    // their pins alone
    let mut groups: Vec<(String, Vec<String>, VersionPolicy)> = group_specs(&cfg.packages)
        .into_iter()
        .map(|(name, pins)| (name, pins, cfg.policy))
        .collect();
    for (name, pins) in group_specs(&cfg.pinned) {
        match groups.iter_mut().find(|g| g.0 == name) {
            Some(g) => g.1.extend(pins),
            None => groups.push((name, pins, VersionPolicy::PinsOnly)),
        }
    }
    for (name, pins, policy) in groups {
        let (packument, sha) = registry::load(&cfg.registry, &name, cfg.offline)?;
        snapshot.packuments.insert(name.clone(), sha);
        note_fetch(&mut snapshot, packument.fetched_at.as_deref());
        let chosen = select_versions(&packument, &cfg.as_of, &pins, policy);
        refused.extend(chosen.refused.into_iter().map(|r| RefusedRecord {
            name: name.clone(),
            version: r.version,
            reason: r.reason,
        }));
        for s in chosen.selected {
            let (mut missing, mut reused) = (Vec::new(), Vec::new());
            for inputs in shard_inputs(&name, &s.version, &slices) {
                match read_current(&cfg.store, &inputs) {
                    Some(_) => reused.push(inputs),
                    None => missing.push(inputs),
                }
            }
            tasks.push(Task {
                name: name.clone(),
                version: s.version,
                role: s.role,
                published: s.published,
                missing,
                reused,
            });
        }
    }
    Ok(Plan {
        as_of: cfg.as_of.clone(),
        tasks,
        refused,
        registry: snapshot,
        slices,
    })
}

/// The plan, as `--dry-run` prints it.
pub fn describe(plan: &Plan) -> String {
    let mut out = format!("as-of {}\n", plan.as_of);
    for s in &plan.slices {
        out.push_str(&format!(
            "slice {}: {}@{} published {} — newest at as-of: {}{}\n",
            s.slice,
            s.bundler,
            s.version,
            s.published.as_deref().unwrap_or("unknown"),
            s.newest_at_as_of.as_deref().unwrap_or("none"),
            if s.bundler_after_as_of {
                "  [bundlerAfterAsOf: the bundler is newer than the as-of date]"
            } else {
                ""
            },
        ));
    }
    for t in &plan.tasks {
        out.push_str(&format!(
            "index {}@{} ({}, published {})\n",
            t.name, t.version, t.role, t.published
        ));
        for i in &t.reused {
            out.push_str(&format!("  reuse shard {}\n", i.label()));
        }
        for i in &t.missing {
            out.push_str(&format!("  build shard {}\n", i.label()));
        }
        if !t.missing.is_empty() {
            out.push_str(&format!(
                "  would run: npm install {}@{} --ignore-scripts --before={}\n",
                t.name, t.version, t.published
            ));
        }
    }
    for r in &plan.refused {
        out.push_str(&format!("refused {}@{}: {}\n", r.name, r.version, r.reason));
    }
    out
}

fn run_cmd(cmd: &mut Command) -> Result<(), String> {
    let out = cmd.output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Every file under `dir` (sorted, recursive), skipping `.bin`/`.cache`.
fn walk_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).collect();
    entries.sort();
    for p in entries {
        let name = p
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if p.is_dir() {
            if name != ".bin" && name != ".cache" {
                walk_files(&p, out);
            }
        } else {
            out.push(p);
        }
    }
}

/// Input sourcemap comments are removed so a bundler does not chain them.
fn strip_source_map_comments(node_modules: &Path) {
    let mut files = Vec::new();
    walk_files(node_modules, &mut files);
    for f in files {
        let n = f.to_string_lossy();
        if !(n.ends_with(".js") || n.ends_with(".mjs") || n.ends_with(".cjs")) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&f) else {
            continue;
        };
        if !text.contains("sourceMappingURL=") {
            continue;
        }
        let kept: Vec<&str> = text
            .split('\n')
            .filter(|l| {
                !(l.starts_with("//# sourceMappingURL=") || l.starts_with("//@ sourceMappingURL="))
            })
            .collect();
        let _ = std::fs::write(&f, kept.join("\n"));
    }
}

/// The package's files (relative to its root, sorted; its own nested
/// `node_modules` excluded) — what a wildcard export can name.
fn package_files(root: &Path) -> Vec<String> {
    let mut files = Vec::new();
    walk_files(root, &mut files);
    files
        .iter()
        .filter_map(|f| f.strip_prefix(root).ok())
        .map(|r| r.to_string_lossy().replace('\\', "/"))
        .filter(|r| !r.split('/').any(|c| c == "node_modules"))
        .collect()
}

/// Write `text` to `path` as a NEW file (unlinked first: a work directory
/// may be a hardlinked copy of another).
fn write_fresh(path: &Path, text: &str) -> Result<(), String> {
    let _ = std::fs::remove_file(path);
    std::fs::write(path, text).map_err(|e| e.to_string())
}

/// The two app entries: a `require()` and an `import()` of the package
/// and of every subpath its `exports` map gives a Node app
/// ([`entry_specifiers`]).
fn write_entries(dir: &Path, name: &str) -> Result<(), String> {
    let root = dir.join("node_modules").join(name);
    let manifest: serde_json::Value = std::fs::read_to_string(root.join("package.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    let specs = entry_specifiers(name, &manifest, &package_files(&root));
    let quoted: Vec<String> = specs
        .iter()
        .map(|s| serde_json::to_string(s).map_err(|e| e.to_string()))
        .collect::<Result<_, _>>()?;
    let app = |tag: &str| {
        format!(
            "function labAppMain_{tag}(x){{ if (!x) throw new Error(\"lab-app {tag}: no input given\"); return Object.keys(x).length + \":{tag}\"; }}\n"
        )
    };
    let entries = dir.join("entries");
    std::fs::create_dir_all(&entries).map_err(|e| e.to_string())?;
    let requires: String = quoted
        .iter()
        .enumerate()
        .map(|(i, q)| format!("const m{i} = require({q});\n"))
        .collect();
    let names: Vec<String> = (0..quoted.len()).map(|i| format!("m{i}")).collect();
    write_fresh(
        &entries.join("req.cjs"),
        &format!(
            "{}{requires}module.exports = {{ run: labAppMain_req, {} }};\n",
            app("req"),
            names.join(", ")
        ),
    )?;
    let loaders: Vec<String> = quoted
        .iter()
        .map(|q| format!("() => import({q})"))
        .collect();
    write_fresh(
        &entries.join("dyn.mjs"),
        &format!(
            "{}export const loaders = [{}];\nexport {{ labAppMain_dyn }};\n",
            app("dyn"),
            loaders.join(", ")
        ),
    )
}

/// Install `task` (once) in its work directory: dependencies as of its
/// own publish time (`--before`), no install scripts.
fn install(work: &Path, task: &Task) -> Result<PathBuf, String> {
    let dir = work.join(format!("{}@{}", task.name.replace('/', "__"), task.version));
    if dir.join(".installed").exists() {
        return Ok(dir);
    }
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let manifest = serde_json::json!({"private": true, "name": "ix-app", "dependencies": {&task.name: &task.version}});
    std::fs::write(dir.join("package.json"), format!("{manifest}\n")).map_err(|e| e.to_string())?;
    let before = format!("--before={}", task.published);
    let base = [
        "install",
        "--ignore-scripts",
        "--no-audit",
        "--no-fund",
        "--loglevel=error",
    ];
    let first = run_cmd(
        Command::new("npm")
            .current_dir(&dir)
            .args(base)
            .arg(&before),
    );
    if let Err(e) = first {
        run_cmd(
            Command::new("npm")
                .current_dir(&dir)
                .args(base)
                .arg(&before)
                .arg("--legacy-peer-deps"),
        )
        .map_err(|e2| format!("npm install {}@{}: {e} / {e2}", task.name, task.version))?;
    }
    strip_source_map_comments(&dir.join("node_modules"));
    std::fs::write(dir.join(".installed"), "").map_err(|e| e.to_string())?;
    Ok(dir)
}

/// `path` with symlinks resolved (a bundler writes map sources relative to
/// the REAL directory it built in), lexically when it cannot be resolved.
fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| lexical(path))
}

/// The files `slices`' builds of this install contain: every source a
/// build's sourcemap maps code to (lexical paths under `dir`).
fn bundled_files(dir: &Path, slices: &[IndexSlice]) -> BTreeSet<PathBuf> {
    let mut out = BTreeSet::new();
    for slice in slices {
        for (tag, _) in ENTRIES {
            let build = dir.join("build").join(slice.name()).join(tag);
            let Ok(text) = std::fs::read_to_string(build.join("out.js.map")) else {
                continue;
            };
            let Ok(map) = SourceMap::parse(&text) else {
                continue;
            };
            for s in map.mapped_sources() {
                out.insert(canonical(&build.join(&map.sources[s as usize])));
            }
        }
    }
    out
}

/// The literal shard: every source file of the installed packages that a
/// build of `slices` contains (exp095 selection lab: ~70% of a whole-tree
/// literal table is files no bundle pulls in — other entry points, CLI
/// scripts, data, shipped tests), owned by its nearest package.json. With
/// no slices there is nothing to restrict by: every file.
pub fn literal_shard(dir: &Path, inputs: &ShardInputs, slices: &[IndexSlice]) -> Shard {
    let mut shard = Shard::new(inputs);
    let mut files = Vec::new();
    walk_files(&dir.join("node_modules"), &mut files);
    let bundled = bundled_files(dir, slices);
    let mut owners: HashMap<PathBuf, Option<(String, String)>> = HashMap::new();
    let out_dir = dir.join("out");
    for f in files {
        let Ok(rel) = f.strip_prefix(dir) else {
            continue;
        };
        if !slices.is_empty() && !bundled.contains(&canonical(&f)) {
            if is_source_file(&f.to_string_lossy()) {
                shard.notes.unbundled_files += 1;
            }
            continue;
        }
        let rel = format!("../{}", rel.to_string_lossy());
        let Some(owner) = owner_cached(&out_dir, &rel, &mut owners) else {
            continue;
        };
        if !is_source_file(&owner.file) || owner.file.contains("node_modules/") {
            continue;
        }
        if is_sponge_file(&owner.file) {
            shard.notes.sponge_files += 1;
            continue;
        }
        if std::fs::metadata(&f).map(|m| m.len()).unwrap_or(u64::MAX) > MAX_SOURCE_BYTES {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&f) else {
            shard.notes.unread_files += 1;
            continue;
        };
        let lits = if owner.file.ends_with(".json") {
            json_literals(&text)
        } else {
            code_literals(&text, LiteralMode::Source)
        };
        match lits {
            Some(l) if !l.is_empty() => shard.literals.push(ShardLiteral {
                owner: owner.package,
                version: owner.version,
                file: owner.file,
                lits: l.into_iter().collect(),
            }),
            Some(_) => {}
            None => shard.notes.unread_files += 1,
        }
    }
    shard
}

/// Build one entry, retrying unresolvable specifiers as externals (up to
/// 4 rounds) and top-level await as ESM.
fn build_entry(recipe: &Recipe, dir: &Path, entry: &str, out: &Path) -> Result<(), String> {
    let (mut externals, mut esm) = (Vec::new(), false);
    let mut last = String::new();
    for _ in 0..6 {
        match recipe.build(dir, entry, out, &externals, esm) {
            Ok(()) => return Ok(()),
            Err(e) => last = e,
        }
        let new: Vec<String> = unresolved_specifiers(&last)
            .into_iter()
            .filter(|s| !externals.contains(s))
            .collect();
        if !new.is_empty() && externals.len() < 64 {
            externals.extend(new);
        } else if last.to_lowercase().contains("top-level await") && !esm {
            esm = true;
        } else {
            break;
        }
    }
    Err(last.lines().next().unwrap_or("build failed").to_string())
}

/// One bundle's module rows: the pipeline's module walk and content key,
/// attributed through the sourcemap.
pub(crate) fn fingerprint_bundle(
    slice: IndexSlice,
    out: &Path,
    shard: &mut Shard,
) -> Result<(), String> {
    let code = std::fs::read_to_string(out.join("out.js")).map_err(|e| e.to_string())?;
    let map_text = std::fs::read_to_string(out.join("out.js.map")).map_err(|e| e.to_string())?;
    let map = SourceMap::parse(&map_text)?;
    let toolchain = humanify_core::toolchain::resolve_toolchain(
        &humanify_core::detect::detect_bundle(&code),
        Some(slice.bundler()),
        None,
    );
    let modules = humanify_core::modules::vendor_content::bundle_content_keys(
        &code,
        toolchain.module_wrappers.piece,
        toolchain.layout.piece,
    );
    let spans: Vec<(u32, u32)> = modules
        .iter()
        .map(|m| (m.body_span.start, m.body_span.end))
        .collect();
    let sources = map.attribute(&code, &spans, PURITY);
    let mut credited: BTreeMap<u32, usize> = BTreeMap::new();
    for src in sources.iter().flatten() {
        *credited.entry(*src).or_default() += 1;
    }
    let mut owners = HashMap::new();
    // A source file maps to ONE module of a bundle: a file credited to two
    // or more is wrong for at least one of them, so none is labelled.
    let mut misattributed: BTreeSet<(String, String)> = BTreeSet::new();
    for (&src, &n) in &credited {
        if n < 2 {
            continue;
        }
        shard.notes.multi_module_files += 1;
        shard.notes.multi_module_dropped += n;
        if let Some(o) = owner_cached(out, &map.sources[src as usize], &mut owners) {
            misattributed.insert((o.package, o.file));
        }
    }
    for (m, src) in modules.iter().zip(sources) {
        let Some(src) = src else {
            shard.notes.mixed_spans += 1;
            continue;
        };
        if credited[&src] >= 2 {
            continue;
        }
        let Some(owner) = owner_cached(out, &map.sources[src as usize], &mut owners) else {
            shard.notes.unowned += 1;
            continue;
        };
        let Some(key) = &m.key else {
            shard.notes.unkeyed += 1;
            continue;
        };
        shard.modules.push(ShardModule {
            owner: owner.package,
            version: owner.version,
            file: owner.file,
            key: key.clone(),
        });
    }
    let (units, notes) = unit_rows(slice, &code, &map, out);
    let (made, kept_before) = (units.len(), shard.units.len());
    shard.units.extend(
        units
            .into_iter()
            .filter(|u| !misattributed.contains(&(u.owner.clone(), u.file.clone()))),
    );
    shard.notes.multi_module_units_dropped += made - (shard.units.len() - kept_before);
    shard.notes.unit_mixed_spans += notes.mixed_spans;
    Ok(())
}

/// Unit rows of one bundle (statement-level library fingerprints): every
/// statement unit attributed through the sourcemap like a module, its
/// functions inheriting its owner; units under [`UNIT_ROW_MIN_NODES`]
/// nodes are not indexed. `module_bytes` is the bundled size of the owner
/// file in this build (the statement units it owns).
pub fn unit_rows(
    slice: IndexSlice,
    code: &str,
    map: &SourceMap,
    out: &Path,
) -> (Vec<ShardUnit>, ShardNotes) {
    let mut notes = ShardNotes::default();
    let toolchain = humanify_core::toolchain::resolve_toolchain(
        &humanify_core::detect::detect_bundle(code),
        Some(slice.bundler()),
        None,
    );
    let allocator = oxc_allocator::Allocator::default();
    let ingest = humanify_core::ingest::Ingest::parse_unambiguous(&allocator, code);
    if !ingest.errors.is_empty() {
        notes.unkeyed += 1;
        return (Vec::new(), notes);
    }
    let units = humanify_core::libindex::units::bundle_unit_prints(
        code,
        &ingest,
        toolchain.layout.piece,
        toolchain.module_wrappers.piece,
        toolchain.interop.piece,
    );
    let spans: Vec<(u32, u32)> = units
        .iter()
        .map(|(u, _)| (u.span.start, u.span.end))
        .collect();
    let mut cache = HashMap::new();
    let mut owned = Vec::new();
    for ((unit, prints), tally) in units.iter().zip(map.tally(code, &spans)) {
        match unit_owner(map, out, &tally, &mut cache) {
            UnitOwner::Owned(owner) => owned.push((owner, u64::from(unit.bytes), prints)),
            UnitOwner::Mixed => notes.mixed_spans += 1,
            UnitOwner::Unowned => notes.unowned += 1,
        }
    }
    let mut module_bytes: HashMap<(String, String), u64> = HashMap::new();
    for (o, bytes, _) in &owned {
        *module_bytes
            .entry((o.package.clone(), o.file.clone()))
            .or_default() += bytes;
    }
    let mut rows = Vec::new();
    for (o, _, prints) in &owned {
        let size = module_bytes[&(o.package.clone(), o.file.clone())];
        for p in std::iter::once(&prints.whole).chain(&prints.functions) {
            if p.nodes >= UNIT_ROW_MIN_NODES {
                rows.push(ShardUnit {
                    owner: o.package.clone(),
                    version: o.version.clone(),
                    file: o.file.clone(),
                    hash: p.hash.clone(),
                    nodes: p.nodes,
                    content: p.content,
                    module_bytes: size,
                });
            }
        }
    }
    (rows, notes)
}

enum UnitOwner {
    Owned(crate::sourcemap::Owner),
    Mixed,
    Unowned,
}

/// A statement unit's owner: the PACKAGE holding at least [`PURITY`] of
/// its segments (a unit may span several files of one package — a
/// package's own pre-bundled file mapped back to its sources), named by
/// that package's majority file. Segments outside `node_modules` count
/// against it.
fn unit_owner(
    map: &SourceMap,
    out: &Path,
    tally: &[(u32, usize)],
    cache: &mut HashMap<PathBuf, Option<(String, String)>>,
) -> UnitOwner {
    let total: usize = tally.iter().map(|t| t.1).sum();
    type Seen = (usize, usize, Option<crate::sourcemap::Owner>);
    let mut by_package: BTreeMap<String, Seen> = BTreeMap::new();
    for &(src, n) in tally {
        let Some(owner) = owner_cached(out, &map.sources[src as usize], cache) else {
            continue;
        };
        let entry = by_package
            .entry(owner.package.clone())
            .or_insert((0, 0, None));
        entry.0 += n;
        if n > entry.1 {
            entry.1 = n;
            entry.2 = Some(owner);
        }
    }
    let Some((_, (n, _, owner))) = by_package.into_iter().max_by_key(|(_, (n, _, _))| *n) else {
        return UnitOwner::Unowned;
    };
    match owner {
        Some(owner) if n as f64 >= PURITY * total as f64 => UnitOwner::Owned(owner),
        _ => UnitOwner::Mixed,
    }
}

/// The stamp a finished build leaves: the recipe and the entry it was
/// built from.
const BUILD_STAMP: &str = ".recipe";

/// Build `entry` into `out` unless a build of the same entry under the
/// current recipe is already there (the literal shard reads the same
/// builds).
fn ensure_build(recipe: &Recipe, dir: &Path, entry: &str, out: &Path) -> Result<(), String> {
    let text = std::fs::read_to_string(dir.join(entry)).map_err(|e| format!("{entry}: {e}"))?;
    let want = format!("{RECIPE_VERSION}\n{text}");
    let stamp = out.join(BUILD_STAMP);
    if std::fs::read_to_string(&stamp).is_ok_and(|s| s == want) {
        return Ok(());
    }
    build_entry(recipe, dir, entry, out)?;
    std::fs::write(&stamp, want).map_err(|e| e.to_string())
}

/// The module shard of one slice: both entries built and fingerprinted.
fn module_shard(dir: &Path, recipe: &Recipe, inputs: &ShardInputs) -> Shard {
    let mut shard = Shard::new(inputs);
    for (tag, entry) in ENTRIES {
        let out = dir.join("build").join(recipe.slice.name()).join(tag);
        let result = ensure_build(recipe, dir, entry, &out)
            .and_then(|()| fingerprint_bundle(recipe.slice, &out, &mut shard));
        let status = match result {
            Ok(()) => "ok".to_string(),
            Err(e) => e,
        };
        shard.notes.builds.insert(tag.to_string(), status);
    }
    shard
}

/// Build every missing shard of `task`: the module shards first, since the
/// literal shard reads their builds (any build not yet made is made).
fn execute(cfg: &BuildConfig, task: &Task) -> Result<(), String> {
    let dir = install(&cfg.work, task)?;
    // the entries are the recipe's: rewritten on every build
    write_entries(&dir, &task.name)?;
    let mut missing: Vec<&ShardInputs> = task.missing.iter().collect();
    missing.sort_by_key(|i| i.kind == ShardKind::Literals);
    for inputs in missing {
        if inputs.kind == ShardKind::Literals {
            for recipe in &cfg.slices {
                for (tag, entry) in ENTRIES {
                    let out = dir.join("build").join(recipe.slice.name()).join(tag);
                    // a failed build contributes no files; the module
                    // shard records the error
                    let _ = ensure_build(recipe, &dir, entry, &out);
                }
            }
        }
        let shard = match inputs.kind {
            ShardKind::Literals => {
                let slices: Vec<IndexSlice> = cfg.slices.iter().map(|r| r.slice).collect();
                literal_shard(&dir, inputs, &slices)
            }
            ShardKind::Modules(slice) => {
                let recipe = cfg
                    .slices
                    .iter()
                    .find(|r| r.slice == slice)
                    .ok_or("no recipe for slice")?;
                module_shard(&dir, recipe, inputs)
            }
        };
        write_shard(&cfg.store, &shard)?;
    }
    Ok(())
}

/// Run the missing tasks on `cfg.jobs` threads; per-task errors are
/// returned (a failed task writes no shard and is not indexed).
fn execute_all(cfg: &BuildConfig, tasks: &[Task]) -> Vec<(String, String)> {
    let todo: Vec<&Task> = tasks.iter().filter(|t| !t.missing.is_empty()).collect();
    let next = Mutex::new(0usize);
    let errors = Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..cfg.jobs.clamp(1, 4) {
            scope.spawn(|| {
                loop {
                    let i = {
                        let mut n = next.lock().expect("lock");
                        *n += 1;
                        *n - 1
                    };
                    let Some(task) = todo.get(i) else { break };
                    eprintln!("building {}@{}", task.name, task.version);
                    if let Err(e) = execute(cfg, task) {
                        let label = format!("{}@{}", task.name, task.version);
                        errors.lock().expect("lock").push((label, e));
                    }
                }
            });
        }
    });
    let mut errors = errors.into_inner().unwrap_or_default();
    errors.sort();
    errors
}

/// Plan, build what is missing, merge into `cfg.out`. Returns the index
/// id. `dry_run`: print the plan and stop (returns an empty id).
pub fn run(cfg: &BuildConfig, dry_run: bool) -> Result<String, String> {
    let mut plan = plan(cfg)?;
    print!("{}", describe(&plan));
    if dry_run {
        return Ok(String::new());
    }
    let errors = execute_all(cfg, &plan.tasks);
    for (label, e) in &errors {
        eprintln!("FAILED {label}: {e}");
    }
    let mut shards = Vec::new();
    let mut packages = Vec::new();
    for t in &plan.tasks {
        let got: Vec<Shard> = shard_inputs(&t.name, &t.version, &plan.slices)
            .iter()
            .filter_map(|i| read_current(&cfg.store, i))
            .collect();
        if got.is_empty() {
            plan.refused.push(RefusedRecord {
                name: t.name.clone(),
                version: t.version.clone(),
                reason: "build-failed".into(),
            });
            continue;
        }
        shards.extend(got);
        packages.push(PackageRecord {
            name: t.name.clone(),
            version: t.version.clone(),
            role: t.role.clone(),
            published: t.published.clone(),
        });
    }
    packages.sort_by(|a, b| {
        a.name
            .cmp(&b.name)
            .then(compare_versions(&a.version, &b.version))
    });
    let index = merge(&MergeInput {
        as_of: plan.as_of.clone(),
        registry: plan.registry.clone(),
        slices: plan.slices.clone(),
        packages,
        refused: plan.refused.clone(),
        shards,
    });
    if let Some(out) = &cfg.out {
        write_index(out, &index.files)?;
    }
    Ok(index.id)
}

fn write_index(out: &Path, files: &BTreeMap<String, Vec<u8>>) -> Result<(), String> {
    std::fs::create_dir_all(out).map_err(|e| format!("mkdir {}: {e}", out.display()))?;
    for (name, bytes) in files {
        std::fs::write(out.join(name), bytes).map_err(|e| format!("write {name}: {e}"))?;
    }
    Ok(())
}
