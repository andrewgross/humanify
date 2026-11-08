//! `library corpus build <corpus>`: build a corpus's bundles from its
//! apps' installs — the step before `freeze`. Ported from
//! experiments/181 `kind_corpus.py build` (esbuild with exp100's recipe,
//! webpack / rollup with the index's own recipe scripts, the store's
//! pinned tools), experiments/150 `bun_corpus.py build` (Bun 1.4.0, both
//! targets) and the helpers both import: experiments/100 `build.py`
//! (`build_esbuild`, `build_bun`, `done`, `record`) and experiments/095
//! `closure.py` (`run`, `bundle`'s retry loop, `install`, `entry`).
//!
//! A build that already has its `build.json` is kept (the Python's
//! `done`). An app that is not installed is installed (npm, `--before`
//! the index's as-of) when it is a CLI; an Electron app or VS Code
//! extension reads its dependencies from GitHub through exp108's rule,
//! which is not ported: such an app must already be installed. The DRAW
//! (bun_corpus.py `plan` / `replace`, Python's seeded RNG) is not ported:
//! its `plan.json` is the input.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::Value;

use super::freeze::{BUN_APPS, KIND_DATA, PLAN_SPLIT};
use super::pypath;
use crate::proc::{self, Spawn};

pub const KIND_ESBUILD: &str =
    "/work/exp181-data/store/tools/esbuild-0.28.2/node_modules/.bin/esbuild";
pub const KIND_WEBPACK: &str = "/work/exp181-data/store/tools/webpack-5.111.1/webpack-build.cjs";
pub const KIND_ROLLUP: &str = "/work/exp181-data/store/tools/rollup-4.64.5/rollup-build.mjs";
pub const BUN: &str = "/work/tools/bun-1.4.0/node_modules/.bin/bun";
/// bun_corpus.py `BEFORE`: the index's as-of, so no version it could not hold is installed.
pub const NPM_BEFORE: &str = "2026-10-15T23:59:59.999Z";
const NPM_FLAGS: [&str; 7] = [
    "install",
    "--ignore-scripts",
    "--omit=dev",
    "--legacy-peer-deps",
    "--no-audit",
    "--no-fund",
    "--loglevel=error",
];

/// closure.py `run`: (exit code, stdout + stderr); 124 "timeout".
fn run(cmd: &[String], cwd: &Path, timeout_s: u64) -> (i32, String) {
    let args: Vec<&str> = cmd[1..].iter().map(String::as_str).collect();
    let spawn = Spawn::new(&cmd[0], &args, Duration::from_secs(timeout_s))
        .cwd(cwd)
        .env("GOMAXPROCS", "2");
    match proc::run(&spawn) {
        Ok(f) => (
            f.status.code().unwrap_or(-1),
            format!(
                "{}{}",
                String::from_utf8_lossy(&f.stdout),
                String::from_utf8_lossy(&f.stderr)
            ),
        ),
        Err(e) if e.contains("timed out") => (124, "timeout".into()),
        Err(e) => (127, e),
    }
}

/// `log[-n:]` (characters).
fn tail(log: &str, n: usize) -> String {
    let count = log.chars().count();
    log.chars().skip(count.saturating_sub(n)).collect()
}

// ---------------------------------------------------------------- JSON as json.dump writes it

/// A Python value for `json.dump` without `sort_keys` (insertion order).
#[derive(Clone, Debug)]
pub enum J {
    Bool(bool),
    Str(String),
    List(Vec<J>),
    Obj(Vec<(String, J)>),
}

fn write_j(out: &mut String, v: &J, indent: Option<usize>, depth: usize) {
    let nl = |out: &mut String, d: usize| {
        if let Some(n) = indent {
            out.push('\n');
            out.extend(std::iter::repeat_n(' ', n * d));
        }
    };
    let (item_sep, key_sep) = if indent.is_some() {
        (",", ": ")
    } else {
        (", ", ": ")
    };
    match v {
        J::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        J::Str(s) => crate::json::py::write_str(out, s),
        J::List(xs) if xs.is_empty() => out.push_str("[]"),
        J::List(xs) => {
            out.push('[');
            for (i, x) in xs.iter().enumerate() {
                if i > 0 {
                    out.push_str(item_sep);
                }
                nl(out, depth + 1);
                write_j(out, x, indent, depth + 1);
            }
            nl(out, depth);
            out.push(']');
        }
        J::Obj(m) if m.is_empty() => out.push_str("{}"),
        J::Obj(m) => {
            out.push('{');
            for (i, (k, x)) in m.iter().enumerate() {
                if i > 0 {
                    out.push_str(item_sep);
                }
                nl(out, depth + 1);
                crate::json::py::write_str(out, k);
                out.push_str(key_sep);
                write_j(out, x, indent, depth + 1);
            }
            nl(out, depth);
            out.push('}');
        }
    }
}

