//! `library corpus freeze <corpus>`: freeze a corpus — copy every input
//! bundle and map under `<DATA>/corpus/<id>/`, build its ground truth
//! under `<DATA>/truth/`, and write its manifest (`corpus-<corpus>.json`)
//! with every sha256 — exactly as the Python freezers did:
//! experiments/096 `corpus.py` (`map_truth`, `sources_table`,
//! `install_versions`, `pnpm_version`, `cc_truth`, `freeze_cc`),
//! experiments/181 `kind_corpus.py freeze` (the `kind-*` corpora) and
//! experiments/150 `bun_corpus.py freeze` (`bun-dev`, `bun-held`).
//!
//! A corpus's RECIPE says which inputs make its entries (the build
//! outputs the `build` step left, the plan that drew the apps); the dev
//! corpus's recipe read inputs this host no longer has
//! (`/work/exp095-index-v2-data/eval`) and is not ported.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::decoder::{self, Inner};
use super::py::{compact, dict, from_json, int, kv, opt_s, s};
use super::pypath;
use crate::json::py::PyValue;

/// How an entry's truth is made (corpus.py `build_truth`'s `how`).
#[derive(Clone, Debug)]
pub enum How {
    /// Source-map truth; versions from the install under `base`.
    Install { base: String, app: Option<String> },
    /// Source-map truth through an inner map; versions from pnpm paths.
    Pnpm { inner: String },
    /// Claude Code's labelled sample.
    Cc { neg: String },
}

/// One planned entry: its manifest fields, its input files by role, and
/// how its truth is made.
pub struct Planned {
    pub meta: Vec<(String, PyValue)>,
    pub files: Vec<(&'static str, String)>,
    pub how: How,
}

/// A corpus's freeze plan: the manifest's head fields and its entries.
pub struct Plan {
    pub head: Vec<(String, PyValue)>,
    pub entries: Vec<Planned>,
}

// ---------------------------------------------------------------- truth

/// truth.py `package_root`: (package, its root in the source string,
/// the file inside it).
fn package_root(src: &str) -> Option<(String, String, String)> {
    let k = src.rfind("node_modules/")?;
    let (head, rest) = (&src[..k], &src[k + "node_modules/".len()..]);
    let parts: Vec<&str> = rest.split('/').collect();
    let n = if parts[0].starts_with('@') { 2 } else { 1 };
    let n = n.min(parts.len());
    let pkg = parts[..n].join("/");
    Some((
        pkg.clone(),
        format!("{head}node_modules/{pkg}"),
        parts[n..].join("/"),
    ))
}

/// corpus.py `sources_table`.
fn sources_table(
    sources: &[Value],
    app: Option<&str>,
    version_of: &mut dyn FnMut(&str, &str) -> PyValue,
) -> PyValue {
    let mut out = Vec::new();
    for src in sources {
        let src = src.as_str().unwrap_or("");
        let row = if src.starts_with('\0') {
            vec![s("<unmapped>"), PyValue::Null, s(src)]
        } else if ["http:", "https:", "webpack:"]
            .iter()
            .any(|p| src.starts_with(p))
        {
            vec![s("<foreign>"), PyValue::Null, s(src)]
        } else {
            match package_root(src) {
                None => vec![s("<app>"), PyValue::Null, PyValue::Null],
                Some((pkg, _, f)) if app == Some(pkg.as_str()) => {
                    vec![s("<self>"), PyValue::Null, s(&f)]
                }
                Some((pkg, root, f)) => vec![s(&pkg), version_of(&root, &pkg), s(&f)],
            }
        };
        out.push(PyValue::List(row));
    }
    PyValue::List(out)
}

/// corpus.py `install_versions(base)`: package.json's `version` beside
/// the mapped file, cached per path.
fn install_version(base: &str, root: &str, cache: &mut BTreeMap<String, PyValue>) -> PyValue {
    let p = pypath::normpath(&pypath::join(&pypath::join(base, root), "package.json"));
    if let Some(v) = cache.get(&p) {
        return v.clone();
    }
    let v = std::fs::read_to_string(&p)
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|j| match j {
            Value::Object(m) => Some(m.get("version").map_or(Ok(PyValue::Null), from_json)),
            _ => None,
        })
        .and_then(Result::ok)
        .unwrap_or(PyValue::Null);
    cache.insert(p, v.clone());
    v
}

