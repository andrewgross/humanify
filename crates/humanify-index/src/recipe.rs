//! How the builder makes what it fingerprints — the ONLY place that spells
//! a bundler's command line (docs/design/library-index.md §6). A slice is
//! a value (`IndexSlice`); everything downstream receives the value.
//!
//! Every rule here is part of [`RECIPE_VERSION`], which is in every shard
//! key: change a rule, bump the version, and the next build rebuilds.

use std::path::{Path, PathBuf};
use std::process::Command;

use humanify_core::libindex::IndexSlice;

/// The build recipe's version (in every shard key).
pub const RECIPE_VERSION: &str = "recipe/1";

/// Source files larger than this are not read (the lab's 4 MB cap).
pub const MAX_SOURCE_BYTES: u64 = 4_000_000;

/// The npm package a slice's bundler is published as (its packument dates
/// the bundler for the as-of check).
pub fn bundler_package(slice: IndexSlice) -> &'static str {
    match slice {
        IndexSlice::Bun => "bun",
        IndexSlice::Esbuild => "esbuild",
    }
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

/// A slice's bundler binary.
#[derive(Clone, Debug)]
pub struct Recipe {
    pub slice: IndexSlice,
    pub bin: PathBuf,
}

impl Recipe {
    /// The bundler's `--version` (trimmed).
    pub fn bundler_version(&self) -> Result<String, String> {
        let out = Command::new(&self.bin)
            .arg("--version")
            .output()
            .map_err(|e| format!("{}: {e}", self.bin.display()))?;
        let v = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !out.status.success() || v.is_empty() {
            return Err(format!("{} --version failed", self.bin.display()));
        }
        Ok(v)
    }

    /// Build `entry` (relative to `cwd`) minified with an external
    /// sourcemap into `out_dir/out.js` (+ `.map`). `esm` switches the
    /// output format (top-level await). Returns the bundler's error text
    /// on failure.
    pub fn build(
        &self,
        cwd: &Path,
        entry: &str,
        out_dir: &Path,
        externals: &[String],
        esm: bool,
    ) -> Result<(), String> {
        let format = if esm { "esm" } else { "cjs" };
        let _ = std::fs::remove_dir_all(out_dir);
        std::fs::create_dir_all(out_dir).map_err(|e| e.to_string())?;
        let mut cmd = Command::new(&self.bin);
        cmd.current_dir(cwd);
        match self.slice {
            IndexSlice::Bun => {
                cmd.args(["build", entry, "--target", "node", "--format", format])
                    .args(["--minify", "--sourcemap=external", "--outdir"])
                    // straight into out_dir: the map's source paths are
                    // relative to the directory Bun wrote it in
                    .arg(out_dir);
                for x in externals {
                    cmd.args(["--external", x]);
                }
            }
            IndexSlice::Esbuild => {
                cmd.args([entry, "--bundle", "--platform=node", "--log-level=error"])
                    .arg(format!("--format={format}"))
                    .args(["--minify", "--sourcemap=external"])
                    .arg(format!("--outfile={}", out_dir.join("out.js").display()));
                for x in externals {
                    cmd.arg(format!("--external:{x}"));
                }
            }
        }
        let out = cmd.output().map_err(|e| e.to_string())?;
        if !out.status.success() {
            return Err(format!(
                "{}{}",
                String::from_utf8_lossy(&out.stderr),
                String::from_utf8_lossy(&out.stdout)
            ));
        }
        if self.slice == IndexSlice::Bun {
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
        Ok(())
    }
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
