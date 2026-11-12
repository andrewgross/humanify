//! `cargo dev e2e [--out D]` (successor of scripts/e2e.ts): the RELEASE
//! BINARY run end to end on the committed e2e fixtures, and its output
//! booted. Per fixture with a committed build
//! (`test/e2e/fixtures/<name>/build/v<ver>/build/index.js`) and per
//! version pair in its `fixture.config.json`:
//!
//! 1. FRESH — the binary humanifies v1 against the ONE stub LLM
//!    (humanify-ag-testkit, in this process), which answers every naming
//!    prompt `<id>Renamed`: the whole LLM path runs, and a run where no
//!    rename lands fails.
//! 2. PRIOR — v2 with `--prior-version` = step 1's output.
//! 3. DETERMINISM — step 2 again into a second directory: byte-identical.
//! 4. BOOT — input and every output imported by Node (the one place the
//!    harness keeps node: running outputs); each must export the SAME
//!    names with the same types, and each exported function called with
//!    no arguments must return a value of the same shape. STRICT: an
//!    export name is the module's API (finding #55; the `esm-exports`
//!    fixture holds every export form). A BUNDLE fixture (`"bundle":
//!    true`) runs split, and its boot compares BEHAVIOUR (stdout + exit of
//!    `node <input>` vs `node run.cjs`; a Bun CJS build runs through
//!    Bun's loader, [`BUN_CJS_LOADER`]); a layout the split does not read
//!    is written unsplit and must hold no runnable tree.
//! 5. SEQUENTIAL — `naming.schedule = "sequential"` + `naming.batch_size
//!    = 10`, fresh and prior each twice, byte-identical, and equal to the
//!    committed legacy goldens (`test/golden/legacy-default/`) where the
//!    pair has one (a post-flip fixture has none: said, not skipped
//!    silently).
//!
//! DETECTION (`detect --toolchain`) is printed first for every pair, and
//! the fixture's `expect` block is checked LAST (`super::expect`). KNOWN
//! GAPS (`super::expect::KNOWN_GAPS`) must fail exactly as declared.
//! Every launch is logged and the log held to the e2e LAUNCH GOLDEN
//! (`launch-goldens/e2e.golden.txt`, recorded by lane L0 from the TS
//! stage), with this run's scratch dir, repo and port normalised.
//! Library names: the committed tiny store `test/e2e/library-index` is
//! named explicitly; a Bun fixture must FIND it, any other must print the
//! skip line, and a run-time index build fails the stage (hermeticity).
//!
//! What it cannot see: model quality (the eval's job). Pass/fail; exit 1
//! on the first failure, naming it. Every child goes through `crate::proc`
//! with a deadline. `--out D` keeps the output trees in D (default: a
//! fresh temp dir, removed after the run).

use std::path::{Path, PathBuf};
use std::time::Duration;

use humanify_ag_model::js::{JsValue, cmp_utf16};
use humanify_ag_testkit::answers;
use humanify_ag_testkit::stub_server::StubServer;

use crate::HostEnv;
use crate::committed::e2e::{Expect, FixtureConfig, VersionPair, fixture_configs};
use crate::e2e::expect::{
    Detection, Files, KNOWN_GAPS, ObservedRun, Verdict, apart_failures, asset_failures,
    expectation_failures, judge_known_gap, or_undefined, placement_failures, stale_known_gaps,
    stays_failures,
};
use crate::e2e::launch_log::{LaunchLog, Setting, first_difference, setting, write_config};
use crate::proc::{Finished, Spawn, run as run_child};
use crate::settings::Parsed;

/// The committed library-index store every launch reads.
const LIBRARY_STORE: &str = "test/e2e/library-index";
/// The legacy goldens of the pre-flip default.
const LEGACY_GOLDENS: &str = "test/golden/legacy-default";
/// The notice a run prints before it BUILDS an index (runtime_index.rs).
const BUILD_NOTICE: &str = "NOTICE: library names (the default) needs a library index";
/// The launch golden this stage holds.
const LAUNCH_GOLDEN: &str = "e2e.golden.txt";
/// How the scratch dir reads in the golden.
const SCRATCH_TOKEN: &str = "<TMP>/humanify-e2e-<RUN>";

/// The deadline over one pipeline launch (fixtures are small: a hang guard).
const LAUNCH_DEADLINE: Duration = Duration::from_secs(600);
/// The deadline over one `detect` or one node probe.
const PROBE_DEADLINE: Duration = Duration::from_secs(120);

/// The module-surface probe: export names → type, and each exported
/// function's no-argument result's shape. `{URL}` is the module's
/// `file://` URL, JSON-quoted.
const SURFACE_PROBE: &str = r#"
    const m = await import({URL});
    const shape = (v) => v === null ? "null" : typeof v === "object"
      ? "{" + Object.keys(v).sort().map((k) => k + ":" + typeof v[k]).join(",") + "}"
      : typeof v;
    const out = {};
    for (const k of Object.keys(m).sort()) {
      out[k] = typeof m[k];
      if (typeof m[k] === "function") {
        try { out[k + "()"] = shape(m[k]()); } catch (e) { out[k + "()"] = "throws " + e.constructor.name; }
      }
    }
    console.log(JSON.stringify(out));
  "#;