/// corpus.py `pnpm_version` (`re.search(r"\.pnpm/([^/]+)/node_modules/", root)`).
fn pnpm_version(root: &str) -> PyValue {
    let mut from = 0;
    while let Some(i) = root[from..].find(".pnpm/") {
        let at = from + i + ".pnpm/".len();
        let seg_end = root[at..].find('/').map_or(root.len(), |j| at + j);
        if seg_end > at && root[seg_end..].starts_with("/node_modules/") {
            let at = root[at..seg_end].replace('+', "/");
            let at = at.split('_').next().unwrap_or("");
            return match at.get(1..).and_then(|x| x.rfind('@')) {
                Some(i) => s(&at[i + 2..]),
                None => PyValue::Null,
            };
        }
        from = from + i + 1;
    }
    PyValue::Null
}

/// Python's text-mode read: UTF-8, universal newlines.
fn read_text(p: &Path) -> Result<String, String> {
    let t = std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?;
    Ok(if t.contains('\r') {
        t.replace("\r\n", "\n").replace('\r', "\n")
    } else {
        t
    })
}

fn read_value(p: &Path) -> Result<Value, String> {
    let t = std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?;
    serde_json::from_str(&super::corpus::lone_surrogates_replaced(&t))
        .map_err(|e| format!("{}: {e}", p.display()))
}

/// A source's version, from its package root (and name).
type VersionOf = Box<dyn FnMut(&str, &str) -> PyValue>;

/// corpus.py `map_truth`.
fn map_truth(js: &str, how: &How) -> Result<PyValue, String> {
    let text = read_text(Path::new(js))?;
    let smap = super::corpus::read_map(Path::new(&format!("{js}.map")))?;
    let (segs, sources, app, mut version_of): (_, _, Option<String>, VersionOf) = match how {
        How::Pnpm { inner } => {
            let imap = super::corpus::read_map(Path::new(inner))?;
            let inner_lines = Inner {
                lines: decoder::decode(&imap.mappings, false)?,
                offset: 1,
            };
            let segs = decoder::segments(&text, &smap.mappings, Some(&inner_lines), false, false)?;
            (
                segs,
                imap.sources,
                None,
                Box::new(|root: &str, _: &str| pnpm_version(root)),
            )
        }
        How::Install { base, app } => {
            let segs = decoder::segments(&text, &smap.mappings, None, false, false)?;
            let base = base.clone();
            let mut cache = BTreeMap::new();
            (
                segs,
                smap.sources,
                app.clone(),
                Box::new(move |root: &str, _: &str| install_version(&base, root, &mut cache)),
            )
        }
        How::Cc { .. } => return Err("the cc truth is a labelled sample, not a map".into()),
    };
    let table = sources_table(&sources, app.as_deref(), &mut *version_of);
    let mut seg_rows = Vec::with_capacity(segs.len());
    for x in &segs {
        let src = x.1.ok_or("an unmapped segment in scorer 4's decode")?;
        seg_rows.push(PyValue::List(vec![int(x.0), int(src)]));
    }
    let segments = PyValue::List(seg_rows);
    Ok(dict(vec![
        kv("schema", s("exp096-truth/1")),
        kv("method", s("sourcemap")),
        kv("appPackage", opt_s(app.as_deref())),
        kv("sources", table),
        kv("segments", segments),
    ]))
}

