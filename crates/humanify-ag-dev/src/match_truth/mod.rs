//! `cargo dev match-truth` — the matcher's ground-truth runner (exp092;
//! successor of experiments/lib/match-truth/run.ts).
//!
//! Download (once) the pinned corpus ([`corpus::CORPUS`]), produce the two
//! input forms the pipeline expects per package pair — the OLD version's
//! shipped build run through the stage-6 formatter (`humanify-ag format`,
//! the closest no-LLM equivalent of a humanified prior) as the prior, and
//! the NEW version's shipped build as the input — run `humanify-ag match`
//! over each pair, score each dump ([`score`]), and write one deterministic
//! scorecard. No LLM is contacted at any point (matching is fully cold; the
//! binary's launches carry an empty `HUMANIFY_API_KEY`).
//!
//! Changed from run.ts: the corpus cache is `--corpus-dir` (default
//! `/work/match-truth-corpus`; it was `experiments/092-*/.corpus`), `--out`
//! is a DIRECTORY holding each pair's work files and `scorecard.json` (run.ts
//! wrote the work files under `experiments/092-*/.runs` and the scorecard
//! to `--out <file>` or stdout), and `--only` takes a list and refuses an
//! unknown package name. The launches themselves are run.ts's, byte for
//! byte (`launch_test.rs` holds them to lane L0's golden).

pub mod canonical;
pub mod corpus;
pub mod score;

use std::path::{Path, PathBuf};
use std::time::Duration;

use humanify_ag_model::js::{JsObject, JsValue, node_path_resolve, stringify_pretty, to_fixed};

use crate::HostEnv;
use crate::committed::sha256_hex;
use crate::paths::repo_root;
use crate::proc::{Spawn, run as run_child};
use crate::settings::Parsed;
use corpus::{CORPUS, CorpusEntry};
use score::{Aggregate, Scorecard, score_match_dump, scorecard_json};

/// The settings this verb reads (its row of the verb table).
pub const ACCEPTED: &[&str] = &["dev.corpus_dir", "run.bin", "run.out", "dev.only"];

/// Where the pinned corpus is cached by default (never committed).
pub const DEFAULT_CORPUS_DIR: &str = "/work/match-truth-corpus";
/// Where a run without `--out` writes (a fresh numbered directory inside).
const DEFAULT_RUNS_DIR: &str = "/work/match-truth-runs";

const NPM_DEADLINE: Duration = Duration::from_secs(5 * 60);
const TAR_DEADLINE: Duration = Duration::from_secs(60);
const FORMAT_DEADLINE: Duration = Duration::from_secs(10 * 60);
const MATCH_DEADLINE: Duration = Duration::from_secs(15 * 60);

/// One run's resolved settings.
struct Config {
    corpus_dir: PathBuf,
    bin: PathBuf,
    out: PathBuf,
    entries: Vec<CorpusEntry>,
}

/// How children are launched: PATH, plus any extra variables (a test's
/// recorder settings).
struct Children {
    env: Vec<(String, String)>,
}

impl Children {
    fn spawn(&self, program: &str, args: &[&str], cwd: &Path, deadline: Duration) -> Spawn {
        let mut s = Spawn::new(program, args, deadline).cwd(cwd);
        for (k, v) in &self.env {
            s = s.env(k, v);
        }
        s
    }

    /// Run to exit 0; stdout bytes. A failure names the command and the
    /// tail of its stderr.
    fn run(&self, spawn: &Spawn) -> Result<Vec<u8>, String> {
        let done = run_child(spawn)?;
        if done.status.success() {
            return Ok(done.stdout);
        }
        let stderr = String::from_utf8_lossy(&done.stderr);
        let tail: Vec<&str> = stderr.lines().rev().take(20).collect();
        let tail: Vec<&str> = tail.into_iter().rev().collect();
        Err(format!(
            "{} exited {}:\n{}",
            spawn.describe(),
            done.status,
            tail.join("\n")
        ))
    }
}