/// Bun's CommonJS output (`// @bun @bun-cjs`) is ONE function expression
/// Bun's loader calls; this does what that loader does.
const BUN_CJS_LOADER: &str = r#"
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
const Module = require("node:module");
const file = path.resolve(process.argv[1]);
const mod = new Module(file, null);
mod.filename = file;
mod.paths = Module._nodeModulePaths(path.dirname(file));
const wrapper = vm.runInThisContext(fs.readFileSync(file, "utf8"), { filename: file });
wrapper.call(mod.exports, mod.exports, Module.createRequire(file), mod, file, path.dirname(file));
"#;

/// A Bun code-split entry (`bun build --compile --splitting`, unpacked)
/// imports its chunks as `/$bunfs/root/<chunk>.js`, the compiled app's
/// virtual root; run as-is, Node resolves those beside the entry (Node's
/// working directory is the entry's directory).
const BUNFS_RESOLVE_HOOK: &str = "data:text/javascript,\
import{registerHooks}from\"node:module\";import{pathToFileURL}from\"node:url\";\
const root=process.cwd();registerHooks({resolve(s,c,next){\
return next(s.startsWith(\"/$bunfs/root/\")?pathToFileURL(root+\"/\"+s.slice(13)).href:s,c)}});";

/// The node arguments that run a built file AS-IS from its directory
/// (`base` is its file name): Bun's CommonJS output through Bun's loader,
/// a Bun code-split entry with its chunks resolved, anything else plain.
fn node_run_args(file: &Path, base: &str) -> Vec<String> {
    let text = read_text(file);
    let mut args: Vec<String> = if is_bun_cjs(file) {
        vec!["-e".into(), BUN_CJS_LOADER.into()]
    } else if humanify_ag_core::unpack::input::choose_input_layout(file, &text)
        == humanify_ag_core::unpack::input::InputLayout::BunChunks
    {
        vec!["--import".into(), BUNFS_RESOLVE_HOOK.into()]
    } else {
        Vec::new()
    };
    args.push(base.to_string());
    args
}

/// Why the stage stopped: a check failed (judged against the known gaps),
/// or the harness itself broke (never a known gap).
enum Stop {
    Failure(String),
    Harness(String),
}

type Step<T> = Result<T, Stop>;

fn fail<T>(msg: impl Into<String>) -> Step<T> {
    Err(Stop::Failure(msg.into()))
}

fn harness<E: std::fmt::Display>(e: E) -> Stop {
    Stop::Harness(e.to_string())
}

/// The last `n` characters of `s` (a failure message's stderr tail).
fn tail(s: &str, n: usize) -> &str {
    let count = s.chars().count();
    match s.char_indices().nth(count.saturating_sub(n)) {
        Some((i, _)) if count > n => &s[i..],
        _ => s,
    }
}

/// One fixture pair.
struct FixturePair {
    name: String,
    pair: VersionPair,
    bundle: bool,
    expect: Option<Expect>,
}

impl FixturePair {
    fn label(&self) -> String {
        format!("{} {}->{}", self.name, self.pair.v1, self.pair.v2)
    }
}

/// The three output legs the boot step reads.
struct Legs {
    fresh: PathBuf,
    prior: PathBuf,
    sequential: PathBuf,
}

impl Legs {
    fn each(&self) -> [&Path; 3] {
        [&self.fresh, &self.prior, &self.sequential]
    }
}

/// The stage's state: where things are, the stub, the launch log.
struct Stage {
    repo: PathBuf,
    bin: PathBuf,
    endpoint: String,
    store: PathBuf,
    log: LaunchLog,
    child_path: String,
}