/// corpus.py `cc_truth`: Claude Code's labelled sample (never committed).
fn cc_truth(neg: &str) -> Result<PyValue, String> {
    let mut specs: BTreeMap<String, String> = BTreeMap::new();
    let spec_file = format!("{neg}/exactlab/specs.txt");
    if let Ok(t) = std::fs::read_to_string(&spec_file) {
        for line in t.lines() {
            if line.starts_with('#') || !line.contains('|') {
                continue;
            }
            for spec in line.split('|').nth(1).unwrap_or("").split_whitespace() {
                if let Some((name, ver)) = spec.rsplit_once('@') {
                    specs
                        .entry(name.to_string())
                        .or_insert_with(|| ver.to_string());
                }
            }
        }
    }
    let meta = read_value(Path::new(&format!("{neg}/data/sample-meta.json")))?;
    let strata = meta
        .get("strata")
        .and_then(Value::as_object)
        .ok_or("sample-meta: no strata")?;
    let labels = std::fs::read_to_string(format!("{neg}/labels.jsonl"))
        .map_err(|e| format!("labels.jsonl: {e}"))?;
    let mut rows: Vec<(i64, PyValue)> = Vec::new();
    for line in labels.lines().filter(|l| !l.trim().is_empty()) {
        let r: Value = serde_json::from_str(line).map_err(|e| format!("labels.jsonl: {e}"))?;
        let stratum = r["stratum"].as_str().unwrap_or("");
        if !strata.contains_key(stratum) {
            continue;
        }
        let lab = r["label"].as_str().unwrap_or("");
        let label = match lab.strip_prefix("library:") {
            Some(p) => p.to_string(),
            None => format!("<{lab}>"),
        };
        let start = r["start"].as_i64().ok_or("a label row without start")?;
        let mut row = vec![
            kv("s", from_json(&r["start"])?),
            kv("e", from_json(&r["end"])?),
            kv("b", from_json(&r["bytes"])?),
            kv("stratum", s(stratum)),
            kv("label", s(&label)),
        ];
        if lab.starts_with("library:") {
            let pkg = r["pkg"].as_str().unwrap_or("");
            let known = r
                .get("version_if_known")
                .filter(|v| v.as_str().is_some_and(|x| !x.is_empty()));
            let version = match known {
                Some(v) => from_json(v)?,
                None => opt_s(specs.get(pkg).map(String::as_str)),
            };
            row.push(kv("pkg", from_json(&r["pkg"])?));
            row.push(kv("version", version));
        }
        rows.push((start, dict(row)));
    }
    rows.sort_by_key(|r| r.0);
    Ok(dict(vec![
        kv("schema", s("exp096-truth/1")),
        kv("method", s("sample")),
        kv("strata", from_json(&meta["strata"])?),
        kv("seed", from_json(&meta["seed"])?),
        kv("populationUnits", from_json(&meta["populationUnits"])?),
        kv("populationBytes", from_json(&meta["populationBytes"])?),
        kv(
            "rows",
            PyValue::List(rows.into_iter().map(|r| r.1).collect()),
        ),
    ]))
}

fn build_truth(p: &Planned) -> Result<PyValue, String> {
    match &p.how {
        How::Cc { neg } => cc_truth(neg),
        how => {
            let js = &p
                .files
                .iter()
                .find(|f| f.0 == "bundle")
                .ok_or("no bundle")?
                .1;
            map_truth(js, how)
        }
    }
}

// ---------------------------------------------------------------- freeze

/// What a freeze wrote: the manifest (its text) and the manifest's id.
pub struct Frozen {
    pub manifest: String,
    pub entries: usize,
}

fn sha_file(p: &Path) -> Result<String, String> {
    super::corpus::sha256_file(p)
}