/// Run the verb.
pub fn run(p: &Parsed, host: &HostEnv) -> (u8, String, String) {
    run_with_env(p, host, &[])
}

/// [`run`], with extra variables for every child (the launch test's
/// recorders read theirs this way).
pub fn run_with_env(
    p: &Parsed,
    host: &HostEnv,
    extra: &[(String, String)],
) -> (u8, String, String) {
    let mut env = vec![("PATH".to_string(), host.child_path())];
    env.extend(extra.iter().cloned());
    let children = Children { env };
    match config(p, host).and_then(|c| execute(&c, &children)) {
        Ok(()) => (0, String::new(), String::new()),
        Err(e) => (1, String::new(), format!("match-truth: {e}\n")),
    }
}

/// A path the user typed, resolved the way run.ts's `path.resolve` did:
/// against the working directory, `.`/`..` folded lexically.
fn resolve(host: &HostEnv, p: &str) -> PathBuf {
    let joined = match &host.cwd {
        Some(cwd) => cwd.join(p),
        None => PathBuf::from(p),
    };
    node_path_resolve(&joined)
}

/// The corpus entries `--only` names (all when empty); an unknown name is
/// refused.
fn select(only: &[String]) -> Result<Vec<CorpusEntry>, String> {
    if only.is_empty() {
        return Ok(CORPUS.to_vec());
    }
    let unknown: Vec<&str> = only
        .iter()
        .filter(|n| !CORPUS.iter().any(|c| c.package == n.as_str()))
        .map(String::as_str)
        .collect();
    if !unknown.is_empty() {
        let names: Vec<&str> = CORPUS.iter().map(|c| c.package).collect();
        return Err(format!(
            "no corpus entry named {} (the corpus: {})",
            unknown.join(", "),
            names.join(", ")
        ));
    }
    Ok(CORPUS
        .iter()
        .filter(|c| only.iter().any(|n| n == c.package))
        .copied()
        .collect())
}

/// A fresh directory under `root`: the first free number, CREATED here
/// (`create_dir` is atomic), so two runs started together never pick the
/// same one (exp205: no lock serializes them any more).
fn fresh_out_dir(root: &Path) -> Result<PathBuf, String> {
    std::fs::create_dir_all(root).map_err(|e| format!("{}: {e}", root.display()))?;
    for n in 1.. {
        let p = root.join(format!("run-{n:04}"));
        match std::fs::create_dir(&p) {
            Ok(()) => return Ok(p),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(format!("{}: {e}", p.display())),
        }
    }
    Err(format!("{}: no free run number", root.display()))
}

fn config(p: &Parsed, host: &HostEnv) -> Result<Config, String> {
    let s = &p.settings;
    let corpus_dir = resolve(
        host,
        &s.text("dev.corpus_dir")
            .unwrap_or_else(|| DEFAULT_CORPUS_DIR.to_string()),
    );
    let bin = match s.text("run.bin") {
        Some(b) => resolve(host, &b),
        None => node_path_resolve(&repo_root().join("target/release/humanify-ag")),
    };
    let out = match s.text("run.out") {
        Some(o) => resolve(host, &o),
        None => fresh_out_dir(Path::new(DEFAULT_RUNS_DIR))?,
    };
    let entries = select(&s.list("dev.only").unwrap_or_default())?;
    Ok(Config {
        corpus_dir,
        bin,
        out,
        entries,
    })
}

fn mkdir(dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))
}

fn path_arg(p: &Path) -> String {
    p.display().to_string()
}