/// Every file under `dir`, relative (`/`-joined), sorted as JS sorts.
fn tree_files(dir: &Path) -> Vec<String> {
    fn walk(root: &Path, rel: &str, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(root.join(rel)) else {
            return;
        };
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            let r = if rel.is_empty() {
                name
            } else {
                format!("{rel}/{name}")
            };
            let is_dir = e.file_type().is_ok_and(|t| t.is_dir());
            if is_dir {
                walk(root, &r, out);
            } else if std::fs::metadata(root.join(&r)).is_ok_and(|m| m.is_file()) {
                out.push(r);
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, "", &mut out);
    out.sort_by(|a, b| cmp_utf16(a, b));
    out
}

/// Every file under `tree/<dir>`, keyed `<dir>/<relative path>`.
fn dir_texts(tree: &Path, dir: &str) -> Files {
    let root = tree.join(dir);
    tree_files(&root)
        .into_iter()
        .map(|f| {
            let text = String::from_utf8_lossy(&std::fs::read(root.join(&f)).unwrap_or_default())
                .to_string();
            (format!("{dir}/{f}"), text)
        })
        .collect()
}

fn read_text(file: &Path) -> String {
    String::from_utf8_lossy(&std::fs::read(file).unwrap_or_default()).to_string()
}

fn assert_identical_trees(a: &Path, b: &Path, label: &str) -> Step<()> {
    let (fa, fb) = (tree_files(a), tree_files(b));
    if fa != fb {
        return fail(format!("{label}: the two runs wrote different file sets"));
    }
    for f in &fa {
        if std::fs::read(a.join(f)).ok() != std::fs::read(b.join(f)).ok() {
            return fail(format!("{label}: {f} differs between two identical runs"));
        }
    }
    Ok(())
}

/// True for Bun's CommonJS output (`// @bun … @bun-cjs` first line).
fn is_bun_cjs(file: &Path) -> bool {
    let text = read_text(file);
    let first = text.split('\n').next().unwrap_or("");
    let word = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let Some(rest) = first.strip_prefix("// @bun") else {
        return false;
    };
    if rest.starts_with(word) {
        return false;
    }
    rest.match_indices("@bun-cjs")
        .any(|(i, m)| !rest[i + m.len()..].starts_with(word))
}

/// The prior the next release diffs against, from the run's `Next
/// release: --prior-version <path>` line.
fn next_release_prior(stderr: &str, label: &str) -> Step<String> {
    stderr
        .lines()
        .find_map(|l| l.strip_prefix("Next release: --prior-version "))
        .filter(|p| !p.is_empty())
        .map(str::to_string)
        .map_or_else(
            || {
                fail(format!(
                    "{label}: the run named no prior for the next release"
                ))
            },
            Ok,
        )
}

/// The skip line's kind, if `stderr` holds the skip line for `store`.
fn skip_kind<'a>(stderr: &'a str, store: &str) -> Option<&'a str> {
    let tail_text = format!(" bundles in {store} — skipping library name matching (");
    stderr.lines().find_map(|l| {
        let rest = l.strip_prefix("no library index for ")?;
        let end = rest.find(char::is_whitespace)?;
        let (kind, after) = rest.split_at(end);
        (!kind.is_empty() && after.starts_with(&tail_text)).then_some(kind)
    })
}

impl Stage {
    fn bin_str(&self) -> String {
        self.bin.display().to_string()
    }

    fn input_of(&self, name: &str, version: &str) -> Step<PathBuf> {
        let p = self
            .repo
            .join(crate::paths::E2E_FIXTURES)
            .join(name)
            .join("build")
            .join(format!("v{version}"))
            .join("build/index.js");
        if !p.exists() {
            return fail(format!("{name}: no committed build at {}", p.display()));
        }
        Ok(p)
    }

    /// The child's PATH: the host's, with the user tool dirs first.
    fn with_path(&self, spawn: Spawn) -> Spawn {
        if self.child_path.is_empty() {
            spawn
        } else {
            spawn.env("PATH", &self.child_path)
        }
    }

    fn node(&self, args: &[&str], cwd: &Path) -> Result<Finished, String> {
        run_child(&self.with_path(Spawn::new("node", args, PROBE_DEADLINE).cwd(cwd)))
    }

    /// One pipeline launch: settings into `<out>.toml`, the launch logged,
    /// exit 0 required and no run-time index build. Returns stderr.
    fn run_binary(&mut self, args: &[String], settings: &[Setting], label: &str) -> Step<String> {
        let out = args
            .iter()
            .position(|a| a == "-o")
            .and_then(|i| args.get(i + 1))
            .ok_or_else(|| harness("a launch without -o"))?;
        let config = PathBuf::from(format!("{out}.toml"));
        if let Some(parent) = config.parent() {
            std::fs::create_dir_all(parent).map_err(harness)?;
        }
        let mut all = vec![
            setting("llm.retries", JsValue::Number(0.0)),
            setting(
                "library.index_store_dir",
                JsValue::str(self.store.display().to_string()),
            ),
        ];
        all.extend(settings.iter().cloned());
        write_config(&config, &all).map_err(Stop::Harness)?;
        let mut argv = args.to_vec();
        argv.extend([
            "--endpoint".into(),
            self.endpoint.clone(),
            "--config".into(),
            config.display().to_string(),
        ]);
        self.log
            .record(&self.bin_str(), "e2e", &argv)
            .map_err(Stop::Harness)?;
        let mut spawn = self
            .with_path(Spawn::new(&self.bin_str(), &[], LAUNCH_DEADLINE))
            .env("HUMANIFY_API_KEY", "e2e");
        spawn.args = argv;
        let done = run_child(&spawn).map_err(|e| Stop::Failure(format!("{label}: {e}")))?;
        let stderr = String::from_utf8_lossy(&done.stderr).to_string();
        if !done.status.success() {
            let code = done
                .status
                .code()
                .map_or("null".to_string(), |c| c.to_string());
            return fail(format!(
                "{label}: the binary exited {code}\n{}",
                tail(&stderr, 2000)
            ));
        }
        if stderr.contains(BUILD_NOTICE) {
            return fail(format!(
                "{label}: the run took the run-time index build path — e2e must never fetch or build one \
                 (hermeticity); the committed store {} must serve every Bun fixture",
                self.store.display()
            ));
        }
        Ok(stderr)
    }

