//! `library npm <corpus>`: fill the npm caches the truth layers read, for
//! one corpus — the port of experiments/112 `origins7.py needs|owners`,
//! experiments/104 `fetch_metadata.py` (the registry manifests:
//! `<cache>/<name>@<version|latest>.json`, the read fields only,
//! `{"__error": 404}` for a name npm does not have) and experiments/112
//! `bundled.py` (the packages a published tarball's third-party license
//! collection names: `<bundled>/<name>@<version>.json`).
//!
//! Polite and resumable: a file already in the cache is never fetched
//! again; at most 8 registry requests (curl, its own retry on a transient
//! error) and `jobs` `npm pack`s run at once.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::corpus::Corpus;
use super::origins::{marker_regions7, path_version};
use crate::json::py::PyValue;
use crate::proc::{self, Spawn};

/// The caches every layer builder reads (this host's).
pub const REGISTRY: &str = "/work/exp112-data/registry";
pub const BUNDLED: &str = "/work/exp112-data/bundled";
const KEEP: [&str; 11] = [
    "name",
    "version",
    "repository",
    "deprecated",
    "dependencies",
    "devDependencies",
    "peerDependencies",
    "optionalDependencies",
    "bundleDependencies",
    "bundledDependencies",
    "homepage",
];

type Pv = (String, Option<String>);

/// (every name, every [name, version]): fetch_metadata.py's universe.
pub type Universe = (Vec<String>, Vec<(String, String)>);

fn sources_of(corpus: &Corpus) -> Result<Vec<super::layers::Loaded>, String> {
    corpus
        .entries()
        .iter()
        .filter(|e| e.truth.method == "sourcemap")
        .map(|e| super::layers::load(corpus, e))
        .collect()
}

/// origins7.py `needs`: (names, [name, version]) of every package the
/// truths' sources and marker regions name.
pub fn needs(corpus: &Corpus) -> Result<Universe, String> {
    let mut got: BTreeSet<Pv> = BTreeSet::new();
    for l in sources_of(corpus)? {
        for (si, p) in l.paths.iter().enumerate() {
            let c = l.contents.get(si).cloned().flatten();
            let (pkg, ver) = (&l.shippers[si], &l.versions[si]);
            let regs = c
                .as_deref()
                .filter(|c| !c.is_empty())
                .map_or_else(Vec::new, |c| marker_regions7(c, true, false));
            let other = |o: &Option<String>| o.as_ref().is_some_and(|o| o != pkg);
            if regs.iter().any(|r| other(&r.1)) {
                for r in regs.iter().filter(|r| other(&r.1)) {
                    let o = r.1.clone().unwrap_or_default();
                    let v = path_version(r.2.as_deref().unwrap_or(""), &o);
                    got.insert((o, v));
                }
            } else if p.as_deref().unwrap_or("").contains("node_modules/")
                && !pkg.is_empty()
                && !pkg.starts_with('<')
            {
                got.insert((pkg.clone(), ver.clone()));
            }
        }
    }
    let names: BTreeSet<String> = got.iter().map(|x| x.0.clone()).collect();
    let pv: BTreeSet<(String, String)> = got
        .into_iter()
        .filter_map(|(p, v)| v.map(|v| (p, v)))
        .collect();
    Ok((names.into_iter().collect(), pv.into_iter().collect()))
}

/// origins7.py `HASHED`: `^(.+?)-[A-Za-z0-9_]{8}(?:-[A-Za-z0-9_-]{8})?$`.
fn hashed(stem: &str) -> bool {
    let c: Vec<char> = stem.chars().collect();
    let w = |x: char| x.is_ascii_alphanumeric() || x == '_';
    let n = c.len();
    let form1 = n >= 10 && c[n - 9] == '-' && c[n - 8..].iter().all(|x| w(*x));
    let form2 = n >= 19
        && c[n - 18] == '-'
        && c[n - 17..n - 9].iter().all(|x| w(*x))
        && c[n - 9] == '-'
        && c[n - 8..].iter().all(|x| w(*x) || *x == '-');
    form1 || form2
}