/// The tarball for one pinned version, downloaded at most once.
fn tarball(cfg: &Config, ch: &Children, package: &str, version: &str) -> Result<PathBuf, String> {
    let path = cfg.corpus_dir.join(format!("{package}-{version}.tgz"));
    if !path.exists() {
        println!("  downloading {package}@{version}");
        let spec = format!("{package}@{version}");
        let dest = path_arg(&cfg.corpus_dir);
        let args = [
            "pack",
            spec.as_str(),
            "--silent",
            "--pack-destination",
            dest.as_str(),
        ];
        ch.run(&ch.spawn("npm", &args, &cfg.corpus_dir, NPM_DEADLINE))?;
    }
    Ok(path)
}

/// Extract `package/<file>` from the tarball to a stable path. The cache
/// key carries the in-tarball path (flattened), not just the basename — two
/// corpus entries of one package can share a basename at different paths.
fn extract(cfg: &Config, ch: &Children, e: &CorpusEntry, version: &str) -> Result<PathBuf, String> {
    let dest_dir = cfg
        .corpus_dir
        .join("extracted")
        .join(e.package)
        .join(version);
    mkdir(&dest_dir)?;
    let dest = dest_dir.join(e.file.replace('/', "__"));
    if dest.exists() {
        return Ok(dest);
    }
    let tgz = path_arg(&tarball(cfg, ch, e.package, version)?);
    let member = format!("package/{}", e.file);
    let bytes = ch.run(&ch.spawn(
        "tar",
        &["-xzf", tgz.as_str(), "-O", member.as_str()],
        &dest_dir,
        TAR_DEADLINE,
    ))?;
    std::fs::write(&dest, bytes).map_err(|e| format!("cannot write {}: {e}", dest.display()))?;
    Ok(dest)
}

/// One pair: format the old build into the prior, match the new build
/// against it, score the dump.
fn run_pair(cfg: &Config, ch: &Children, e: &CorpusEntry) -> Result<Scorecard, String> {
    println!("== {} {} -> {}", e.package, e.old_version, e.new_version);
    let old_file = path_arg(&extract(cfg, ch, e, e.old_version)?);
    let new_file = path_arg(&extract(cfg, ch, e, e.new_version)?);
    let pair_dir = cfg.out.join(e.package);
    mkdir(&pair_dir)?;
    let bin = path_arg(&cfg.bin);
    let prior = path_arg(&pair_dir.join("prior.formatted.js"));
    let dump_file = pair_dir.join("dump.json");
    let dump = path_arg(&dump_file);
    let work = path_arg(&pair_dir.join("work"));
    // Matching is LLM-free: no key reaches the binary.
    let format = ch
        .spawn(
            &bin,
            &["format", &old_file, "-o", &prior],
            &pair_dir,
            FORMAT_DEADLINE,
        )
        .env("HUMANIFY_API_KEY", "");
    ch.run(&format)?;
    let args = [
        "match",
        new_file.as_str(),
        "--prior-version",
        prior.as_str(),
        "-o",
        dump.as_str(),
        "--work-dir",
        work.as_str(),
    ];
    let matching = ch
        .spawn(&bin, &args, &pair_dir, MATCH_DEADLINE)
        .env("HUMANIFY_API_KEY", "");
    ch.run(&matching)?;
    let text =
        std::fs::read_to_string(&dump_file).map_err(|err| format!("cannot read {dump}: {err}"))?;
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|err| format!("{dump}: {err}"))?;
    let card = score_match_dump(&value)?;
    println!("{}", pair_line(&card));
    Ok(card)
}

fn fixed(r: Option<f64>, digits: usize) -> String {
    r.map_or_else(|| "-".to_string(), |x| to_fixed(x, digits))
}

fn pair_line(card: &Scorecard) -> String {
    let f = &card.functions;
    format!(
        "   functions: must {}/{} (recall {}), should {}/{}, reported {} (identical {}, near {}, far {})",
        f.matched_of_must,
        f.must_match,
        fixed(f.recall, 3),
        f.matched_of_should,
        f.should_match,
        f.reported,
        f.reported_classes.identical,
        f.reported_classes.near,
        f.reported_classes.far
    )
}