    /// `humanify-ag detect --toolchain` on one input. Detection asks no
    /// model, so it is handed an empty API key (the TS removed it).
    fn detection_of(&mut self, file: &Path, label: &str) -> Step<Detection> {
        let argv = vec![
            "detect".to_string(),
            "--toolchain".to_string(),
            file.display().to_string(),
        ];
        self.log
            .record(&self.bin_str(), "", &argv)
            .map_err(Stop::Harness)?;
        let mut spawn = self
            .with_path(Spawn::new(&self.bin_str(), &[], PROBE_DEADLINE))
            .env("HUMANIFY_API_KEY", "");
        spawn.args = argv;
        let done = run_child(&spawn).map_err(|e| Stop::Failure(format!("{label}: {e}")))?;
        if !done.status.success() {
            return fail(format!(
                "{label}: detect --toolchain failed\n{}",
                String::from_utf8_lossy(&done.stderr)
            ));
        }
        let stdout = String::from_utf8_lossy(&done.stdout).to_string();
        let last = stdout.trim().split('\n').next_back().unwrap_or("");
        let v: serde_json::Value = serde_json::from_str(last)
            .map_err(|e| Stop::Failure(format!("{label}: detect --toolchain: {e}")))?;
        let sel = &v["selection"];
        let text = |k: &str| sel.get(k).and_then(|x| x.as_str()).map(str::to_string);
        let layout = v["toolchain"].as_array().and_then(|pieces| {
            pieces
                .iter()
                .find(|p| p["piece"] == "bundleLayout")
                .and_then(|p| p["choice"].as_str())
                .map(str::to_string)
        });
        Ok(Detection {
            bundler: text("bundler").unwrap_or_else(|| "undefined".into()),
            bundler_tier: text("bundlerTier"),
            minifier: text("minifier"),
            unpack_adapter: text("unpackAdapter").unwrap_or_else(|| "undefined".into()),
            bundle_layout: layout,
        })
    }

    /// Which library path a launch took: a Bun bundle must FIND the
    /// committed index; any other must print the skip line for its kind.
    fn library_path(&self, stderr: &str, bundler: &str) -> Step<String> {
        let found = stderr
            .lines()
            .any(|l| l.starts_with("Library index: using "));
        let skip = skip_kind(stderr, &self.store.display().to_string());
        if bundler == "bun" {
            if found && skip.is_none() {
                return Ok("names, the committed index FOUND".into());
            }
            return fail(format!(
                "a Bun bundle ran without the found-index line:\n{}",
                tail(stderr, 2000)
            ));
        }
        let Some(kind) = skip.filter(|_| !found) else {
            return fail(format!(
                "a {bundler} bundle ran without the skip line (the committed store has Bun slices only):\n{}",
                tail(stderr, 2000)
            ));
        };
        if kind == "bun" || (bundler != "unknown" && kind != bundler) {
            return fail(format!(
                "the skip line names kind {kind} for a {bundler} bundle"
            ));
        }
        Ok(format!(
            "names, SKIPPED: no {kind} index in the committed store"
        ))
    }

    /// Step 1. Returns the prior reference the prior legs read.
    fn fresh_leg(
        &mut self,
        fp: &FixturePair,
        fresh: &Path,
        split: &[Setting],
        bundler: &str,
    ) -> Step<String> {
        let label = fp.label();
        let mut settings = split.to_vec();
        if fp.bundle {
            settings.push(setting(
                "diagnostics.stats_json",
                JsValue::str(format!("{}.stats.json", fresh.display())),
            ));
        }
        let input = self.input_of(&fp.name, &fp.pair.v1)?;
        let args = vec![
            input.display().to_string(),
            "-o".into(),
            fresh.display().to_string(),
        ];
        let stderr = self.run_binary(&args, &settings, &format!("{label} fresh"))?;
        println!("  library: {}", self.library_path(&stderr, bundler)?);
        let fresh_out = fresh.join("index.js");
        if !fp.bundle && !fresh_out.exists() {
            return fail(format!("{label}: fresh run wrote no index.js"));
        }
        let landed = if fp.bundle {
            tree_files(fresh)
                .iter()
                .any(|f| read_text(&fresh.join(f)).contains("Renamed"))
        } else {
            read_text(&fresh_out).contains("Renamed")
        };
        if !landed {
            return fail(format!(
                "{label}: no stub rename landed — the LLM path did not run end to end"
            ));
        }
        if fp.bundle {
            next_release_prior(&stderr, &format!("{label} fresh"))
        } else {
            Ok(fresh_out.display().to_string())
        }
    }

