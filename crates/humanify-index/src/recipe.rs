//! How the builder makes what it fingerprints — the ONLY place that spells
//! a bundler's command line (docs/design/library-index.md §6). A slice is
//! a build recipe (`libindex::recipe::Recipe`: bundler, target, format,
//! minify), named by `--recipe`; everything downstream receives the value.
//!
//! Every rule here is part of [`RECIPE_VERSION`], which is in every shard
//! key: change a rule, bump the version, and the next build rebuilds. One
//! exception, by proof: no recipe writes a map's `sourcesContent` (exp131),
//! a byte the builder never reads, so the builds cached before it stay
//! current (index tables byte-identical, `experiments/131-cache-slim/`).

use std::path::{Path, PathBuf};
use std::process::Command;

use humanify_core::libindex::recipe::{Recipe, RecipeBundler, Target};

/// The build recipe's version (in every shard key and the manifest's
/// slice records; the pipeline's loader checks it).
pub use humanify_core::libindex::format::RECIPE_VERSION;

/// Source files larger than this are not read (the lab's 4 MB cap).
pub const MAX_SOURCE_BYTES: u64 = 4_000_000;

/// The npm package a slice's bundler is published as (its packument dates
/// the bundler for the as-of check).
pub fn bundler_package(bundler: RecipeBundler) -> &'static str {
    match bundler {
        RecipeBundler::Bun => "bun",
        RecipeBundler::Esbuild => "esbuild",
        RecipeBundler::Webpack => "webpack",
        RecipeBundler::Rollup => "rollup",
    }
}

/// A bundler run as a Node script (`crates/humanify-index/recipes/`): its
/// configuration is code (plugins, a minimizer), not a command line.
fn runs_as_node_script(bundler: RecipeBundler) -> bool {
    matches!(bundler, RecipeBundler::Webpack | RecipeBundler::Rollup)
}

/// A file the literal index reads: JavaScript or JSON, not a type
/// declaration.
pub fn is_source_file(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    let js = [".js", ".mjs", ".cjs", ".json"]
        .iter()
        .any(|ext| name.ends_with(ext));
    js && !name.contains(".d.")
}

/// A pre-bundled, minified, CDN or browser build ("sponge"): it holds
/// other packages' code, so its literals claim everything (exp095 scale
/// lane: the cause of most wrong literal claims at top-2,000). The lab's
/// rule, on the file NAME: `bundled`, `bundle.`, `.min.`, `.global.`,
/// `.umd.`, `browser` anywhere in it, or a name starting `cdn.`.
pub fn is_sponge_file(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.starts_with("cdn.")
        || [
            "bundled", "bundle.", ".min.", ".global.", ".umd.", "browser",
        ]
        .iter()
        .any(|t| name.contains(t))
}

/// What a `production` recipe defines (as app bundles do; webpack's recipe
/// script always does).
const NODE_ENV_PRODUCTION: &str = "process.env.NODE_ENV=\"production\"";

/// `<bundler> --version` answers in milliseconds.
const VERSION_DEADLINE: std::time::Duration = std::time::Duration::from_secs(60);
/// One minified build of one package entry (seconds; generous for the
/// largest packages).
const BUILD_DEADLINE: std::time::Duration = std::time::Duration::from_secs(30 * 60);

/// One recipe and the bundler binary that runs it.
#[derive(Clone, Debug)]
pub struct RecipeBuild {
    pub recipe: Recipe,
    pub bin: PathBuf,
}

impl RecipeBuild {
    /// The command that prints the bundler's version: the binary's
    /// `--version`, or the recipe script's (`node <script> --version`).
    pub fn version_command(&self) -> Command {
        let mut cmd = if runs_as_node_script(self.recipe.bundler) {
            let mut c = Command::new("node");
            c.arg(&self.bin);
            c
        } else {
            Command::new(&self.bin)
        };
        cmd.arg("--version");
        cmd
    }

    /// The bundler's `--version` (trimmed).
    pub fn bundler_version(&self) -> Result<String, String> {
        let out =
            humanify_core::subprocess::run(&mut self.version_command(), None, VERSION_DEADLINE)
                .map_err(|e| format!("{}: {e}", self.bin.display()))?;
        let v = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !out.status.success() || v.is_empty() {
            return Err(format!("{} --version failed", self.bin.display()));
        }
        Ok(v)
    }