fn entry_json(e: &CorpusEntry) -> JsValue {
    JsValue::Object(JsObject::from_entries(vec![
        ("package".into(), JsValue::str(e.package)),
        ("oldVersion".into(), JsValue::str(e.old_version)),
        ("newVersion".into(), JsValue::str(e.new_version)),
        ("file".into(), JsValue::str(e.file)),
    ]))
}

/// run.ts's result object.
fn result_json(cfg: &Config, bin_sha: &str, cards: &[Scorecard], total: &Aggregate) -> JsValue {
    let packages = cfg
        .entries
        .iter()
        .zip(cards)
        .map(|(e, card)| {
            let JsValue::Object(mut o) = entry_json(e) else {
                unreachable!("an entry is an object")
            };
            o.insert("scorecard", scorecard_json(card));
            JsValue::Object(o)
        })
        .collect();
    JsValue::Object(JsObject::from_entries(vec![
        ("schemaVersion".into(), JsValue::Number(1.0)),
        (
            "tool".into(),
            JsValue::str("humanify match (exp092 ground-truth harness)"),
        ),
        (
            "binary".into(),
            JsValue::Object(JsObject::from_entries(vec![
                ("path".into(), JsValue::str(path_arg(&cfg.bin))),
                ("sha256".into(), JsValue::str(bin_sha)),
            ])),
        ),
        (
            "corpus".into(),
            JsValue::Array(cfg.entries.iter().map(entry_json).collect()),
        ),
        ("total".into(), total.to_json()),
        ("packages".into(), JsValue::Array(packages)),
    ]))
}

/// Totals first: the aggregate line, then the five weakest packages.
fn report(cfg: &Config, cards: &[Scorecard], total: &Aggregate) {
    println!(
        "TOTAL {}/{} must-match (recall {}), {}/{} should-match, statements {}/{}",
        total.matched_of_must,
        total.must_match,
        fixed(total.recall(), 4),
        total.matched_of_should,
        total.should_match,
        total.statement_proposed_of_must,
        total.statement_must_match
    );
    let mut weakest: Vec<(&CorpusEntry, &Scorecard)> = cfg.entries.iter().zip(cards).collect();
    let recall = |c: &Scorecard| c.functions.recall.unwrap_or(1.0);
    weakest.sort_by(|a, b| recall(a.1).total_cmp(&recall(b.1)));
    for (e, c) in weakest.iter().take(5) {
        println!(
            "  lead: {} recall {} ({} missed must-pairs)",
            e.package,
            fixed(c.functions.recall, 3),
            c.functions.missed_must.len()
        );
    }
}

fn execute(cfg: &Config, ch: &Children) -> Result<(), String> {
    let bin_bytes = std::fs::read(&cfg.bin).map_err(|_| {
        format!(
            "the humanify binary is not at {} — build it with `cargo build --release --locked -p humanify-ag-cli` or pass --bin",
            cfg.bin.display()
        )
    })?;
    let bin_sha = sha256_hex(&bin_bytes);
    mkdir(&cfg.corpus_dir)?;
    mkdir(&cfg.out)?;
    println!("match-truth: writing into {}", cfg.out.display());
    let cards = cfg
        .entries
        .iter()
        .map(|e| run_pair(cfg, ch, e))
        .collect::<Result<Vec<_>, _>>()?;
    let total = Aggregate::of(&cards);
    let json = stringify_pretty(&result_json(cfg, &bin_sha, &cards, &total), 2) + "\n";
    let card_file = cfg.out.join("scorecard.json");
    std::fs::write(&card_file, json)
        .map_err(|e| format!("cannot write {}: {e}", card_file.display()))?;
    println!("scorecard written to {}", card_file.display());
    report(cfg, &cards, &total);
    Ok(())
}

#[cfg(test)]
#[path = "launch_test.rs"]
mod launch_test;

#[cfg(test)]
mod runs_dir_test;