    /// v2 with `--prior-version prior`, into `a` then `b`: byte-identical.
    /// Returns the first run's stderr.
    fn twice(
        &mut self,
        input: &Path,
        prior: Option<&str>,
        (a, b): (&Path, &Path),
        settings: &[Setting],
        (label, step): (&str, &str),
    ) -> Step<String> {
        let mut base = vec![input.display().to_string()];
        if let Some(p) = prior {
            base.extend(["--prior-version".to_string(), p.to_string()]);
        }
        let with_out = |o: &Path| {
            let mut v = base.clone();
            v.extend(["-o".to_string(), o.display().to_string()]);
            v
        };
        let stderr = self.run_binary(&with_out(a), settings, &format!("{label} {step}"))?;
        self.run_binary(&with_out(b), settings, &format!("{label} {step} (again)"))?;
        Ok(stderr)
    }

    /// Steps 2-3.
    fn prior_leg(
        &mut self,
        fp: &FixturePair,
        prior_ref: &str,
        root: &Path,
        split: &[Setting],
    ) -> Step<()> {
        let label = fp.label();
        let input = self.input_of(&fp.name, &fp.pair.v2)?;
        let (a, b) = (root.join("prior-a"), root.join("prior-b"));
        self.twice(&input, Some(prior_ref), (&a, &b), split, (&label, "prior"))?;
        assert_identical_trees(&a, &b, &label)
    }

    /// The byte comparison against a committed legacy golden, if any.
    fn compare_golden(&self, actual: &Path, golden: &Path, label: &str, step: &str) -> Step<()> {
        if golden.exists() {
            return assert_identical_trees(
                actual,
                golden,
                &format!("{label} {step} vs the pre-flip default's golden"),
            );
        }
        println!(
            "  {label}: no committed golden for this pair — golden comparison skipped ({step})"
        );
        Ok(())
    }

    /// Step 5. Returns the sequential-prior output dir.
    fn sequenced_leg(&mut self, fp: &FixturePair, root: &Path, split: &[Setting]) -> Step<PathBuf> {
        let label = fp.label();
        let mut seq = split.to_vec();
        seq.push(setting("naming.schedule", JsValue::str("sequential")));
        seq.push(setting("naming.batch_size", JsValue::Number(10.0)));
        let goldens = self.repo.join(LEGACY_GOLDENS);
        let (fresh_a, fresh_b) = (root.join("seq-fresh-a"), root.join("seq-fresh-b"));
        let (prior_a, prior_b) = (root.join("seq-prior-a"), root.join("seq-prior-b"));
        let v1 = self.input_of(&fp.name, &fp.pair.v1)?;
        let stderr = self.twice(
            &v1,
            None,
            (&fresh_a, &fresh_b),
            &seq,
            (&label, "sequential fresh"),
        )?;
        assert_identical_trees(&fresh_a, &fresh_b, &format!("{label} --sequential fresh"))?;
        let fresh_golden = goldens.join(format!("{}-{}-fresh", fp.name, fp.pair.v1));
        self.compare_golden(&fresh_a, &fresh_golden, &label, "--sequential fresh")?;
        let prior_ref = if fp.bundle {
            next_release_prior(&stderr, &format!("{label} sequential fresh"))?
        } else {
            fresh_a.join("index.js").display().to_string()
        };
        let v2 = self.input_of(&fp.name, &fp.pair.v2)?;
        self.twice(
            &v2,
            Some(&prior_ref),
            (&prior_a, &prior_b),
            &seq,
            (&label, "sequential prior"),
        )?;
        assert_identical_trees(&prior_a, &prior_b, &format!("{label} --sequential prior"))?;
        let prior_golden = goldens.join(format!("{}-{}-{}", fp.name, fp.pair.v1, fp.pair.v2));
        self.compare_golden(&prior_a, &prior_golden, &label, "--sequential prior")?;
        Ok(prior_a)
    }

    /// A BUNDLE's observable behaviour: stdout and exit of the file run
    /// AS-IS by Node in its own directory.
    fn behavior_of(&self, file: &Path, label: &str) -> Step<(String, i32)> {
        let dir = file.parent().unwrap_or(Path::new("/"));
        let base = file
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let owned = node_run_args(file, &base);
        let args: Vec<&str> = owned.iter().map(String::as_str).collect();
        let done = self.node(&args, dir).map_err(|e| {
            Stop::Failure(format!(
                "{label}: {} could not be run by Node:\n{e}",
                file.display()
            ))
        })?;
        let Some(code) = done.status.code() else {
            return fail(format!(
                "{label}: {} could not be run by Node:\n{}",
                file.display(),
                String::from_utf8_lossy(&done.stderr)
            ));
        };
        Ok((String::from_utf8_lossy(&done.stdout).to_string(), code))
    }