    /// The command that builds `entry` (relative to `cwd`) by the recipe
    /// into `out_dir` (`out.js` + `.map`, except Bun, which names them
    /// after the entry: [`Self::build`] renames). `esm` forces the ESM
    /// format.
    pub fn command(
        &self,
        cwd: &Path,
        entry: &str,
        out_dir: &Path,
        externals: &[String],
        esm: bool,
    ) -> Command {
        let format = if esm {
            "esm"
        } else {
            self.recipe.format.name()
        };
        let target = self.recipe.target.name();
        let mut cmd = Command::new(if runs_as_node_script(self.recipe.bundler) {
            Path::new("node")
        } else {
            self.bin.as_path()
        });
        cmd.current_dir(cwd);
        match self.recipe.bundler {
            RecipeBundler::Bun => {
                cmd.args(["build", entry, "--target", target, "--format", format]);
                if self.recipe.minify {
                    cmd.arg("--minify");
                }
                cmd.args(["--sourcemap=external", "--outdir"])
                    // straight into out_dir: the map's source paths are
                    // relative to the directory Bun wrote it in
                    .arg(out_dir);
                if self.recipe.production {
                    cmd.args(["--define", NODE_ENV_PRODUCTION]);
                }
                for x in externals {
                    cmd.args(["--external", x]);
                }
            }
            RecipeBundler::Esbuild => {
                debug_assert_eq!(
                    self.recipe.target,
                    Target::Node,
                    "Recipe::parse refuses others"
                );
                cmd.args([entry, "--bundle", "--platform=node", "--log-level=error"])
                    .arg(format!("--format={format}"));
                if self.recipe.minify {
                    cmd.arg("--minify");
                }
                cmd.args(["--sourcemap=external", "--sources-content=false"])
                    .arg(format!("--outfile={}", out_dir.join("out.js").display()));
                if self.recipe.production {
                    cmd.arg(format!("--define:{NODE_ENV_PRODUCTION}"));
                }
                for x in externals {
                    cmd.arg(format!("--external:{x}"));
                }
            }
            // The scripts hold the whole recipe (target node, production
            // minifier, one chunk, the source map's relative paths): the
            // command line is the entry, the output, the format and the
            // externals.
            RecipeBundler::Webpack | RecipeBundler::Rollup => {
                cmd.arg(&self.bin)
                    .arg(entry)
                    .arg(out_dir)
                    .arg(format)
                    .args(externals);
            }
        }
        cmd
    }

    /// Build `entry` (relative to `cwd`) by the recipe, with an external
    /// sourcemap, into `out_dir/out.js` (+ `.map`). `esm` forces the ESM
    /// format (an entry with top-level await cannot be CommonJS). Returns
    /// the bundler's error text on failure.
    pub fn build(
        &self,
        cwd: &Path,
        entry: &str,
        out_dir: &Path,
        externals: &[String],
        esm: bool,
    ) -> Result<(), String> {
        let _ = std::fs::remove_dir_all(out_dir);
        std::fs::create_dir_all(out_dir).map_err(|e| e.to_string())?;
        let mut cmd = self.command(cwd, entry, out_dir, externals, esm);
        let out = humanify_core::subprocess::run(&mut cmd, None, BUILD_DEADLINE)
            .map_err(|e| e.to_string())?;
        if !out.status.success() {
            return Err(format!(
                "{}{}",
                String::from_utf8_lossy(&out.stderr),
                String::from_utf8_lossy(&out.stdout)
            ));
        }
        if self.recipe.bundler == RecipeBundler::Bun {
            let stem = Path::new(entry)
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            std::fs::rename(out_dir.join(format!("{stem}.js")), out_dir.join("out.js"))
                .map_err(|e| format!("bun output: {e}"))?;
            std::fs::rename(
                out_dir.join(format!("{stem}.js.map")),
                out_dir.join("out.js.map"),
            )
            .map_err(|e| format!("bun map: {e}"))?;
        }
        // Bun has no flag for it: whatever a bundler embedded, the map
        // keeps only what the builder reads (exp131)
        let map = out_dir.join("out.js.map");
        if map.exists() {
            crate::slim::slim_map_file(&map, None)?;
        }
        Ok(())
    }
}

