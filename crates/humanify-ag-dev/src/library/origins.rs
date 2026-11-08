//! The origin layer (experiments/096 `origins.py`): for each mapped byte
//! of a source-mapped bundle, the ORIGIN package when the shipping package
//! carries a recognisable copy of another package's code — a vendored
//! directory in the path, or a bundler module marker in the shipped file
//! (scorer 4); a rolldown lazy body after its region and a chunk file named
//! after a build-input package (scorer 7); the nested chunk's publisher
//! (`chunk/via:<R>`, scorer 10); a pnpm patched marker path (scorer 12).
//! Read only from the frozen map and the cached npm metadata.

use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use serde_json::Value;

use super::families::Families;
use super::pyre;

pub const SCHEMA: &str = "exp096-origins/1";
pub const VIA: &str = "chunk/via:";
const RUNTIME_KEYS: [&str; 3] = ["dependencies", "peerDependencies", "optionalDependencies"];
const BUILD_KEYS: [&str; 3] = [
    "devDependencies",
    "bundleDependencies",
    "bundledDependencies",
];

/// Which shapes the layer recognises (origins.py `RULES7/10/12`; all off =
/// scorer 4).
#[derive(Clone, Copy, Default, Debug)]
pub struct Rules {
    pub lazybody: bool,
    pub chunk: bool,
    pub via: bool,
    pub patched: bool,
}

