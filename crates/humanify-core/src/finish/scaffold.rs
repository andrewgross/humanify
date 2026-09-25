//! Runnable scaffolding for a runnable split tree — TS
//! `src/split/runnable-scaffold.ts`.
//!
//! `run.cjs` boots the entry (running `using` faithfully: natively under
//! Bun or Node >= 24, else a re-exec under V8's flag, else a loud failure
//! unless `HUMANIFY_STRIP_USING=1`); `package.json` lists the external
//! packages the tree requires (pinned to the copies installed beside the
//! input bundle, else `"*"`); `RUNNABLE.md` says how to run it. The bytes
//! are the TS's templates: `run.cjs` is the TS output verbatim with the
//! entry spliced in (`scaffold/run.cjs.template`).

use std::fs;
use std::path::{Path, PathBuf};

use humanify_model::js::{
    JsObject, JsValue, cmp_utf16, is_js_whitespace, node_path_resolve, stringify, stringify_pretty,
};

use crate::place::layout::METADATA_DIR;

pub const RUNNER_FILENAME: &str = "run.cjs";
pub const SCAFFOLD_README: &str = "RUNNABLE.md";

/// Node v24.18.0's `require("node:module").builtinModules` (the TS reads
/// it at runtime; the oracle ran on this Node — RUNBOOK §1 pins it).
const BUILTIN_MODULES: [&str; 72] = [
    "_http_agent",
    "_http_client",
    "_http_common",
    "_http_incoming",
    "_http_outgoing",
    "_http_server",
    "_stream_duplex",
    "_stream_passthrough",
    "_stream_readable",
    "_stream_transform",
    "_stream_wrap",
    "_stream_writable",
    "_tls_common",
    "_tls_wrap",
    "assert",
    "assert/strict",
    "async_hooks",
    "buffer",
    "child_process",
    "cluster",
    "console",
    "constants",
    "crypto",
    "dgram",
    "diagnostics_channel",
    "dns",
    "dns/promises",
    "domain",
    "events",
    "fs",
    "fs/promises",
    "http",
    "http2",
    "https",
    "inspector",
    "inspector/promises",
    "module",
    "net",
    "os",
    "path",
    "path/posix",
    "path/win32",
    "perf_hooks",
    "process",
    "punycode",
    "querystring",
    "readline",
    "readline/promises",
    "repl",
    "stream",
    "stream/consumers",
    "stream/promises",
    "stream/web",
    "string_decoder",
    "sys",
    "timers",
    "timers/promises",
    "tls",
    "trace_events",
    "tty",
    "url",
    "util",
    "util/types",
    "v8",
    "vm",
    "wasi",
    "worker_threads",
    "zlib",
    "node:sea",
    "node:sqlite",
    "node:test",
    "node:test/reporters",
];

fn is_builtin(spec: &str) -> bool {
    let bare = spec.strip_prefix("node:").unwrap_or(spec);
    let head = bare.split('/').next().unwrap_or(bare);
    BUILTIN_MODULES.contains(&bare) || BUILTIN_MODULES.contains(&head)
}

/// The installable package name: scope + name, deep subpath dropped.
fn package_name(spec: &str) -> String {
    let parts: Vec<&str> = spec.split('/').collect();
    if spec.starts_with('@') {
        parts[..parts.len().min(2)].join("/")
    } else {
        parts[0].to_string()
    }
}