    /// The module's observable surface, probed in a child Node.
    fn surface_of(&self, file: &Path, label: &str) -> Step<String> {
        let url = serde_json::to_string(&format!("file://{}", file.display())).map_err(harness)?;
        let probe = SURFACE_PROBE.replace("{URL}", &url);
        let dir = file.parent().unwrap_or(Path::new("/"));
        let done = self
            .node(&["--input-type=module", "-e", &probe], dir)
            .map_err(|e| {
                Stop::Failure(format!(
                    "{label}: {} does not load under Node:\n{e}",
                    file.display()
                ))
            })?;
        if !done.status.success() {
            return fail(format!(
                "{label}: {} does not load under Node:\n{}",
                file.display(),
                String::from_utf8_lossy(&done.stderr)
            ));
        }
        Ok(String::from_utf8_lossy(&done.stdout).trim().to_string())
    }

    /// A bundle's split trees boot with the input's behaviour.
    fn boot_trees(&self, fp: &FixturePair, legs: &Legs) -> Step<()> {
        let label = fp.label();
        let versions = [&fp.pair.v1, &fp.pair.v2, &fp.pair.v2];
        for ((out, tag), version) in legs
            .each()
            .into_iter()
            .zip(["fresh", "prior", "sequential"])
            .zip(versions)
        {
            let input = self.input_of(&fp.name, version)?;
            let want = self.behavior_of(&input, &format!("{label} input v{version}"))?;
            let got = self.behavior_of(
                &out.join("run.cjs"),
                &format!("{label} {tag} output v{version}"),
            )?;
            if got != want {
                let show = |(stdout, status): &(String, i32)| {
                    format!(
                        "{{\"stdout\":{},\"status\":{status}}}",
                        serde_json::to_string(stdout).unwrap_or_default()
                    )
                };
                return fail(format!(
                    "{label}: v{version}'s split tree behaves differently\n  input:  {}\n  output: {}",
                    show(&want),
                    show(&got)
                ));
            }
        }
        println!(
            "  {label}: fresh + prior (+ --sequential, twice) split trees ran, deterministic, boots with the input's behavior"
        );
        Ok(())
    }

    /// A single-module fixture boots with the input's module surface.
    fn boot_surfaces(&self, fp: &FixturePair, root: &Path, legs: &Legs) -> Step<()> {
        let label = fp.label();
        let versions = [&fp.pair.v1, &fp.pair.v2, &fp.pair.v2];
        for ((out, tag), version) in legs
            .each()
            .into_iter()
            .zip(["fresh", "prior", "sequential"])
            .zip(versions)
        {
            let input = self.input_of(&fp.name, version)?;
            let in_mod = as_module(&input, &root.join(format!("boot-in-{version}")))?;
            let want = self.surface_of(&in_mod, &format!("{label} input v{version}"))?;
            let out_mod = as_module(&out.join("index.js"), &root.join(format!("boot-out-{tag}")))?;
            let got = self.surface_of(&out_mod, &format!("{label} {tag} output v{version}"))?;
            if got != want {
                return fail(format!(
                    "{label}: v{version}'s output boots with a different surface\n  input:  {want}\n  output: {got}"
                ));
            }
        }
        println!(
            "  {label}: fresh + prior (+ --sequential, twice, vs the legacy goldens) ran, deterministic, boots with the input's surface"
        );
        Ok(())
    }

    /// The run steps of one pair, then the boot step.
    fn run_pair(&mut self, fp: &FixturePair, root: &Path, bundler: &str) -> Step<()> {
        let fresh = root.join("fresh");
        // Said EXPLICITLY both ways: the split is on by default since
        // exp179, and a library fixture keeps its one-file run.
        let split = vec![setting("split.enabled", JsValue::Bool(fp.bundle))];
        let prior_ref = self.fresh_leg(fp, &fresh, &split, bundler)?;
        self.prior_leg(fp, &prior_ref, root, &split)?;
        let sequential = self.sequenced_leg(fp, root, &split)?;
        let legs = Legs {
            fresh: fresh.clone(),
            prior: root.join("prior-a"),
            sequential,
        };
        if !fp.bundle {
            self.boot_surfaces(fp, root, &legs)
        } else if fresh.join("run.cjs").exists() {
            self.boot_trees(fp, &legs)
        } else {
            check_unsplit(&fp.label(), &legs)
        }
    }

