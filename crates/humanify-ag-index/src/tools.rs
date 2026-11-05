//! The bundler tools a recipe runs, and where they live (exp181, Andrew
//! 2026-11-01: "all intermediates in the store dir, easy to find and
//! reuse"): the ONE owner of which tool builds a recipe, at which pinned
//! version, installed where, and how it is fetched.
//!
//! Every tool lives in the library-index store, beside `registry/`,
//! `store/` and the merged indexes: `<root>/tools/<bundler>-<version>/`,
//! an npm install (`--ignore-scripts`, the builder's rule for everything it
//! installs) of the bundler at its pin ([`pin`]):
//!
//! - Bun and esbuild run as `node_modules/.bin/<bundler>`. Their packages
//!   ship a placeholder that their own postinstall replaces with the one
//!   platform binary npm installed; with scripts skipped, the fetch does
//!   that step itself ([`fetch_tool`]).
//! - webpack and rollup run as the builder's recipe script
//!   (`crates/humanify-ag-index/recipes/`, embedded in this binary), written
//!   into the tool directory beside the install: the script resolves the
//!   bundler and its plugins from its own directory. A script the store
//!   holds that differs from this binary's is rewritten on lookup — the
//!   script IS the recipe.
//!
//! The run-time build (`--library-index-from-bundle`) fetches the tool the
//! bundle's recipe needs, once per store; the offline builder is handed
//! its tools on the command line (`--bun`, `--esbuild`, `--webpack`,
//! `--rollup`) and may point them here.

use std::path::{Path, PathBuf};
use std::process::Command;

use humanify_ag_core::libindex::format::compare_versions;
use humanify_ag_core::libindex::recipe::{Recipe, RecipeBundler};

use crate::cache::TOOLS_DIR;
use crate::recipe::{RecipeBuild, bundler_version_of};

/// Every recipe bundler, in [`RecipeBundler`]'s order.
pub const ALL_RECIPE_BUNDLERS: [RecipeBundler; 4] = [
    RecipeBundler::Bun,
    RecipeBundler::Esbuild,
    RecipeBundler::Webpack,
    RecipeBundler::Rollup,
];

/// One bundler's pinned tool: the version a run-time build fetches, and
/// what the fetch costs on disk (measured on linux-x64, exp181: 81, 12,
/// 26 and 13 MB installed).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ToolPin {
    pub bundler: RecipeBundler,
    pub version: &'static str,
    /// The npm packages the recipe script loads beside the bundler, each
    /// pinned (a plugin's version changes the build as much as the
    /// bundler's).
    pub plugins: &'static [&'static str],
    pub fetch_bytes: u64,
    /// How the notice names the tool.
    pub label: &'static str,
}

/// The pinned tool of `bundler`. Bun: the version every slice of the
/// prebuilt full index was built with (exp152). esbuild, webpack and
/// rollup: the versions the as-of-2026-10-15 index's slices of those
/// bundlers were built with (`/work/library-index/current`, the F1
/// campaign's last multi-bundler index). The rollup plugins were recorded
/// nowhere; each is its newest release at that as-of.
pub fn pin(bundler: RecipeBundler) -> ToolPin {
    match bundler {
        RecipeBundler::Bun => ToolPin {
            bundler,
            version: "1.4.0",
            plugins: &[],
            fetch_bytes: 82_000_000,
            label: "Bun",
        },
        RecipeBundler::Esbuild => ToolPin {
            bundler,
            version: "0.28.2",
            plugins: &[],
            fetch_bytes: 12_000_000,
            label: "esbuild",
        },
        RecipeBundler::Webpack => ToolPin {
            bundler,
            version: "5.111.1",
            plugins: &[],
            fetch_bytes: 26_000_000,
            label: "webpack",
        },
        RecipeBundler::Rollup => ToolPin {
            bundler,
            version: "4.64.5",
            plugins: &[
                "@rollup/plugin-node-resolve@16.0.3",
                "@rollup/plugin-commonjs@29.0.3",
                "@rollup/plugin-json@6.1.0",
                "@rollup/plugin-terser@1.0.0",
            ],
            fetch_bytes: 13_000_000,
            label: "rollup",
        },
    }
}

/// The registry time every tool fetch installs as of (`npm install
/// --before`): the bundler's own dependencies resolve to what they were
/// when the pins were taken, not to whatever is newest on the day of the
/// fetch. Measured, exp181: webpack 5.111.1 installed undated on
/// 2026-11-01 pulls a newer minimizer plugin and fails every build
/// (`EMBEDDED_LANGUAGES is not iterable`); dated, it builds. Every pin
/// above was published before it.
pub const TOOLS_BEFORE: &str = "2026-10-16T00:00:00Z";

/// The npm specs the fetch of `bundler`'s tool installs: the bundler at
/// its pin first, then its recipe's plugins.
pub fn fetch_specs(bundler: RecipeBundler) -> Vec<String> {
    fetch_specs_at(bundler, pin(bundler).version)
}