/// Freeze `plan` into `data` (`corpus/<id>/`, `truth/<id>.truth.json.gz`).
pub fn freeze(plan: Plan, data: &Path) -> Result<Frozen, String> {
    let truths = humanify_ag_core::par::map_ordered(&plan.entries, build_truth);
    let mut entries = Vec::new();
    for (p, truth) in plan.entries.iter().zip(truths) {
        let truth = truth?;
        let id = p
            .meta
            .iter()
            .find(|(k, _)| k == "id")
            .and_then(|(_, v)| match v {
                PyValue::Str(x) => Some(x.clone()),
                _ => None,
            })
            .ok_or("a planned entry without an id")?;
        let dir = data.join("corpus").join(&id);
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let mut files = Vec::new();
        for (role, src) in &p.files {
            let name = pypath::basename(src).to_string();
            let dst = dir.join(&name);
            if !dst.exists() || sha_file(&dst)? != sha_file(Path::new(src))? {
                std::fs::copy(src, &dst).map_err(|e| format!("{src}: {e}"))?;
            }
            let bytes = std::fs::metadata(&dst).map_err(|e| e.to_string())?.len();
            files.push(kv(
                role,
                dict(vec![
                    kv("name", s(&name)),
                    kv("sha256", s(&sha_file(&dst)?)),
                    kv("bytes", int(bytes as i64)),
                    kv("origin", s(src)),
                ]),
            ));
        }
        let tp: PathBuf = data.join("truth").join(format!("{id}.truth.json.gz"));
        super::gz::write_gz(&tp, compact(&truth).as_bytes())?;
        let method = match p.how {
            How::Cc { .. } => "sample",
            _ => "sourcemap",
        };
        let mut e = p.meta.clone();
        e.push(kv("files", dict(files)));
        e.push(kv(
            "truth",
            dict(vec![
                kv("method", s(method)),
                kv("sha256", s(&sha_file(&tp)?)),
            ]),
        ));
        entries.push(dict(e));
    }
    let n = entries.len();
    let mut head = plan.head;
    head.push(kv("entries", PyValue::List(entries)));
    Ok(Frozen {
        manifest: crate::json::py_text(&dict(head), true),
        entries: n,
    })
}

// ---------------------------------------------------------------- recipes

/// A plan row's dev / held split. A constant: env_guard's "a setting read
/// out of a JSON map" rule would take the plan's field for the pipeline's
/// `split` setting.
pub const PLAN_SPLIT: &str = "split";

/// exp150's apps (`/work/exp150-data/bun`): the draw and the builds.
pub const BUN_APPS: &str = "/work/exp150-data/bun";
/// exp181's bundler-kind builds.
pub const KIND_DATA: &str = "/work/exp181-data";
/// Claude Code 2.1.216's extracted entry and its labelled sample.
pub const CC_BUNDLE: &str = "/Users/andrewgross/Development/claude-code-versions/inputs/claude-code-2.1.216/binary-decompiled/src/entrypoints/index.js";
pub const CC_NEG: &str = "/work/cc-library-truth/2.1.216";

fn apps_plan() -> Result<(Vec<Value>, BTreeMap<String, Value>), String> {
    let plan = read_value(Path::new(&format!("{BUN_APPS}/plan.json")))?;
    let built = read_value(Path::new(&format!("{BUN_APPS}/built.json")))?;
    let apps = plan["apps"]
        .as_array()
        .cloned()
        .ok_or("plan.json: no apps")?;
    let built = built
        .as_array()
        .ok_or("built.json is not a list")?
        .iter()
        .filter_map(|r| Some((r["id"].as_str()?.to_string(), r.clone())))
        .collect();
    Ok((apps, built))
}

fn text_of(v: &Value, k: &str) -> String {
    v[k].as_str().unwrap_or("").to_string()
}

fn app_package(a: &Value) -> String {
    if a["kind"].as_str() == Some("cli") {
        text_of(a, "name")
    } else {
        "-".into()
    }
}