/// The most subpath entries one package gets (a wildcard export can name
/// every file of a package).
pub const MAX_ENTRY_SPECIFIERS: usize = 256;

/// What the app entries require of package `name`: the package itself and
/// every subpath of its `exports` map a Node app can load (conditions
/// node / import / require / default), wildcards expanded over `files`
/// (the package's files, relative to its root).
pub fn entry_specifiers(name: &str, manifest: &serde_json::Value, files: &[String]) -> Vec<String> {
    let Some(exports) = manifest.get("exports") else {
        return vec![name.to_string()];
    };
    let dot = String::from(".");
    let subpaths: Vec<(&String, &serde_json::Value)> = match exports.as_object() {
        Some(map) if map.keys().any(|k| k.starts_with('.')) => {
            map.iter().filter(|(k, _)| k.starts_with('.')).collect()
        }
        // a string, an array or a conditions object: the "." export
        _ => vec![(&dot, exports)],
    };
    let shipped = |t: &str| files.iter().any(|f| f == t.trim_start_matches("./"));
    let mut out = Vec::new();
    for (key, target) in subpaths {
        let targets = node_targets(target);
        if key.ends_with(".json") || key.ends_with('/') || targets.is_empty() {
            continue;
        }
        if key.contains('*') {
            out.extend(
                expand_wildcard(key, targets[0], files)
                    .into_iter()
                    .map(|sub| format!("{name}{}", &sub[1..])),
            );
        } else if targets.iter().all(|t| shipped(t)) {
            // a file the package does not ship would be retried as an
            // external, and an external package name takes every subpath
            // with it
            out.push(if key == "." {
                name.to_string()
            } else {
                format!("{name}{}", &key[1..])
            });
        }
    }
    out.sort();
    out.dedup();
    out.truncate(MAX_ENTRY_SPECIFIERS);
    if out.is_empty() {
        // nothing a Node app can load: the bare name, which fails to
        // resolve and is recorded as the build's error
        out.push(name.to_string());
    }
    out
}

/// The files an export can resolve to for a Node app (conditions node /
/// require / import / default, nested objects and fallback arrays
/// followed, in that preference); empty if only other conditions
/// (`browser`, `types`, …) or `null` reach a file. Type declarations are
/// not files an app loads.
fn node_targets(v: &serde_json::Value) -> Vec<&str> {
    match v {
        serde_json::Value::String(s) if !s.contains(".d.") => vec![s.as_str()],
        serde_json::Value::Array(a) => a.iter().flat_map(node_targets).take(1).collect(),
        serde_json::Value::Object(m) => ["node", "require", "import", "default"]
            .iter()
            .filter_map(|c| m.get(*c))
            .flat_map(node_targets)
            .collect(),
        _ => Vec::new(),
    }
}

/// A wildcard export (`"./*": "./dist/cjs/*"`) → the subpaths its target
/// pattern matches among `files` (JavaScript only).
fn expand_wildcard(key: &str, target: &str, files: &[String]) -> Vec<String> {
    let target = target.trim_start_matches("./");
    let (Some((t_pre, t_suf)), Some((k_pre, k_suf))) =
        (target.split_once('*'), key.split_once('*'))
    else {
        return Vec::new();
    };
    files
        .iter()
        .filter(|f| {
            let n = f.rsplit('/').next().unwrap_or(f);
            [".js", ".cjs", ".mjs"].iter().any(|e| n.ends_with(e)) && !n.contains(".d.")
        })
        .filter_map(|f| {
            let rest = f.strip_prefix(t_pre)?.strip_suffix(t_suf)?;
            (!rest.is_empty()).then(|| format!("{k_pre}{rest}{k_suf}"))
        })
        .collect()
}

/// The specifiers a failed build could not resolve (retried as external).
pub fn unresolved_specifiers(err: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for marker in [
        "Could not resolve: \"",
        "Could not resolve \"",
        "Could not resolve: '",
    ] {
        let quote = if marker.ends_with('\'') { '\'' } else { '"' };
        for part in err.split(marker).skip(1) {
            if let Some(spec) = part.split(quote).next()
                && !spec.is_empty()
                && !out.iter().any(|s| s == spec)
            {
                out.push(spec.to_string());
            }
        }
    }
    out
}