/// [`fetch_specs`] at an explicit bundler version (exp184: a spec pins its
/// own).
pub fn fetch_specs_at(bundler: RecipeBundler, version: &str) -> Vec<String> {
    let p = pin(bundler);
    std::iter::once(format!("{}@{}", bundler_package_name(bundler), version))
        .chain(p.plugins.iter().map(|s| s.to_string()))
        .collect()
}

/// The npm package the bundler is published as (also its `.bin` name).
fn bundler_package_name(bundler: RecipeBundler) -> &'static str {
    crate::recipe::bundler_package(bundler)
}

/// The recipe script a bundler runs as (file name, bytes), embedded from
/// `crates/humanify-ag-index/recipes/`; None: it runs as its own binary.
pub fn recipe_script(bundler: RecipeBundler) -> Option<(&'static str, &'static str)> {
    match bundler {
        RecipeBundler::Bun | RecipeBundler::Esbuild => None,
        RecipeBundler::Webpack => Some((
            "webpack-build.cjs",
            include_str!("../recipes/webpack-build.cjs"),
        )),
        RecipeBundler::Rollup => Some((
            "rollup-build.mjs",
            include_str!("../recipes/rollup-build.mjs"),
        )),
    }
}

/// `<root>/tools/<bundler>-<version>/`.
pub fn tool_dir(root: &Path, bundler: RecipeBundler, version: &str) -> PathBuf {
    root.join(TOOLS_DIR)
        .join(format!("{}-{version}", bundler.name()))
}

/// A tool the store holds: what a recipe of its bundler runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tool {
    pub bundler: RecipeBundler,
    pub version: String,
    /// The binary (Bun, esbuild) or the recipe script (webpack, rollup).
    pub bin: PathBuf,
}

impl Tool {
    /// `esbuild 0.28.2` — how the notice and the report trail name it.
    pub fn describe(&self) -> String {
        format!("{} {}", pin(self.bundler).label, self.version)
    }
}

/// Whether `dir` holds an install of `bundler` (as far as a lookup looks:
/// the `.bin` binary, or the bundler's package beside where its script
/// goes).
fn installed(dir: &Path, bundler: RecipeBundler) -> bool {
    match recipe_script(bundler) {
        None => dir
            .join("node_modules/.bin")
            .join(bundler_package_name(bundler))
            .is_file(),
        Some(_) => dir
            .join("node_modules")
            .join(bundler_package_name(bundler))
            .join("package.json")
            .is_file(),
    }
}

/// The tool in `dir`, its recipe script (re)written to this binary's bytes.
fn tool_in(dir: &Path, bundler: RecipeBundler, version: &str) -> Result<Tool, String> {
    let bin = match recipe_script(bundler) {
        None => dir
            .join("node_modules/.bin")
            .join(bundler_package_name(bundler)),
        Some((name, bytes)) => {
            let path = dir.join(name);
            if std::fs::read_to_string(&path).ok().as_deref() != Some(bytes) {
                std::fs::write(&path, bytes)
                    .map_err(|e| format!("writing {}: {e}", path.display()))?;
            }
            path
        }
    };
    Ok(Tool {
        bundler,
        version: version.to_string(),
        bin,
    })
}

/// The tool a recipe of `bundler` runs: the NEWEST install the store holds
/// (semver order; the run does not know the bundle's own bundler version).
/// Err: none installed — the error names the store directory and the exact
/// command that fetches the pinned one by hand ([`fetch_tool`] does it).
pub fn find_tool(root: &Path, bundler: RecipeBundler) -> Result<Tool, String> {
    let tools = root.join(TOOLS_DIR);
    let prefix = format!("{}-", bundler.name());
    let mut versions: Vec<String> = std::fs::read_dir(&tools)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter_map(|n| n.strip_prefix(&prefix).map(str::to_string))
        .filter(|v| installed(&tool_dir(root, bundler, v), bundler))
        .collect();
    versions.sort_by(|a, b| compare_versions(b, a));
    match versions.first() {
        Some(v) => tool_in(&tool_dir(root, bundler, v), bundler, v),
        None => Err(format!(
            "no {} under {} (by hand: npm install {} --before={TOOLS_BEFORE} --prefix {} \
             --ignore-scripts)",
            pin(bundler).label,
            tools.display(),
            fetch_specs(bundler).join(" "),
            tool_dir(root, bundler, pin(bundler).version).display()
        )),
    }
}

/// The store's tool for `bundler` at exactly `version`: the install if it
/// is there, else a fetch of it. The spec-mode builder's tool lookup — a
/// spec pins the version, so "the newest install" ([`find_tool`]) is the
/// wrong question.
pub fn tool_at(root: &Path, bundler: RecipeBundler, version: &str) -> Result<Tool, String> {
    let dir = tool_dir(root, bundler, version);
    if installed(&dir, bundler) {
        tool_in(&dir, bundler, version)
    } else {
        fetch_tool_at(root, bundler, version)
    }
}