/// origins7.py `CHUNK_DIR`: `(?:^|/)_?chunks/|(?:^|/)libs/`.
fn chunk_dir(rest: &str) -> bool {
    rest.split('/')
        .rev()
        .skip(1)
        .any(|d| d == "chunks" || d == "_chunks" || d == "libs")
}

/// origins7.py `owners`: (owner, version) of every file the chunk rule
/// could name after another package.
pub fn owners(corpus: &Corpus) -> Result<Vec<Pv>, String> {
    let mut got: BTreeSet<Pv> = BTreeSet::new();
    for l in sources_of(corpus)? {
        for (si, p) in l.paths.iter().enumerate() {
            let p = p.as_deref().unwrap_or("");
            let c = l.contents.get(si).cloned().flatten();
            let (pkg, ver) = (&l.shippers[si], &l.versions[si]);
            let regs = c
                .as_deref()
                .filter(|c| !c.is_empty())
                .map_or_else(Vec::new, |c| marker_regions7(c, true, false));
            let other = |o: &Option<String>| o.as_ref().is_some_and(|o| o != pkg);
            let mut files: Vec<(String, Option<String>, String)> = Vec::new();
            if regs.iter().any(|r| other(&r.1)) {
                for r in regs.iter().filter(|r| other(&r.1)) {
                    let o = r.1.clone().unwrap_or_default();
                    let mp = r.2.clone().unwrap_or_default();
                    let marker = format!("node_modules/{o}/");
                    if let Some(k) = mp.rfind(&marker) {
                        files.push((
                            o.clone(),
                            path_version(&mp, &o),
                            mp[k + marker.len()..].to_string(),
                        ));
                    }
                }
            } else if !pkg.is_empty() {
                let marker = format!("node_modules/{pkg}/");
                if let Some(k) = p.rfind(&marker) {
                    files.push((pkg.clone(), ver.clone(), p[k + marker.len()..].to_string()));
                }
            }
            for (o, v, rest) in files {
                let stem = super::pyre::strip_src_ext(rest.rsplit('/').next().unwrap_or(&rest));
                if hashed(stem) || chunk_dir(&rest) {
                    got.insert((o, v));
                }
            }
        }
    }
    let mut out: Vec<Pv> = got.into_iter().collect();
    out.sort_by(|a, b| {
        (a.0.as_str(), a.1.as_deref().unwrap_or(""))
            .cmp(&(b.0.as_str(), b.1.as_deref().unwrap_or("")))
    });
    Ok(out)
}

// ---------------------------------------------------------------- the registry

/// Python's `urllib.parse.quote(s, safe)`.
fn quote(s: &str, safe: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        let c = b as char;
        if c.is_ascii_alphanumeric() || "_.-~".contains(c) || safe.contains(c) {
            out.push(c);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// fetch_metadata.py `path`.
pub fn metadata_path(cache: &Path, n: &str, v: &str) -> PathBuf {
    cache.join(format!(
        "{}@{}.json",
        n.replace('/', "__"),
        v.replace('/', "_")
    ))
}

/// `json.dumps(v, sort_keys=True)` (default separators, ensure_ascii).
fn dumps_sorted(out: &mut String, v: &PyValue) {
    match v {
        PyValue::List(xs) => {
            out.push('[');
            for (i, x) in xs.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                dumps_sorted(out, x);
            }
            out.push(']');
        }
        PyValue::Dict(m) => {
            let mut m: Vec<&(String, PyValue)> = m.iter().collect();
            m.sort_by(|a, b| a.0.cmp(&b.0));
            out.push('{');
            for (i, (k, x)) in m.into_iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                crate::json::py::write_str(out, k);
                out.push_str(": ");
                dumps_sorted(out, x);
            }
            out.push('}');
        }
        other => out.push_str(crate::json::py::dumps(other).as_str()),
    }
}