/// One marker region: (first line, origin, marker path, kind).
pub(crate) type Region = (usize, Option<String>, Option<String>, Option<&'static str>);

/// origins.py `marker_regions`: (first line, origin) per module marker.
fn marker_regions(content: &str) -> Vec<(usize, Option<String>)> {
    let mut out = Vec::new();
    for (i, line) in content.split('\n').enumerate() {
        if line.starts_with("//#endregion") {
            out.push((i, None));
            continue;
        }
        if let Some(p) = pyre::marker_path(line).filter(|p| pyre::pathlike(p, false)) {
            out.push((i, pyre::node_modules_pkgs(p).last().map(|s| s.to_string())));
        }
    }
    out
}

/// origins.py `marker_regions7`.
pub(crate) fn marker_regions7(content: &str, lazybody: bool, patched: bool) -> Vec<Region> {
    let lines: Vec<&str> = content.split('\n').collect();
    let mut out: Vec<Region> = Vec::new();
    let mut last: Option<(Option<String>, String)> = None;
    let mut lazy = false;
    for (i, line) in lines.iter().enumerate() {
        if line.starts_with("//#endregion") {
            out.push((i, None, None, None));
            if let Some((Some(o), mp)) = last.as_ref().filter(|_| lazybody)
                && i + 1 < lines.len()
                && pyre::lazy_body(lines[i + 1])
            {
                out.push((i + 1, Some(o.clone()), Some(mp.clone()), Some("lazybody")));
                lazy = true;
            }
            last = None;
            continue;
        }
        if lazy && pyre::export_line(line) {
            out.push((i, None, None, None));
            lazy = false;
            continue;
        }
        if let Some(p) = pyre::marker_path(line).filter(|p| pyre::pathlike(p, patched)) {
            let o = pyre::node_modules_pkgs(p).last().map(|s| s.to_string());
            last = Some((o.clone(), p.to_string()));
            out.push((i, o, Some(p.to_string()), Some("marker")));
            lazy = false;
        }
    }
    out
}

/// origins.py `path_version`.
pub(crate) fn path_version(path: &str, pkg: &str) -> Option<String> {
    pyre::pnpm_pkgs(path)
        .into_iter()
        .find(|(name, _)| name.replace('+', "/") == pkg)
        .map(|(_, v)| v.to_string())
}

/// origins.py `chunk_candidates`: a built file's stem's `-`-prefixes.
fn chunk_candidates(rest: &str) -> Vec<String> {
    let base = pyre::strip_src_ext(rest.rsplit('/').next().unwrap_or(rest));
    let parts: Vec<&str> = base.split('-').collect();
    if parts[0].is_empty() {
        return Vec::new();
    }
    (1..=parts.len())
        .rev()
        .map(|k| parts[..k].join("-"))
        .collect()
}

/// (runtime dependency names, build-input names).
type Deps = (BTreeSet<String>, BTreeSet<String>);

/// An origin layer's tables: (names, kinds, runs).
pub type Runs = (Vec<String>, Vec<String>, Vec<(i64, i64, i64)>);

/// The cached npm metadata the chunk rule reads (origins7.py `inputs`):
/// registry manifests and tarball license collections.
pub struct Inputs {
    registry: std::path::PathBuf,
    bundled: std::path::PathBuf,
    cache: std::cell::RefCell<HashMap<(String, Option<String>), Deps>>,
}

fn safe(v: &str) -> String {
    v.replace('/', "_")
}

impl Inputs {
    pub fn new(registry: &Path, bundled: &Path) -> Inputs {
        Inputs {
            registry: registry.to_path_buf(),
            bundled: bundled.to_path_buf(),
            cache: Default::default(),
        }
    }

    fn manifest(&self, pkg: &str, ver: Option<&str>) -> Option<Value> {
        let tries: Vec<&str> = match ver {
            Some(v) => vec![v, "latest"],
            None => vec!["latest"],
        };
        for v in tries {
            let p = self
                .registry
                .join(format!("{}@{}.json", pkg.replace('/', "__"), safe(v)));
            if let Ok(text) = std::fs::read_to_string(&p) {
                let d: Value = serde_json::from_str(&text).ok()?;
                return if d.get("__error").is_some() {
                    None
                } else {
                    Some(d)
                };
            }
        }
        None
    }

    fn bundled(&self, pkg: &str, ver: Option<&str>) -> BTreeSet<String> {
        let tries: Vec<&str> = match ver {
            Some(v) => vec![v, "latest"],
            None => vec!["latest"],
        };
        for v in tries {
            let p = self
                .bundled
                .join(format!("{}@{v}.json", pkg.replace('/', "__")));
            if let Ok(text) = std::fs::read_to_string(&p) {
                let d: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
                return names_of(d.get("bundled"));
            }
        }
        BTreeSet::new()
    }

    /// (runtime dependency names, build-input names that are not runtime).
    pub fn inputs(&self, pkg: &str, ver: Option<&str>) -> (BTreeSet<String>, BTreeSet<String>) {
        let key = (pkg.to_string(), ver.map(String::from));
        if let Some(hit) = self.cache.borrow().get(&key) {
            return hit.clone();
        }
        let m = self.manifest(pkg, ver).unwrap_or(Value::Null);
        let runtime: BTreeSet<String> = RUNTIME_KEYS
            .iter()
            .flat_map(|k| names_of(m.get(*k)))
            .collect();
        let mut build: BTreeSet<String> = BUILD_KEYS
            .iter()
            .flat_map(|k| names_of(m.get(*k)))
            .collect();
        build.extend(self.bundled(pkg, ver));
        let build = build.difference(&runtime).cloned().collect();
        let out = (runtime, build);
        self.cache.borrow_mut().insert(key, out.clone());
        out
    }
}

/// Python's `set(m.get(k) or ())`: a dict's keys, a list's items.
fn names_of(v: Option<&Value>) -> BTreeSet<String> {
    match v {
        Some(Value::Object(o)) => o.keys().cloned().collect(),
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(|x| x.as_str().map(String::from))
            .collect(),
        _ => BTreeSet::new(),
    }
}

/// origins.py `chunk_origin`.
fn chunk_origin(
    owner: &str,
    version: Option<&str>,
    rest: &str,
    inputs: &Inputs,
    fam: &Families,
) -> Option<String> {
    let (runtime, build) = inputs.inputs(owner, version);
    if build.is_empty() {
        return None;
    }
    chunk_candidates(rest).into_iter().find(|c| {
        build.contains(c) && !runtime.contains(c) && c != owner && !fam.same(Some(c), Some(owner))
    })
}

/// One origin: (kind, package).
pub type Origin = Option<(String, String)>;

/// origins.py `Origins`: origin(source index, original line).
pub struct Origins {
    whole: HashMap<usize, (String, String)>,
    regions: HashMap<usize, (Vec<usize>, Vec<Origin>)>,
}

/// The chunk-rule context: per-source versions, the cached inputs and the
/// family judgement.
pub struct ChunkCtx<'a> {
    pub versions: &'a [Option<String>],
    pub inputs: &'a Inputs,
    pub fam: &'a Families,
}

/// Scorer 4's regions of one shipped file: marker origins only.
fn regions4(c: &str, ship: &str) -> Vec<(usize, Origin)> {
    marker_regions(c)
        .into_iter()
        .map(|(ln, o)| {
            (
                ln,
                o.filter(|o| o != ship).map(|o| ("marker".to_string(), o)),
            )
        })
        .collect()
}