/// The build that runs `recipe` with the store's tool for its bundler.
pub fn recipe_build(root: &Path, recipe: Recipe) -> Result<(RecipeBuild, Tool), String> {
    let tool = find_tool(root, recipe.bundler)?;
    Ok((
        RecipeBuild {
            recipe,
            bin: tool.bin.clone(),
        },
        tool,
    ))
}

/// The deadline over one tool fetch (an npm install of up to ~80 MB) and
/// over its `--version` check.
const FETCH_DEADLINE: std::time::Duration = std::time::Duration::from_secs(15 * 60);

/// Fetch `bundler`'s pinned tool into `<root>/tools/<bundler>-<version>/`
/// (exp171 for Bun, exp181 for every recipe bundler): `npm install` of
/// [`fetch_specs`] as of [`TOOLS_BEFORE`] with install scripts skipped, then the step a skipped
/// postinstall would have done (Bun, esbuild: the platform binary over the
/// package's placeholder; webpack, rollup: the recipe script beside the
/// install). The tool must answer `--version` with its pin, else the fetch
/// fails loudly and leaves nothing behind.
pub fn fetch_tool(root: &Path, bundler: RecipeBundler) -> Result<Tool, String> {
    fetch_tool_at(root, bundler, pin(bundler).version)
}

/// [`fetch_tool`] at an EXPLICIT version (exp184: an index spec pins its
/// own tools; the runtime path calls this with [`pin`]'s version). The
/// npm packages installed are still [`fetch_specs`]' (the recipe script's
/// own plugin pins).
pub fn fetch_tool_at(root: &Path, bundler: RecipeBundler, version: &str) -> Result<Tool, String> {
    let p = pin(bundler);
    let dir = tool_dir(root, bundler, version);
    let fail = |what: String| -> String {
        let _ = std::fs::remove_dir_all(&dir);
        format!(
            "fetching {} {} into {}: {what}",
            p.label,
            version,
            dir.display()
        )
    };
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| fail(e.to_string()))?;
    let out = humanify_ag_core::subprocess::run(
        Command::new("npm")
            .arg("install")
            .args(fetch_specs_at(bundler, version))
            .arg(format!("--before={TOOLS_BEFORE}"))
            .args(["--prefix"])
            .arg(&dir)
            .args([
                "--ignore-scripts",
                "--no-audit",
                "--no-fund",
                "--loglevel=error",
            ]),
        None,
        FETCH_DEADLINE,
    )
    .map_err(|e| fail(format!("npm install: {e}")))?;
    if !out.status.success() {
        return Err(fail(format!(
            "npm install: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    if let Some((scope, binary, placeholder)) = native_placeholder(bundler) {
        place_platform_binary(&dir, scope, binary, placeholder).map_err(fail)?;
    }
    let tool = tool_in(&dir, bundler, version).map_err(fail)?;
    let said = bundler_version_of(bundler, &tool.bin, FETCH_DEADLINE).map_err(fail)?;
    if said != version {
        return Err(fail(format!(
            "{} --version answered {said:?}",
            tool.bin.display()
        )));
    }
    Ok(tool)
}

/// For a bundler whose npm package ships a placeholder its postinstall
/// replaces: (the scope its platform packages are published under, the
/// binary's path inside one, the placeholder's path under node_modules).
fn native_placeholder(
    bundler: RecipeBundler,
) -> Option<(&'static str, &'static str, &'static str)> {
    match bundler {
        RecipeBundler::Bun => Some(("@oven", "bin/bun", "bun/bin/bun.exe")),
        RecipeBundler::Esbuild => Some(("@esbuild", "bin/esbuild", "esbuild/bin/esbuild")),
        RecipeBundler::Webpack | RecipeBundler::Rollup => None,
    }
}

/// Link (else copy) the ONE platform binary npm installed under
/// `node_modules/<scope>/*/<binary>` over the placeholder — what the
/// package's own postinstall does.
fn place_platform_binary(
    dir: &Path,
    scope: &str,
    binary: &str,
    placeholder: &str,
) -> Result<(), String> {
    let scope_dir = dir.join("node_modules").join(scope);
    let platform = std::fs::read_dir(&scope_dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .map(|e| e.path().join(binary))
        .find(|p| p.is_file())
        .ok_or_else(|| format!("no platform binary under {}", scope_dir.display()))?;
    let target = dir.join("node_modules").join(placeholder);
    let _ = std::fs::remove_file(&target);
    std::fs::hard_link(&platform, &target)
        .or_else(|_| std::fs::copy(&platform, &target).map(|_| ()))
        .map_err(|e| format!("placing {}: {e}", platform.display()))
}