/// One match of `/(?:require|import)\s*\(\s*["']([^"']+)["']\s*\)/` at
/// byte `at`: the captured specifier and the match end.
fn specifier_at(text: &str, at: usize) -> Option<(&str, usize)> {
    let rest = &text[at..];
    let after_kw = if rest.starts_with("require") {
        at + 7
    } else if rest.starts_with("import") {
        at + 6
    } else {
        return None;
    };
    let skip_ws = |mut i: usize| {
        while let Some(c) = text[i..].chars().next() {
            if !is_js_whitespace(c) {
                break;
            }
            i += c.len_utf8();
        }
        i
    };
    let mut i = skip_ws(after_kw);
    if !text[i..].starts_with('(') {
        return None;
    }
    i = skip_ws(i + 1);
    if !text[i..].starts_with(['"', '\'']) {
        return None;
    }
    let spec_start = i + 1;
    let spec_len = text[spec_start..].find(['"', '\''])?;
    if spec_len == 0 {
        return None;
    }
    let spec_end = spec_start + spec_len;
    i = skip_ws(spec_end + 1);
    if !text[i..].starts_with(')') {
        return None;
    }
    Some((&text[spec_start..spec_end], i + 1))
}

/// Every specifier the global regex captures in `text`, in order.
fn specifiers(text: &str) -> Vec<&str> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        if (bytes[at] == b'r' || bytes[at] == b'i')
            && let Some((spec, end)) = specifier_at(text, at)
        {
            out.push(spec);
            at = end;
            continue;
        }
        at += 1;
    }
    out
}

/// `externalPackagesFrom(contents)`: the bare, non-builtin, non-scheme
/// specifiers' package names, sorted and de-duplicated.
pub fn external_packages_from<'a>(contents: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for text in contents {
        for spec in specifiers(text) {
            if spec.starts_with('.') || spec.starts_with('/') {
                continue;
            }
            if spec.contains(':') {
                continue;
            }
            if is_builtin(spec) {
                continue;
            }
            found.push(package_name(spec));
        }
    }
    found.sort_by(|a, b| cmp_utf16(a, b));
    found.dedup();
    found
}