/// fetch_metadata.py `get`: false when the registry could not answer.
fn fetch_one(cache: &Path, n: &str, v: &str) -> bool {
    let p = metadata_path(cache, n, v);
    if p.exists() {
        return true;
    }
    let url = format!(
        "https://registry.npmjs.org/{}/{}",
        quote(n, "@"),
        quote(v, "/")
    );
    let tmp = PathBuf::from(format!("{}.part", p.display()));
    let args = [
        "-sS",
        "--retry",
        "3",
        "--max-time",
        "30",
        "-o",
        &tmp.to_string_lossy(),
        "-w",
        "%{http_code}",
        &url,
    ]
    .map(String::from);
    let argv: Vec<&str> = args.iter().map(String::as_str).collect();
    let Ok(done) = proc::run(&Spawn::new("curl", &argv, Duration::from_secs(150))) else {
        return false;
    };
    let code = String::from_utf8_lossy(&done.stdout).trim().to_string();
    let body = match code.as_str() {
        "404" | "405" => PyValue::Dict(vec![("__error".into(), PyValue::Int(code.clone()))]),
        "200" => {
            let text = std::fs::read_to_string(&tmp).unwrap_or_default();
            let Ok(PyValue::Dict(m)) = PyValue::parse(&text) else {
                let _ = std::fs::remove_file(&tmp);
                return false;
            };
            let mut keep: BTreeMap<String, PyValue> = BTreeMap::new();
            for (k, x) in m {
                if KEEP.contains(&k.as_str()) || k == "__error" {
                    keep.insert(k, x);
                }
            }
            PyValue::Dict(keep.into_iter().collect())
        }
        _ => {
            let _ = std::fs::remove_file(&tmp);
            return false;
        }
    };
    let _ = std::fs::remove_file(&tmp);
    let mut out = String::new();
    dumps_sorted(&mut out, &body);
    std::fs::write(&p, out).is_ok()
}

/// fetch_metadata.py: every name's `latest` and every (name, version);
/// (requests, failures).
pub fn fetch_metadata(
    cache: &Path,
    names: &[String],
    pv: &[(String, String)],
) -> Result<(usize, usize), String> {
    std::fs::create_dir_all(cache).map_err(|e| format!("{}: {e}", cache.display()))?;
    let jobs: Vec<(String, String)> = names
        .iter()
        .map(|n| (n.clone(), "latest".to_string()))
        .chain(pv.iter().cloned())
        .collect();
    let ok = crate::runners::each(&jobs, 8, |(n, v)| fetch_one(cache, n, v));
    Ok((jobs.len(), ok.iter().filter(|x| !**x).count()))
}

// ---------------------------------------------------------------- bundled

/// bundled.py `path`.
pub fn bundled_path(dir: &Path, pkg: &str, ver: Option<&str>) -> PathBuf {
    dir.join(format!(
        "{}@{}.json",
        pkg.replace('/', "__"),
        ver.unwrap_or("latest")
    ))
}

/// One tar member: (name, regular file, size, offset of its data).
fn tar_members(tar: &[u8]) -> Vec<(String, bool, usize, usize)> {
    let mut out = Vec::new();
    let mut k = 0;
    let mut long_name: Option<String> = None;
    let field =
        |b: &[u8]| String::from_utf8_lossy(b.split(|&x| x == 0).next().unwrap_or(&[])).to_string();
    while k + 512 <= tar.len() {
        let h = &tar[k..k + 512];
        if h.iter().all(|&x| x == 0) {
            break;
        }
        let size = usize::from_str_radix(field(&h[124..136]).trim(), 8).unwrap_or(0);
        let kind = h[156];
        let data = k + 512;
        let mut name = field(&h[0..100]);
        if &h[257..262] == b"ustar" {
            let prefix = field(&h[345..500]);
            if !prefix.is_empty() {
                name = format!("{prefix}/{name}");
            }
        }
        let body = &tar[data..(data + size).min(tar.len())];
        match kind {
            b'x' => {
                for rec in String::from_utf8_lossy(body).split('\n') {
                    if let Some((_, kv)) = rec.split_once(' ')
                        && let Some(p) = kv.strip_prefix("path=")
                    {
                        long_name = Some(p.to_string());
                    }
                }
            }
            b'L' => long_name = Some(field(body)),
            b'g' => {}
            _ => {
                let name = long_name.take().unwrap_or(name);
                out.push((name, kind == b'0' || kind == 0, size, data));
            }
        }
        k = data + size.div_ceil(512) * 512;
    }
    out
}