    /// One pair end to end: detection printed, the run steps, then the
    /// fixture's declared expectations.
    fn check_pair(&mut self, fp: &FixturePair, scratch: &Path) -> Step<()> {
        let label = fp.label();
        let input = self.input_of(&fp.name, &fp.pair.v1)?;
        let d = self.detection_of(&input, &label)?;
        println!(
            "  {label}: detect v{}: bundler={} ({}), minifier={}, unpack={}, layout={}",
            fp.pair.v1,
            d.bundler,
            or_undefined(&d.bundler_tier),
            or_undefined(&d.minifier),
            d.unpack_adapter,
            or_undefined(&d.bundle_layout)
        );
        let root = scratch.join(format!("{}-{}-{}", fp.name, fp.pair.v1, fp.pair.v2));
        let fresh = root.join("fresh");
        self.run_pair(fp, &root, &d.bundler)?;
        let vendor_files = if fp.bundle {
            tree_files(&fresh.join("vendor"))
                .iter()
                .filter(|f| f.ends_with(".js"))
                .count()
        } else {
            0
        };
        let split_method = if fp.bundle {
            split_method_of(&PathBuf::from(format!("{}.stats.json", fresh.display())))
        } else {
            None
        };
        if fp.bundle {
            println!(
                "  {label}: fresh split method: {}; vendor/: {vendor_files} file(s)",
                or_undefined(&split_method)
            );
        }
        let missed = expectations_missed(fp, &d, &root, vendor_files, split_method);
        if !missed.is_empty() {
            return fail(format!("{label}: {}", missed.join("; ")));
        }
        Ok(())
    }

    /// One pair held to its known-gap entry. Ok(true): a recorded gap.
    fn judged_pair(&mut self, fp: &FixturePair, scratch: &Path) -> Step<bool> {
        let failure = match self.check_pair(fp, scratch) {
            Ok(()) => None,
            Err(Stop::Failure(f)) => Some(f),
            Err(h) => return Err(h),
        };
        let gap = KNOWN_GAPS.iter().find(|g| g.fixture == fp.name);
        let judged = judge_known_gap(gap, failure.as_deref());
        match judged.verdict {
            Verdict::Fail => fail(judged.message),
            Verdict::KnownGap => {
                println!("  {}", judged.message);
                Ok(true)
            }
            Verdict::Pass => Ok(false),
        }
    }

    /// The log held to the committed launch golden.
    fn hold_launch_golden(&self, scratch: &Path, port: &str) -> Step<()> {
        let recorded = normalize_launches(&self.log, scratch, &self.repo, port);
        let file = self
            .repo
            .join(crate::paths::LAUNCH_GOLDENS)
            .join(LAUNCH_GOLDEN);
        let golden = std::fs::read_to_string(&file)
            .unwrap_or_else(|_| format!("<no golden at {}>", file.display()));
        if recorded != golden {
            return fail(format!(
                "the e2e launches changed — {} is the launch protocol the runner must reproduce; \
                 a deliberate change is re-recorded from a live run and says why each line changed\n{}",
                file.display(),
                first_difference(&recorded, &golden)
            ));
        }
        Ok(())
    }
}

/// The log with the run's scratch dir, repo and port replaced, as the
/// golden stores them (scripts/e2e.ts `normalizedLaunches`).
fn normalize_launches(log: &LaunchLog, scratch: &Path, repo: &Path, port: &str) -> String {
    let text = log.normalized(&[
        (scratch.display().to_string(), SCRATCH_TOKEN.to_string()),
        (repo.display().to_string(), "<REPO>".to_string()),
    ]);
    text.replace(
        &format!("http://127.0.0.1:{port}/v1"),
        "http://127.0.0.1:<PORT>/v1",
    )
}

/// The fixture's declared expectations the run missed.
fn expectations_missed(
    fp: &FixturePair,
    d: &Detection,
    root: &Path,
    vendor_files: usize,
    split_method: Option<String>,
) -> Vec<String> {
    let fresh = root.join("fresh");
    let e = fp.expect.as_ref();
    let app = dir_texts(&fresh, "src");
    let app_files: Vec<String> = app.iter().map(|(f, _)| f.clone()).collect();
    let mut missed = expectation_failures(
        e,
        d,
        &ObservedRun {
            vendor_files,
            split_method,
        },
    );
    let none: Vec<String> = Vec::new();
    missed.extend(apart_failures(
        e.and_then(|e| e.apart.as_deref()).unwrap_or(&[]),
        &app,
    ));
    missed.extend(asset_failures(
        e.and_then(|e| e.assets.as_ref()).unwrap_or(&none),
        &app_files,
    ));
    missed.extend(stays_failures(
        e.and_then(|e| e.stays.as_ref()).unwrap_or(&none),
        &dir_texts(&fresh, "vendor"),
        &dir_texts(&root.join("prior-a"), "vendor"),
    ));
    missed.extend(placement_failures(
        &e.and_then(|e| e.placement.clone()).unwrap_or_default(),
        &app,
        &dir_texts(&fresh, "vendor"),
    ));
    missed
}

/// The fresh run's stats file `splitMethod.method`, if written.
fn split_method_of(stats: &Path) -> Option<String> {
    let text = std::fs::read_to_string(stats).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    v["splitMethod"]["method"].as_str().map(str::to_string)
}