/// `json.dumps(v)` / `json.dumps(v, indent=n)`.
pub fn dumps(v: &J, indent: Option<usize>) -> String {
    let mut out = String::new();
    write_j(&mut out, v, indent, 0);
    out
}

fn strs(xs: &[String]) -> J {
    J::List(xs.iter().cloned().map(J::Str).collect())
}

// ---------------------------------------------------------------- builders

/// One build's record (build.json).
pub struct Built {
    pub ok: bool,
    pub record: J,
}

/// build.py `done`.
fn done(o: &Path) -> Option<bool> {
    let text = std::fs::read_to_string(o.join("build.json")).ok()?;
    let v: Value = serde_json::from_str(&text).ok()?;
    Some(v["ok"].as_bool() == Some(true))
}

/// The Bun node build's externals / loaders / format (exp150 recorded them).
struct Closure {
    externals: Vec<String>,
    loaders: Vec<String>,
    esm: bool,
}

fn bun_closure(slug: &str) -> Result<Closure, String> {
    let p = format!("{BUN_APPS}/build/bun-node/{slug}/build.json");
    let text = std::fs::read_to_string(&p).map_err(|e| format!("{p}: {e}"))?;
    let v: Value = serde_json::from_str(&text).map_err(|e| format!("{p}: {e}"))?;
    let list = |k: &str| -> Vec<String> {
        v[k].as_array()
            .map(|xs| {
                xs.iter()
                    .filter_map(Value::as_str)
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_default()
    };
    Ok(Closure {
        externals: list("externals"),
        loaders: list("loaders"),
        esm: v["esm"].as_bool() == Some(true),
    })
}

/// build.py `build_esbuild` (the store's esbuild).
fn build_esbuild(d: &Path, o: &Path, cl: &Closure) -> Built {
    let fmt = if cl.esm { "esm" } else { "cjs" };
    let mut cmd: Vec<String> = [
        KIND_ESBUILD,
        "entry.cjs",
        "--bundle",
        "--minify",
        "--platform=node",
    ]
    .iter()
    .map(|x| x.to_string())
    .collect();
    cmd.push(format!("--format={fmt}"));
    cmd.push("--sourcemap".into());
    cmd.push(format!("--outfile={}/entry.js", o.display()));
    cmd.push("--log-level=error".into());
    cmd.push("--loader:.node=empty".into());
    cmd.push("--define:process.env.NODE_ENV=\"production\"".into());
    cmd.extend(cl.loaders.iter().map(|x| format!("--loader:{x}=empty")));
    cmd.extend(cl.externals.iter().map(|x| format!("--external:{x}")));
    let (rc, log) = run(&cmd, d, 1200);
    Built {
        ok: rc == 0,
        record: J::Obj(vec![
            ("ok".into(), J::Bool(rc == 0)),
            ("log".into(), J::Str(tail(&log, 1500))),
        ]),
    }
}

/// kind_corpus.py `recipe_script`: the index's recipe script, renamed entry.js.
fn recipe_script(script: &str, d: &Path, o: &Path, cl: &Closure) -> Built {
    let fmt = if cl.esm { "esm" } else { "cjs" };
    let mut cmd: Vec<String> = vec![
        "node".into(),
        "--max-old-space-size=8192".into(),
        script.into(),
        format!("{}/entry.cjs", d.display()),
        o.display().to_string(),
        fmt.into(),
    ];
    cmd.extend(cl.externals.iter().cloned());
    let (mut rc, mut log) = run(&cmd, d, 1800);
    if rc != 0 && log.contains("top-level await") && fmt == "cjs" {
        cmd[5] = "esm".into();
        (rc, log) = run(&cmd, d, 1800);
    }
    let out_js = o.join("out.js");
    if rc == 0 && out_js.exists() {
        let renamed = std::fs::read_to_string(&out_js).map(|t| {
            t.replace(
                "//# sourceMappingURL=out.js.map",
                "//# sourceMappingURL=entry.js.map",
            )
        });
        let moved = renamed.and_then(|t| {
            std::fs::write(o.join("entry.js"), t)?;
            std::fs::rename(o.join("out.js.map"), o.join("entry.js.map"))?;
            std::fs::remove_file(&out_js)
        });
        if let Err(e) = moved {
            log = e.to_string();
        } else {
            return Built {
                ok: true,
                record: J::Obj(vec![
                    ("ok".into(), J::Bool(true)),
                    ("log".into(), J::Str(String::new())),
                ]),
            };
        }
    }
    Built {
        ok: false,
        record: J::Obj(vec![
            ("ok".into(), J::Bool(false)),
            ("log".into(), J::Str(tail(&log, 1500))),
        ]),
    }
}

/// `re.findall(r'Could not resolve:? "([^"]+)"', text)`, sorted, unique.
fn unresolved(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for (i, _) in text.match_indices("Could not resolve") {
        let mut rest = &text[i + "Could not resolve".len()..];
        rest = rest.strip_prefix(':').unwrap_or(rest);
        if let Some(r) = rest.strip_prefix(" \"")
            && let Some(end) = r.find('"')
            && end > 0
        {
            out.push(r[..end].to_string());
        }
    }
    out.sort();
    out.dedup();
    out
}

/// `re.findall(r'No loader is configured for "(\.[^"]+)" files', text)`.
fn no_loader(text: &str) -> Vec<String> {
    let pre = "No loader is configured for \"";
    let mut out = Vec::new();
    for (i, _) in text.match_indices(pre) {
        let r = &text[i + pre.len()..];
        if let Some(end) = r.find('"')
            && end > 1
            && r.starts_with('.')
            && r[end..].starts_with("\" files")
        {
            out.push(r[..end].to_string());
        }
    }
    out
}

/// `re.findall(r"node_modules/((?:@[^/\s]+/)?[^/\s]+)/", text)`.
fn node_module_pkgs(text: &str) -> Vec<String> {
    let seg = |s: &str| -> usize {
        s.char_indices()
            .find(|(_, c)| *c == '/' || super::pyre::is_space(*c))
            .map_or(s.len(), |(i, _)| i)
    };
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(i) = text[from..].find("node_modules/") {
        let at = from + i + "node_modules/".len();
        let rest = &text[at..];
        let mut hit = None;
        if rest.starts_with('@') {
            let a = seg(rest);
            if a > 1 && rest[a..].starts_with('/') {
                let r2 = &rest[a + 1..];
                let b = seg(r2);
                if b > 0 && r2[b..].starts_with('/') {
                    hit = Some((rest[..a + 1 + b].to_string(), at + a + 1 + b + 1));
                }
            }
        }
        if hit.is_none() {
            let a = seg(rest);
            if a > 0 && rest[a..].starts_with('/') {
                hit = Some((rest[..a].to_string(), at + a + 1));
            }
        }
        match hit {
            Some((m, end)) => {
                out.push(m);
                from = end;
            }
            None => from = from + i + 1,
        }
    }
    out
}

/// closure.py `bundle(d, tool=BUN, ...)`: Bun with the retry loop that
/// finds the externals / loaders / ESM switch; (ok, externals, loaders, esm, log).
fn bundle_bun(
    d: &Path,
    o: &Path,
    entry_file: &str,
    target: &str,
) -> (bool, Vec<String>, Vec<String>, bool, String) {
    let (mut externals, mut loaders, mut esm) = (Vec::<String>::new(), Vec::<String>::new(), false);
    let deps: Vec<String> = std::fs::read_to_string(d.join("package.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| {
            v["dependencies"]
                .as_object()
                .map(|m| m.keys().cloned().collect())
        })
        .unwrap_or_default();
    let protect: Vec<String> = if deps.len() == 1 { deps } else { Vec::new() };
    let mut last = String::new();
    for _ in 0..12 {
        let mut cmd: Vec<String> = vec![
            BUN.into(),
            "build".into(),
            entry_file.into(),
            "--target".into(),
            target.into(),
            "--format".into(),
            if esm { "esm" } else { "cjs" }.into(),
            "--minify".into(),
            "--sourcemap=external".into(),
            "--outdir".into(),
            o.display().to_string(),
            "--define".into(),
            "process.env.NODE_ENV=\"production\"".into(),
        ];
        for x in &externals {
            cmd.push("--external".into());
            cmd.push(x.clone());
        }
        for x in loaders.iter().map(String::as_str).chain([".node"]) {
            cmd.push("--loader".into());
            cmd.push(format!("{x}:file"));
        }
        let rc;
        (rc, last) = run(&cmd, d, 1200);
        if rc == 0 {
            return (true, externals, loaders, esm, String::new());
        }
        let new: Vec<String> = unresolved(&last)
            .into_iter()
            .filter(|x| !externals.contains(x))
            .collect();
        let mut nl: Vec<String> = no_loader(&last)
            .into_iter()
            .filter(|x| !loaders.contains(x))
            .collect();
        nl.sort();
        nl.dedup();
        let mut bad: Vec<String> = node_module_pkgs(&last)
            .into_iter()
            .filter(|m| !externals.contains(m) && !protect.contains(m))
            .collect();
        bad.sort();
        bad.dedup();
        let tla = last.contains("top-level await")
            || last.contains("Top-level await")
            || last.contains("\"await\" can only be used inside an \"async\" function");
        if !new.is_empty() && externals.len() < 400 {
            externals.extend(new);
        } else if !nl.is_empty() {
            loaders.extend(nl);
        } else if tla && !esm {
            esm = true;
        } else if !bad.is_empty() && externals.len() < 400 {
            externals.extend(bad);
        } else {
            break;
        }
    }
    (false, externals, loaders, esm, tail(&last, 1500))
}

/// build.py `build_bun`, the top-level-await fallback to an ESM entry included.
fn build_bun(d: &Path, o: &Path, target: &str) -> Built {
    let (mut ok, mut ext, mut ld, mut esm, mut log) = bundle_bun(d, o, "entry.cjs", target);
    if !ok && log.contains("top-level await") {
        let entry = std::fs::read_to_string(d.join("entry.cjs")).unwrap_or_default();
        let mut specs = Vec::new();
        for (i, _) in entry.match_indices("require(\"") {
            let r = &entry[i + "require(".len()..];
            if let Some(end) = r[1..].find('"')
                && end > 0
                && r[1 + end + 1..].starts_with(')')
            {
                specs.push(r[..end + 2].to_string());
            }
        }
        let mjs: String = specs
            .iter()
            .enumerate()
            .map(|(i, x)| format!("import * as m{i} from {x};\nexport {{ m{i} }};\n"))
            .collect();
        if std::fs::write(d.join("entry.mjs"), mjs).is_ok() {
            (ok, ext, ld, esm, log) = bundle_bun(d, o, "entry.mjs", target);
        }
    }
    let ok = ok && o.join("entry.js").exists();
    Built {
        ok,
        record: J::Obj(vec![
            ("ok".into(), J::Bool(ok)),
            ("externals".into(), strs(&ext)),
            ("loaders".into(), strs(&ld)),
            ("esm".into(), J::Bool(esm)),
            ("log".into(), J::Str(log)),
        ]),
    }
}

/// `done(o)` else a fresh build recorded in `o/build.json`.
fn build_once(o: &Path, build: impl FnOnce() -> Built) -> Result<bool, String> {
    if let Some(ok) = done(o) {
        return Ok(ok);
    }
    let _ = std::fs::remove_dir_all(o);
    std::fs::create_dir_all(o).map_err(|e| format!("{}: {e}", o.display()))?;
    let b = build();
    std::fs::write(o.join("build.json"), dumps(&b.record, None)).map_err(|e| e.to_string())?;
    Ok(b.ok)
}

// ---------------------------------------------------------------- installs

/// closure.py `install` + `entry` for a CLI app (its published version,
/// npm `--before` the index's as-of); a non-CLI app must be installed.
fn ensure_installed(a: &Value, d: &Path) -> Result<(), String> {
    if d.join("entry.cjs").exists() && d.join(".installed").exists() {
        return Ok(());
    }
    if a["kind"].as_str() != Some("cli") {
        return Err(format!(
            "{}: not installed, and a non-CLI app's dependencies come from GitHub through exp108's rule (not ported)",
            a["id"].as_str().unwrap_or("?")
        ));
    }
    std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    let name = a["name"].as_str().ok_or("a CLI app without a name")?;
    let version = a["version"].as_str().ok_or("a CLI app without a version")?;
    let pj = J::Obj(vec![
        ("private".into(), J::Bool(true)),
        ("name".into(), J::Str("sel-app".into())),
        (
            "dependencies".into(),
            J::Obj(vec![(name.to_string(), J::Str(version.to_string()))]),
        ),
    ]);
    std::fs::write(d.join("package.json"), dumps(&pj, Some(1))).map_err(|e| e.to_string())?;
    let mut cmd: Vec<String> = std::iter::once("npm".to_string())
        .chain(NPM_FLAGS.iter().map(|x| x.to_string()))
        .chain([format!("--before={NPM_BEFORE}")])
        .collect();
    let (mut rc, mut out) = run(&cmd, d, 1800);
    if rc != 0 && out.contains("EBADPLATFORM") {
        cmd.push("--force".into());
        (rc, out) = run(&cmd, d, 1800);
    }
    if rc != 0 {
        return Err(format!("install failed: {}", tail(&out, 600)));
    }
    std::fs::write(d.join(".installed"), "").map_err(|e| e.to_string())?;
    // closure.py `entry` for a CLI: require its bin file(s) by path
    let root = d.join("node_modules").join(name);
    let pkg: Value = std::fs::read_to_string(root.join("package.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or(Value::Null);
    let bins: Vec<String> = match &pkg["bin"] {
        Value::Object(m) => m
            .values()
            .filter_map(Value::as_str)
            .map(String::from)
            .collect(),
        Value::String(b) => vec![b.clone()],
        _ => Vec::new(),
    };
    let mut bins: Vec<String> = bins
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    bins.sort();
    let mut lines = Vec::new();
    for p in bins {
        let f = pypath::normpath(&pypath::join(&root.to_string_lossy(), &p));
        if Path::new(&f).exists() {
            let mut q = String::new();
            crate::json::py::write_str(&mut q, &f);
            lines.push(format!("require({q});"));
        }
    }
    if lines.is_empty() {
        return Err("empty entry".into());
    }
    std::fs::write(
        d.join("entry.cjs"),
        format!("module.exports={{}};\n{}\n", lines.join("\n")),
    )
    .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------- corpora

fn read_list(p: &str) -> Result<Vec<Value>, String> {
    let text = std::fs::read_to_string(p).map_err(|e| format!("{p}: {e}"))?;
    let v: Value = serde_json::from_str(&text).map_err(|e| format!("{p}: {e}"))?;
    v.as_array()
        .cloned()
        .ok_or_else(|| format!("{p}: not a list"))
}

/// The plan's apps and the Bun builds' records.
fn plan_apps() -> Result<(Vec<Value>, BTreeMap<String, Value>), String> {
    let text =
        std::fs::read_to_string(format!("{BUN_APPS}/plan.json")).map_err(|e| e.to_string())?;
    let plan: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let built = read_list(&format!("{BUN_APPS}/built.json"))?
        .into_iter()
        .filter_map(|r| Some((r["id"].as_str()?.to_string(), r)))
        .collect();
    Ok((plan["apps"].as_array().cloned().unwrap_or_default(), built))
}

/// Build `corpus`'s bundles under `root` (the bundler outputs' data dir:
/// exp181's for the kind corpora, exp150's for Bun), `jobs` at a time;
/// `only` limits the apps by slug. Writes / merges `<root>/built.json`.
pub fn build(
    corpus: &str,
    root: &Path,
    jobs: usize,
    only: Option<&[String]>,
) -> Result<String, String> {
    let (apps, bun_built) = plan_apps()?;
    let keep =
        |a: &&Value| only.is_none_or(|o| o.iter().any(|x| Some(x.as_str()) == a["slug"].as_str()));
    let mut text = String::new();
    match corpus {
        "kind-esbuild" | "kind-webpack" | "kind-rollup" => {
            let kind = &corpus["kind-".len()..];
            let dev: Vec<&Value> = apps
                .iter()
                .filter(|a| {
                    a[PLAN_SPLIT].as_str() == Some("dev")
                        && a["id"]
                            .as_str()
                            .and_then(|id| bun_built.get(id))
                            .is_some_and(|r| r["node"].as_bool() == Some(true))
                })
                .filter(keep)
                .collect();
            let results = crate::runners::each(&dev, jobs, |a| -> Result<(String, bool), String> {
                let slug = a["slug"].as_str().unwrap_or("");
                let d = PathBuf::from(format!("{BUN_APPS}/apps/{slug}"));
                let o = root.join("build").join(kind).join(slug);
                let cl = bun_closure(slug)?;
                let ok = build_once(&o, || match kind {
                    "esbuild" => build_esbuild(&d, &o, &cl),
                    "webpack" => recipe_script(KIND_WEBPACK, &d, &o, &cl),
                    _ => recipe_script(KIND_ROLLUP, &d, &o, &cl),
                })?;
                Ok((
                    a["id"].as_str().unwrap_or("").to_string(),
                    ok && o.join("entry.js.map").exists(),
                ))
            });
            let mut merged = Vec::new();
            for (a, r) in dev.iter().zip(results) {
                let (id, ok) = r?;
                text.push_str(&format!(
                    "{kind} {id} {}\n",
                    if ok { "ok" } else { "FAILED" }
                ));
                merged.push((id, a["slug"].as_str().unwrap_or("").to_string(), ok));
            }
            merge_built(root, kind, &merged)?;
            let _ = KIND_DATA;
        }
        "bun-dev" | "bun-held" => {
            let split = if corpus == "bun-dev" { "dev" } else { "held" };
            let mine: Vec<&Value> = apps
                .iter()
                .filter(|a| a[PLAN_SPLIT].as_str() == Some(split))
                .filter(keep)
                .collect();
            let results =
                crate::runners::each(&mine, jobs, |a| -> Result<Vec<(String, bool)>, String> {
                    let slug = a["slug"].as_str().unwrap_or("");
                    let d = PathBuf::from(format!("{BUN_APPS}/apps/{slug}"));
                    ensure_installed(a, &d)?;
                    let mut out = Vec::new();
                    for t in ["node", "bun"] {
                        let o = root.join("build").join(format!("bun-{t}")).join(slug);
                        out.push((t.to_string(), build_once(&o, || build_bun(&d, &o, t))?));
                    }
                    Ok(out)
                });
            for (a, r) in mine.iter().zip(results) {
                let id = a["id"].as_str().unwrap_or("");
                match r {
                    Ok(ts) => {
                        for (t, ok) in ts {
                            text.push_str(&format!(
                                "bun-{t} {id} {}\n",
                                if ok { "ok" } else { "FAILED" }
                            ));
                        }
                    }
                    Err(e) => text.push_str(&format!("{id} FAILED: {e}\n")),
                }
            }
        }
        other => {
            return Err(format!(
                "corpus {other}: no build recipe (kind-esbuild, kind-webpack, kind-rollup, bun-dev, bun-held)"
            ));
        }
    }
    Ok(text)
}

/// Merge one kind's results into `<root>/built.json` (kind_corpus.py
/// writes every kind of an app in one row; a row keeps its other kinds).
fn merge_built(root: &Path, kind: &str, results: &[(String, String, bool)]) -> Result<(), String> {
    let p = root.join("built.json");
    let mut rows: Vec<Vec<(String, J)>> = Vec::new();
    if let Ok(text) = std::fs::read_to_string(&p) {
        let v: Value = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", p.display()))?;
        for r in v.as_array().into_iter().flatten() {
            let mut row = Vec::new();
            for (k, x) in r.as_object().into_iter().flatten() {
                let j = match x {
                    Value::Bool(b) => J::Bool(*b),
                    Value::String(s) => J::Str(s.clone()),
                    other => J::Str(other.to_string()),
                };
                row.push((k.clone(), j));
            }
            rows.push(row);
        }
    }
    for (id, slug, ok) in results {
        let at = rows.iter().position(|r| {
            r.iter()
                .any(|(k, v)| k == "id" && matches!(v, J::Str(x) if x == id))
        });
        let row = match at {
            Some(i) => &mut rows[i],
            None => {
                rows.push(vec![
                    ("id".into(), J::Str(id.clone())),
                    ("slug".into(), J::Str(slug.clone())),
                ]);
                rows.last_mut().expect("just pushed")
            }
        };
        row.retain(|(k, _)| k != kind && *k != format!("{kind}Log"));
        row.push((kind.to_string(), J::Bool(*ok)));
    }
    let v = J::List(rows.into_iter().map(J::Obj).collect());
    std::fs::write(&p, dumps(&v, Some(1))).map_err(|e| format!("{}: {e}", p.display()))
}

#[cfg(test)]
#[path = "corpus_build_test.rs"]
mod corpus_build_test;