/// Scorer 7-12's regions of one shipped file: markers, lazy bodies, and a
/// region's nested chunk named after a build input of the region's package.
fn regions7(c: &str, ship: &str, rules: Rules, ctx: Option<&ChunkCtx>) -> Vec<(usize, Origin)> {
    let mut r: Vec<(usize, Origin)> = Vec::new();
    for (ln, o, mp, kind) in marker_regions7(c, rules.lazybody, rules.patched) {
        let Some(o) = o.filter(|o| o != ship) else {
            r.push((ln, None));
            continue;
        };
        let mp = mp.unwrap_or_default();
        let nested = if rules.chunk {
            let ctx = ctx.expect("the chunk rule needs its context");
            let rest = mp
                .rsplit_once(&format!("node_modules/{o}/"))
                .map_or("", |x| x.1);
            (!rest.is_empty())
                .then(|| {
                    chunk_origin(
                        &o,
                        path_version(&mp, &o).as_deref(),
                        rest,
                        ctx.inputs,
                        ctx.fam,
                    )
                })
                .flatten()
        } else {
            None
        };
        match nested {
            Some(nested) => {
                let kind = if rules.via {
                    format!("{VIA}{o}")
                } else {
                    "chunk".into()
                };
                r.push((ln, Some((kind, nested))));
            }
            None => r.push((ln, Some((kind.unwrap_or("marker").to_string(), o)))),
        }
    }
    r
}

/// The chunk rule on a shipper's own file: the package it is named after.
fn whole_chunk(p: &str, ship: &str, si: usize, ctx: &ChunkCtx) -> Option<String> {
    if !p.contains("node_modules/") || ship.is_empty() || ctx.versions.is_empty() {
        return None;
    }
    let rest = p
        .rsplit_once(&format!("node_modules/{ship}/"))
        .map_or("", |x| x.1);
    if rest.is_empty() {
        return None;
    }
    chunk_origin(ship, ctx.versions[si].as_deref(), rest, ctx.inputs, ctx.fam)
}

impl Origins {
    pub fn new(
        paths: &[Option<String>],
        contents: &[Option<String>],
        shippers: &[String],
        rules: Rules,
        ctx: Option<&ChunkCtx>,
    ) -> Origins {
        let mut whole = HashMap::new();
        let mut regions = HashMap::new();
        // Python zips paths with contents (`contents or [None] * len(paths)`):
        // a source past the end of sourcesContent is never looked at.
        let n = if contents.is_empty() {
            paths.len()
        } else {
            paths.len().min(contents.len())
        };
        for (si, p) in paths.iter().enumerate().take(n) {
            let p = p.as_deref().unwrap_or("");
            let c = contents
                .get(si)
                .and_then(|c| c.as_deref())
                .filter(|c| !c.is_empty());
            let ship = &shippers[si];
            let inside = p.rsplit_once("node_modules/").map_or(p, |x| x.1);
            if let Some(m) = pyre::vendor_dir(inside).filter(|m| m != ship) {
                whole.insert(si, ("path".to_string(), m.to_string()));
                continue;
            }
            let scorer4 = !rules.lazybody && !rules.chunk;
            let r = match c {
                Some(c) if scorer4 => regions4(c, ship),
                Some(c) => regions7(c, ship, rules, ctx),
                None => Vec::new(),
            };
            if r.iter().any(|x| x.1.is_some()) {
                regions.insert(si, r.into_iter().unzip());
            } else if rules.chunk {
                let ctx = ctx.expect("the chunk rule needs its context");
                if let Some(o) = whole_chunk(p, ship, si, ctx) {
                    whole.insert(si, ("chunk".to_string(), o));
                }
            }
        }
        Origins { whole, regions }
    }

    pub fn at(&self, si: usize, oline: i64) -> Option<&(String, String)> {
        if let Some(w) = self.whole.get(&si) {
            return Some(w);
        }
        let (lines, origins) = self.regions.get(&si)?;
        let k = lines.partition_point(|&l| (l as i64) <= oline);
        if k == 0 {
            None
        } else {
            origins[k - 1].as_ref()
        }
    }
}

/// origins.py `origin_runs`: per segment its origin, run-length coded.
pub fn origin_runs(segs: &[(i64, i64, i64)], origins: &Origins) -> Runs {
    let (mut names, mut kinds, mut runs) = (Vec::new(), Vec::new(), Vec::new());
    let (mut ni, mut ki): (HashMap<String, i64>, HashMap<String, i64>) =
        (HashMap::new(), HashMap::new());
    let mut prev: Option<Option<&(String, String)>> = None;
    for (k, &(src, ol, _)) in segs.iter().enumerate() {
        let o = origins.at(src as usize, ol);
        // Python starts with prev = None, the same value as "no origin".
        let same = match prev {
            None => o.is_none(),
            Some(p) => p == o,
        };
        if !same {
            match o {
                None => runs.push((k as i64, -1, -1)),
                Some((kind, name)) => {
                    let n = *ni.entry(name.clone()).or_insert_with(|| {
                        names.push(name.clone());
                        names.len() as i64 - 1
                    });
                    let kd = *ki.entry(kind.clone()).or_insert_with(|| {
                        kinds.push(kind.clone());
                        kinds.len() as i64 - 1
                    });
                    runs.push((k as i64, n, kd));
                }
            }
        }
        prev = Some(o);
    }
    (names, kinds, runs)
}