/// kind_corpus.py `do_freeze`.
fn kind_plan(corpus: &str, kind: &str) -> Result<Plan, String> {
    let desc = match kind {
        "esbuild" => {
            "esbuild 0.28.2 --bundle --minify --platform=node --format=cjs --sourcemap (exp100's recipe)"
        }
        "webpack" => {
            "webpack 5.111.1 production + terser, target node, one chunk (the index's webpack recipe script)"
        }
        _ => {
            "rollup 4.64.5 + node-resolve + commonjs + json + terser, cjs (the index's rollup recipe script)"
        }
    };
    let (apps, bun_built) = apps_plan()?;
    let kbuilt = read_value(Path::new(&format!("{KIND_DATA}/built.json")))?;
    let kbuilt: BTreeMap<String, Value> = kbuilt
        .as_array()
        .ok_or("built.json is not a list")?
        .iter()
        .filter_map(|r| Some((r["id"].as_str()?.to_string(), r.clone())))
        .collect();
    let mut entries = Vec::new();
    for a in &apps {
        let id = text_of(a, "id");
        let dev = a[PLAN_SPLIT].as_str() == Some("dev")
            && bun_built
                .get(&id)
                .is_some_and(|r| r["node"].as_bool() == Some(true));
        if !dev
            || kbuilt
                .get(&id)
                .is_none_or(|r| r[kind].as_bool() != Some(true))
        {
            continue;
        }
        let slug = text_of(a, "slug");
        let js = format!("{KIND_DATA}/build/{kind}/{slug}/entry.js");
        let app = app_package(a);
        entries.push(Planned {
            meta: vec![
                kv("id", s(&format!("{corpus}-{slug}"))),
                kv("set", s(corpus)),
                kv("bundler", s(kind)),
                kv("builtWith", s(kind)),
                kv("target", s("node")),
                kv("appPackage", s(&app)),
                kv("kind", from_json(&a["kind"])?),
                kv("stratum", from_json(&a["stratum"])?),
                kv("description", s(&format!("exp181 app {id}: {desc}"))),
            ],
            files: vec![("bundle", js.clone()), ("map", format!("{js}.map"))],
            how: How::Install {
                base: format!("{BUN_APPS}/apps/{slug}"),
                app: Some(app),
            },
        });
    }
    Ok(Plan {
        head: vec![
            kv("schema", s("exp096-corpus/1")),
            kv("corpus", s(corpus)),
            kv("frozen", PyValue::Bool(true)),
            kv(
                "note",
                s(&format!(
                    "exp181 DEV corpus of {kind} builds of exp150's 45 bun-dev apps (experiments/181-index-all-bundlers/scripts/kind_corpus.py)"
                )),
            ),
        ],
        entries,
    })
}

/// bun_corpus.py `do_freeze`.
fn bun_plan(corpus: &str) -> Result<Plan, String> {
    let (split, note) = match corpus {
        "bun-dev" => (
            "dev",
            "exp150 DEV corpus of Bun builds: exp108 TRAINING apps in no other corpus, built by Bun 1.4.0 with --target node and --target bun (experiments/150-bun-testbed/scripts/bun_corpus.py)",
        ),
        _ => (
            "held",
            "exp150 HELD-OUT corpus of Bun builds (same recipe and draw as bun-dev, the split's other side); private: scored once per candidate, summaries only, never tuned on or diagnosed",
        ),
    };
    let (apps, built) = apps_plan()?;
    let mut entries = Vec::new();
    for t in ["node", "bun"] {
        for a in &apps {
            let id = text_of(a, "id");
            let ok = built.get(&id).is_some_and(|r| {
                r["node"].as_bool() == Some(true) && r["bun"].as_bool() == Some(true)
            });
            if a[PLAN_SPLIT].as_str() != Some(split) || !ok {
                continue;
            }
            let slug = text_of(a, "slug");
            let js = format!("{BUN_APPS}/build/bun-{t}/{slug}/entry.js");
            let app = app_package(a);
            entries.push(Planned {
                meta: vec![
                    kv("id", s(&format!("{corpus}-{t}-{slug}"))),
                    kv("set", s(&format!("{corpus}-{t}"))),
                    kv("bundler", s("bun")),
                    kv("builtWith", s("bun")),
                    kv("target", s(t)),
                    kv("appPackage", s(&app)),
                    kv("kind", from_json(&a["kind"])?),
                    kv("stratum", from_json(&a["stratum"])?),
                    kv(
                        "description",
                        s(&format!(
                            "exp150 app {id}: Bun 1.4.0 build --target {t} --format cjs --minify --sourcemap=external (exp100's recipe, target {t})"
                        )),
                    ),
                ],
                files: vec![("bundle", js.clone()), ("map", format!("{js}.map"))],
                how: How::Install {
                    base: pypath::dirname(&js),
                    app: Some(app),
                },
            });
        }
    }
    Ok(Plan {
        head: vec![
            kv("schema", s("exp096-corpus/1")),
            kv("corpus", s(corpus)),
            kv("frozen", PyValue::Bool(true)),
            kv("note", s(note)),
        ],
        entries,
    })
}