/// `third[_-]party[_-]license` + `tail` (lowercased).
fn third_party(s: &str, tail: &str) -> bool {
    let b = s.as_bytes();
    s.is_ascii()
        && s.len() == 19 + tail.len()
        && s.starts_with("third")
        && (b[5] == b'_' || b[5] == b'-')
        && &s[6..11] == "party"
        && (b[11] == b'_' || b[11] == b'-')
        && &s[12..19] == "license"
        && &s[19..] == tail
}

/// bundled.py `LIC_DIR` (case-insensitive):
/// `^package/(?:.*/)?(?:licenses|third[_-]party[_-]licenses|LICENSES)/((?:@[^/]+/)?[^/]+)/[^/]+$`.
fn lic_dir(name: &str) -> Option<String> {
    let c: Vec<&str> = name.split('/').collect();
    if c.len() < 4 || !c[0].eq_ignore_ascii_case("package") {
        return None;
    }
    let is_dir = |d: &str| {
        let d = d.to_ascii_lowercase();
        d == "licenses" || third_party(&d, "s")
    };
    // the greedy `(?:.*/)?`: the LAST license directory that fits
    for i in (1..c.len()).rev() {
        let rest = &c[i + 1..];
        if !is_dir(c[i]) || rest.iter().any(|x| x.is_empty()) {
            continue;
        }
        match rest.len() {
            3 if rest[0].starts_with('@') => return Some(format!("{}/{}", rest[0], rest[1])),
            2 => return Some(rest[0].to_string()),
            _ => {}
        }
    }
    None
}

/// bundled.py `LIC_MD` (case-insensitive):
/// `^package/(?:LICENSE|THIRD[_-]PARTY[_-]LICENSES?|LICENSES)(?:\.md)?$`.
fn lic_md(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    let Some(base) = n.strip_prefix("package/") else {
        return false;
    };
    let ok =
        |b: &str| b == "license" || b == "licenses" || third_party(b, "") || third_party(b, "s");
    ok(base) || base.strip_suffix(".md").is_some_and(ok)
}

/// bundled.py `HEADING` (multiline): `^#{2,3} ((?:@[\w.-]+/)?[\w.-]+)\s*$`.
fn headings(text: &str) -> Vec<String> {
    let w = |c: char| super::pyre::is_word(c) || c == '.' || c == '-';
    let mut out = Vec::new();
    for line in text.split('\n') {
        let rest = line
            .strip_prefix("### ")
            .or_else(|| line.strip_prefix("## "));
        let Some(rest) = rest else { continue };
        let end = rest.trim_end_matches(super::pyre::is_space);
        let name_ok = |s: &str| !s.is_empty() && s.chars().all(w);
        let ok = match end.strip_prefix('@').and_then(|r| r.split_once('/')) {
            Some((scope, pkg)) => name_ok(scope) && name_ok(pkg),
            None => name_ok(end),
        };
        if ok {
            out.push(end.to_string());
        }
    }
    out
}