/// `jsFilesUnder(dir)`: every `.js` / `.cjs` file, skipping
/// `node_modules` and the metadata dir (readdir order; every consumer is
/// per-file or order-free).
pub fn js_files_under(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    walk(dir, &mut out)?;
    Ok(out)
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = fs::read_dir(dir).map_err(|e| format!("readdir {}: {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("readdir {}: {e}", dir.display()))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let full = entry.path();
        let ty = entry
            .file_type()
            .map_err(|e| format!("stat {}: {e}", full.display()))?;
        if ty.is_dir() {
            if name == "node_modules" || name == METADATA_DIR {
                continue;
            }
            walk(&full, out)?;
        } else if name.ends_with(".js") || name.ends_with(".cjs") {
            out.push(full);
        }
    }
    Ok(())
}

/// `readFile(…, "utf-8")`: invalid sequences become U+FFFD.
pub(crate) fn read_utf8(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// `detectExternalPackages(dir)`.
pub fn detect_external_packages(dir: &Path) -> Result<Vec<String>, String> {
    let files = js_files_under(dir)?;
    let contents: Vec<String> = files
        .iter()
        .map(|f| read_utf8(f))
        .collect::<Result<_, _>>()?;
    Ok(external_packages_from(contents.iter().map(String::as_str)))
}

/// The `version` of `pkg` installed nearest `from_dir` (Node's walk up the
/// `node_modules` directories), or None.
fn installed_version(pkg: &str, from_dir: &Path) -> Option<String> {
    let mut dir = node_path_resolve(from_dir);
    loop {
        let manifest = dir.join("node_modules").join(pkg).join("package.json");
        if let Ok(text) = fs::read_to_string(&manifest)
            && let Ok(JsValue::Object(obj)) = JsValue::parse(&text)
            && let Some(JsValue::String(v)) = obj.get("version")
        {
            return Some(v.clone());
        }
        match dir.parent() {
            Some(p) if p != dir => dir = p.to_path_buf(),
            _ => return None,
        }
    }
}

/// `resolveExternalVersions(externals, fromDir)`: package → range, in the
/// externals' order.
pub fn resolve_external_versions(
    externals: &[String],
    from_dir: Option<&Path>,
) -> Vec<(String, String)> {
    externals
        .iter()
        .map(|name| {
            let version = from_dir.and_then(|d| installed_version(name, d));
            (name.clone(), version.unwrap_or_else(|| "*".into()))
        })
        .collect()
}

/// `runnerSource(entryFile)`.
pub fn runner_source(entry_file: &str) -> String {
    include_str!("scaffold/run.cjs.template").replace(
        "{ENTRY_JSON}",
        &stringify(&JsValue::String(entry_file.to_string())),
    )
}

/// `packageJsonSource(dependencies)`: `JSON.stringify(…, null, 2)` + "\n".
pub fn package_json_source(dependencies: &[(String, String)]) -> String {
    let mut deps = JsObject::new();
    for (name, range) in dependencies {
        deps.insert(name.clone(), JsValue::String(range.clone()));
    }
    let mut scripts = JsObject::new();
    scripts.insert("start", JsValue::String(format!("node {RUNNER_FILENAME}")));
    let mut root = JsObject::new();
    root.insert("name", JsValue::String("humanified-runnable".into()));
    root.insert("private", JsValue::Bool(true));
    root.insert(
        "description",
        JsValue::String("Unpacked, humanified, split bundle — runnable under Node.".into()),
    );
    root.insert("scripts", JsValue::Object(scripts));
    root.insert("dependencies", JsValue::Object(deps));
    format!("{}\n", stringify_pretty(&JsValue::Object(root), 2))
}

/// `readmeSource(entryFile, dependencies)`.
pub fn readme_source(entry_file: &str, dependencies: &[(String, String)]) -> String {
    let names: Vec<&str> = dependencies.iter().map(|(n, _)| n.as_str()).collect();
    let pinned = dependencies.iter().filter(|(_, v)| v != "*").count();
    let version_note = if pinned == names.len() {
        "Versions are pinned to the copies installed beside the input bundle.\n".to_string()
    } else if pinned > 0 {
        format!(
            "{pinned} of {} version(s) are pinned to the copies installed beside the input bundle; the rest use \"*\" — pin them if a package's internal layout must match the original bundle.\n",
            names.len()
        )
    } else {
        "Versions are best-effort (\"*\"); pin them if a package's internal layout must match the original bundle.\n".to_string()
    };
    let deps = if names.is_empty() {
        "This tree needs no external packages.\n\n".to_string()
    } else {
        format!(
            "This tree requires {} external package(s): {}.\n{version_note}\n```sh\nnpm install\n```\n\n",
            names.len(),
            names.join(", ")
        )
    };
    let r = RUNNER_FILENAME;
    format!(
        "# Running this tree

{deps}Then boot it:

```sh
node {r} --version
node {r} --help
```

`{r}` loads `{entry_file}`, which requires every module in
the tree (split runtime files + re-linked Bun factory modules). It runs
`using`/`await using` faithfully: natively under Bun or Node >= 24,
otherwise it re-execs once under `--js-explicit-resource-management` so
disposal still fires. On Node too old for that flag it stops with an error;
set `HUMANIFY_STRIP_USING=1` to strip `using` instead (disposal is then
lost).

If the original bundle targets the Bun runtime (calls `Bun.*` APIs), run it
under Bun — Node has no `Bun` global, so a real workload will stop at the
first such call regardless of syntax support:

```sh
bun {r} --version
```
"
    )
}

/// `writeRunnableScaffold(outputDir, entryFile, externals, resolveFromDir)`.
pub fn write_runnable_scaffold(
    output_dir: &Path,
    entry_file: &str,
    externals: &[String],
    resolve_from_dir: Option<&Path>,
) -> Result<(), String> {
    let dependencies = resolve_external_versions(externals, resolve_from_dir);
    let write = |name: &str, text: String| {
        let p = output_dir.join(name);
        fs::write(&p, text).map_err(|e| format!("write {}: {e}", p.display()))
    };
    write(RUNNER_FILENAME, runner_source(entry_file))?;
    write("package.json", package_json_source(&dependencies))?;
    write(SCAFFOLD_README, readme_source(entry_file, &dependencies))
}

#[cfg(test)]
mod scaffold_test;