/// corpus.py `freeze_cc`.
fn cc_plan() -> Plan {
    Plan {
        head: vec![
            kv("schema", s("exp096-corpus/1")),
            kv("corpus", s("cc")),
            kv("frozen", PyValue::Bool(true)),
            kv(
                "note",
                s(
                    "exp150 Claude Code corpus: 2.1.216 scored on the seed-95 redraw of exp095's negatives sample, re-labelled (labels and truth under /work/cc-library-truth/, never committed)",
                ),
            ),
        ],
        entries: vec![Planned {
            meta: vec![
                kv("id", s("cc-2.1.216")),
                kv("set", s("cc")),
                kv("bundler", s("bun")),
                kv(
                    "description",
                    s(
                        "Claude Code 2.1.216, the bun --compile binary's extracted entry (Bun, --target bun, minified)",
                    ),
                ),
            ],
            files: vec![("bundle", CC_BUNDLE.to_string())],
            how: How::Cc {
                neg: CC_NEG.to_string(),
            },
        }],
    }
}

/// The freeze plan of `corpus`, or why it has none.
pub fn plan(corpus: &str) -> Result<Plan, String> {
    match corpus {
        "kind-esbuild" | "kind-webpack" | "kind-rollup" => {
            kind_plan(corpus, &corpus["kind-".len()..])
        }
        "bun-dev" | "bun-held" => bun_plan(corpus),
        "cc" => Ok(cc_plan()),
        other => Err(format!(
            "corpus {other}: no freeze recipe (its inputs are not on this host; the Python freezer that made it read paths that are gone)"
        )),
    }
}

/// `library corpus freeze <corpus> --out DIR`: freeze into DIR (data under
/// it, the manifest beside), then compare the manifest's identity and,
/// when it is the committed corpus, re-freeze its origin layer from the
/// new data and compare that too. (all reproduce, report text)
pub fn run(repo: &Path, name: &str, out: &Path) -> Result<(bool, String), String> {
    let corpus = crate::committed::library::corpus_named(name)?;
    let frozen = freeze(plan(name)?, out)?;
    let mpath = out.join(format!("corpus{}.json", corpus.suffix));
    std::fs::write(&mpath, &frozen.manifest).map_err(|e| format!("{}: {e}", mpath.display()))?;
    let id = crate::committed::sha256_hex(frozen.manifest.as_bytes());
    let mut text = format!("froze {} entries -> {}\n", frozen.entries, mpath.display());
    let committed = super::corpus::Corpus::open(repo, name, Some(&out.to_string_lossy()));
    let Ok(c) = committed else {
        text.push_str("no committed manifest to compare: a NEW corpus (commit the manifest, then `library layers`)\n");
        return Ok((true, text));
    };
    let same = c.loaded.manifest_id == id;
    text.push_str(&format!(
        "manifest {} the committed corpus{}.json ({})\n",
        if same { "IDENTICAL to" } else { "DIFFERS from" },
        corpus.suffix,
        &id[..12]
    ));
    if !same {
        return Ok((false, text));
    }
    let layer = &super::layers::ORIGIN_LAYERS[0];
    let r = super::layers::refreeze_origin_layer(
        &c,
        layer,
        out,
        Path::new(super::layers::REGISTRY),
        Path::new(super::layers::BUNDLED),
    )?;
    text.push_str(&format!(
        "origins: {}/{} entries reproduce the committed sha; manifest {}\n",
        r.same,
        r.entries,
        if r.manifest_same {
            "identical"
        } else {
            "DIFFERS"
        }
    ));
    Ok((r.differ.is_empty() && r.manifest_same, text))
}

#[cfg(test)]
#[path = "freeze_test.rs"]
mod freeze_test;