/// bundled.py `one`: false when the cache already had it.
fn bundled_one(dir: &Path, pkg: &str, ver: Option<&str>) -> Result<(), String> {
    let p = bundled_path(dir, pkg, ver);
    if p.exists() {
        return Ok(());
    }
    let td = dir
        .join(format!(".pack-{}", std::process::id()))
        .join(pkg.replace('/', "__") + "@" + ver.unwrap_or("latest"));
    std::fs::create_dir_all(&td).map_err(|e| e.to_string())?;
    let spec = format!("{pkg}@{}", ver.unwrap_or("latest"));
    let tds = td.to_string_lossy().to_string();
    let r = proc::run(&Spawn::new(
        "npm",
        &[
            "pack",
            &spec,
            "--pack-destination",
            &tds,
            "--prefer-offline",
            "--silent",
            "--ignore-scripts",
        ],
        Duration::from_secs(300),
    ));
    let tgz = std::fs::read_dir(&td).ok().and_then(|d| {
        d.flatten()
            .map(|e| e.path())
            .find(|x| x.extension().is_some_and(|e| e == "tgz"))
    });
    let write = |v: &str| std::fs::write(&p, v).map_err(|e| format!("{}: {e}", p.display()));
    let result = match (r, tgz) {
        (Ok(done), Some(tgz)) if done.status.success() => {
            let mut tar = Vec::new();
            let bytes = std::fs::read(&tgz).map_err(|e| e.to_string())?;
            flate2::read::GzDecoder::new(&bytes[..])
                .read_to_end(&mut tar)
                .map_err(|e| format!("{}: {e}", tgz.display()))?;
            let mut out: BTreeSet<String> = BTreeSet::new();
            for (name, file, size, at) in tar_members(&tar) {
                if let Some(d) = lic_dir(&name) {
                    out.insert(d);
                } else if file && lic_md(&name) && size < 5_000_000 {
                    out.extend(headings(&String::from_utf8_lossy(&tar[at..at + size])));
                }
            }
            let v = PyValue::Dict(vec![(
                "bundled".into(),
                PyValue::List(out.into_iter().map(PyValue::Str).collect()),
            )]);
            let mut s = String::new();
            dumps_sorted(&mut s, &v);
            write(&s)
        }
        (r, _) => {
            let err = match r {
                Ok(done) => {
                    let e = String::from_utf8_lossy(&done.stderr).to_string();
                    if e.is_empty() {
                        "npm pack failed".into()
                    } else {
                        e
                    }
                }
                Err(e) => e,
            };
            let tail: String = err
                .chars()
                .rev()
                .take(200)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            let v = PyValue::Dict(vec![("error".into(), PyValue::Str(tail))]);
            let mut s = String::new();
            dumps_sorted(&mut s, &v);
            write(&s)
        }
    };
    let _ = std::fs::remove_dir_all(&td);
    result
}

/// bundled.py over `owners`, `jobs` at a time.
pub fn fetch_bundled(dir: &Path, owners: &[Pv], jobs: usize) -> Result<usize, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let res = crate::runners::each(owners, jobs, |(p, v)| bundled_one(dir, p, v.as_deref()));
    let _ = std::fs::remove_dir_all(dir.join(format!(".pack-{}", std::process::id())));
    Ok(res.iter().filter(|r| r.is_err()).count())
}

/// `library npm <corpus>`: needs -> registry, owners -> bundled; the report.
pub fn fill(
    corpus: &Corpus,
    registry: &Path,
    bundled: &Path,
    jobs: usize,
    only: Option<&[String]>,
    record: Option<&Path>,
) -> Result<(String, usize), String> {
    let keep = |n: &str| only.is_none_or(|o| o.iter().any(|x| x == n));
    let (names, pv) = needs(corpus)?;
    let all_owners = owners(corpus)?;
    if let Some(dir) = record {
        // origins7.py's `needs` / `owners` files, for the record
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let s = |x: &str| PyValue::Str(x.to_string());
        let universe = PyValue::Dict(vec![
            (
                "names".into(),
                PyValue::List(names.iter().map(|n| s(n)).collect()),
            ),
            (
                "pv".into(),
                PyValue::List(
                    pv.iter()
                        .map(|(p, v)| PyValue::List(vec![s(p), s(v)]))
                        .collect(),
                ),
            ),
        ]);
        let ow = PyValue::List(
            all_owners
                .iter()
                .map(|(p, v)| PyValue::List(vec![s(p), v.as_deref().map_or(PyValue::Null, s)]))
                .collect(),
        );
        for (f, v) in [("needs.json", universe), ("owners.json", ow)] {
            let mut text = String::new();
            dumps_sorted(&mut text, &v);
            std::fs::write(dir.join(f), text).map_err(|e| e.to_string())?;
        }
    }
    let names: Vec<String> = names.into_iter().filter(|n| keep(n)).collect();
    let pv: Vec<(String, String)> = pv.into_iter().filter(|x| keep(&x.0)).collect();
    let (n, fails) = fetch_metadata(registry, &names, &pv)?;
    let owners: Vec<Pv> = all_owners.into_iter().filter(|x| keep(&x.0)).collect();
    let bfails = fetch_bundled(bundled, &owners, jobs)?;
    let text = format!(
        "registry: {n} manifests ({} names, {} versions), {fails} failed -> {}\nbundled: {} owners, {bfails} failed -> {}\n",
        names.len(),
        pv.len(),
        registry.display(),
        owners.len(),
        bundled.display()
    );
    Ok((text, fails + bfails))
}

#[cfg(test)]
#[path = "npm_test.rs"]
mod npm_test;
