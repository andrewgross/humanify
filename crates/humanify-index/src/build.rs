//! The build: plan (as-of selection + which shards exist), then install,
//! bundle and fingerprint what is missing, then merge
//! (docs/design/library-index.md §2).
//!
//! Every fingerprint comes from humanify-core: the module key from
//! `modules::vendor_content::bundle_content_keys` with the pieces
//! `toolchain::resolve_toolchain` chooses for the slice's bundler, the
//! literal sets from `libindex::literals`. This file only moves files
//! around, runs npm and the bundlers, and reads sourcemaps.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

use humanify_core::libindex::format::{
    PackageRecord, RefusedRecord, RegistrySnapshot, SliceRecord, fingerprint_versions,
};
use humanify_core::libindex::literals::{LiteralMode, code_literals, json_literals};
use humanify_core::libindex::{IndexSlice, format::compare_versions};

use crate::asof::{normalize_time, select_versions};
use crate::merge::{MergeInput, merge};
use crate::recipe::{
    MAX_SOURCE_BYTES, RECIPE_VERSION, Recipe, bundler_package, is_source_file, is_sponge_file,
    unresolved_specifiers,
};
use crate::registry;
use crate::shard::{
    Shard, ShardInputs, ShardKind, ShardLiteral, ShardModule, read_current, write_shard,
};
use crate::sourcemap::{SourceMap, owner_cached};

/// A module span must take at least this share of its mapping segments
/// from one source file to be attributed (the lab's "pure" spans).
const PURITY: f64 = 0.9;

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
                let newest = select_versions(p, &cfg.as_of, &[])
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
    let mut out = vec![ShardInputs {
        kind: ShardKind::Literals,
        package: name.to_string(),
        version: version.to_string(),
        bundler: None,
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
            fingerprint: f.module.clone(),
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
    for (name, pins) in group_specs(&cfg.packages) {
        let (packument, sha) = registry::load(&cfg.registry, &name, cfg.offline)?;
        snapshot.packuments.insert(name.clone(), sha);
        note_fetch(&mut snapshot, packument.fetched_at.as_deref());
        let chosen = select_versions(&packument, &cfg.as_of, &pins);
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

/// The two app entries: a `require()` and an `import()` of the package.
fn write_entries(dir: &Path, name: &str) -> Result<(), String> {
    let spec = serde_json::to_string(name).map_err(|e| e.to_string())?;
    let app = |tag: &str| {
        format!(
            "function labAppMain_{tag}(x){{ if (!x) throw new Error(\"lab-app {tag}: no input given\"); return Object.keys(x).length + \":{tag}\"; }}\n"
        )
    };
    let entries = dir.join("entries");
    std::fs::create_dir_all(&entries).map_err(|e| e.to_string())?;
    std::fs::write(
        entries.join("req.cjs"),
        format!(
            "{}const m0 = require({spec});\nmodule.exports = {{ run: labAppMain_req, m0 }};\n",
            app("req")
        ),
    )
    .map_err(|e| e.to_string())?;
    std::fs::write(
        entries.join("dyn.mjs"),
        format!(
            "{}export const loaders = [() => import({spec})];\nexport {{ labAppMain_dyn }};\n",
            app("dyn")
        ),
    )
    .map_err(|e| e.to_string())
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
    write_entries(&dir, &task.name)?;
    std::fs::write(dir.join(".installed"), "").map_err(|e| e.to_string())?;
    Ok(dir)
}

/// The literal shard: every source file of every installed package,
/// owned by its nearest package.json.
fn literal_shard(dir: &Path, inputs: &ShardInputs) -> Shard {
    let mut shard = Shard::new(inputs);
    let mut files = Vec::new();
    walk_files(&dir.join("node_modules"), &mut files);
    let mut owners: HashMap<PathBuf, Option<(String, String)>> = HashMap::new();
    let out_dir = dir.join("out");
    for f in files {
        let Ok(rel) = f.strip_prefix(dir) else {
            continue;
        };
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
fn fingerprint_bundle(slice: IndexSlice, out: &Path, shard: &mut Shard) -> Result<(), String> {
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
    let mut owners = HashMap::new();
    for (m, src) in modules.iter().zip(sources) {
        let Some(src) = src else {
            shard.notes.mixed_spans += 1;
            continue;
        };
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
    Ok(())
}

/// The module shard of one slice: both entries built and fingerprinted.
fn module_shard(dir: &Path, recipe: &Recipe, inputs: &ShardInputs) -> Shard {
    let mut shard = Shard::new(inputs);
    for (tag, entry) in [("req", "entries/req.cjs"), ("dyn", "entries/dyn.mjs")] {
        let out = dir.join("build").join(recipe.slice.name()).join(tag);
        let result = build_entry(recipe, dir, entry, &out)
            .and_then(|()| fingerprint_bundle(recipe.slice, &out, &mut shard));
        let status = match result {
            Ok(()) => "ok".to_string(),
            Err(e) => e,
        };
        shard.notes.builds.insert(tag.to_string(), status);
    }
    shard
}

/// Build every missing shard of `task`.
fn execute(cfg: &BuildConfig, task: &Task) -> Result<(), String> {
    let dir = install(&cfg.work, task)?;
    for inputs in &task.missing {
        let shard = match inputs.kind {
            ShardKind::Literals => literal_shard(&dir, inputs),
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