/// Copy a file into an ESM scope so Node reads its `export`s as a module.
fn as_module(file: &Path, dir: &Path) -> Step<PathBuf> {
    std::fs::create_dir_all(dir).map_err(harness)?;
    std::fs::write(dir.join("package.json"), "{\"type\":\"module\"}\n").map_err(harness)?;
    let dest = dir.join("index.js");
    std::fs::copy(file, &dest).map_err(|e| harness(format!("{}: {e}", file.display())))?;
    Ok(dest)
}

/// A bundle layout the split does not read: no runnable tree may exist.
fn check_unsplit(label: &str, legs: &Legs) -> Step<()> {
    for out in legs.each() {
        for f in ["run.cjs", ".humanify/split-ledger.json"] {
            if out.join(f).exists() {
                return fail(format!(
                    "{label}: {} holds {f} but no runnable tree",
                    out.display()
                ));
            }
        }
    }
    println!(
        "  {label}: fresh + prior (+ --sequential, twice) ran unsplit, deterministic — no runnable tree to boot"
    );
    Ok(())
}

/// Every fixture pair, fixtures in name order.
fn fixture_pairs(repo: &Path) -> Result<Vec<FixturePair>, String> {
    let configs = fixture_configs(repo)?;
    let mut out = Vec::new();
    for (name, loaded) in configs {
        let cfg: &FixtureConfig = &loaded.value;
        for pair in &cfg.version_pairs {
            out.push(FixturePair {
                name: name.clone(),
                pair: pair.clone(),
                bundle: cfg.bundle == Some(true),
                expect: cfg.expect.clone(),
            });
        }
    }
    Ok(out)
}

/// The whole stage on the checkout at `repo`, trees under `scratch`.
/// Ok: the summary line.
fn stage(repo: &Path, scratch: &Path, child_path: &str) -> Step<String> {
    let bin = crate::e2e::release_binary(repo).map_err(Stop::Failure)?;
    let pairs = fixture_pairs(repo).map_err(Stop::Harness)?;
    if pairs.is_empty() {
        return fail(format!(
            "no fixture pairs under {}",
            repo.join(crate::paths::E2E_FIXTURES).display()
        ));
    }
    let names: Vec<String> = pairs.iter().map(|p| p.name.clone()).collect();
    let stale = stale_known_gaps(KNOWN_GAPS, &names);
    if !stale.is_empty() {
        return fail(stale.join("\n"));
    }
    let server = StubServer::answering(answers::renamed);
    let port = server
        .base_url
        .trim_start_matches("http://127.0.0.1:")
        .trim_end_matches("/v1")
        .to_string();
    let mut st = Stage {
        repo: repo.to_path_buf(),
        bin,
        endpoint: server.base_url.clone(),
        store: repo.join(LIBRARY_STORE),
        log: LaunchLog::default(),
        child_path: child_path.to_string(),
    };
    let mut gaps = 0;
    for fp in &pairs {
        if st.judged_pair(fp, scratch)? {
            gaps += 1;
        }
    }
    st.hold_launch_golden(scratch, &port)?;
    Ok(format!(
        "e2e: {} fixture pair(s) passed, {gaps} known gap(s) failed exactly as declared; {} launches equal to {}/{LAUNCH_GOLDEN}",
        pairs.len() - gaps,
        st.log.len(),
        crate::paths::LAUNCH_GOLDENS
    ))
}

/// The stage on `repo`; `out`: keep the trees there.
pub fn e2e(repo: &Path, out: Option<&Path>, child_path: &str) -> (u8, String, String) {
    let scratch = match out {
        Some(o) => std::fs::create_dir_all(o)
            .map_err(|e| format!("{}: {e}", o.display()))
            .and_then(|()| {
                o.canonicalize()
                    .map_err(|e| format!("{}: {e}", o.display()))
            }),
        None => crate::e2e::fresh_dir("humanify-e2e-"),
    };
    let scratch = match scratch {
        Ok(s) => s,
        Err(e) => return (1, String::new(), format!("E2E FAILED: {e}\n")),
    };
    let result = stage(repo, &scratch, child_path);
    if out.is_none() {
        let _ = std::fs::remove_dir_all(&scratch);
    }
    match result {
        Ok(summary) => (0, format!("{summary}\n"), String::new()),
        Err(Stop::Failure(m) | Stop::Harness(m)) => {
            (1, String::new(), format!("E2E FAILED: {m}\n"))
        }
    }
}

/// Run the verb.
pub fn run(p: &Parsed, host: &HostEnv) -> (u8, String, String) {
    let out = p.settings.text("run.out").map(|o| match &host.cwd {
        Some(cwd) => cwd.join(o),
        None => PathBuf::from(o),
    });
    e2e(&crate::e2e::repo(), out.as_deref(), &host.child_path())
}

#[cfg(test)]
#[path = "run_test.rs"]
mod run_test;
