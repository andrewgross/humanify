//! `run stub <bin> <bundle> <out> [<prior.js>] [--mode names|default]`:
//! run a pipeline binary over one bundle against the ONE stub LLM
//! (`humanify-ag-testkit`, the `<id>Renamed` policy), so every ask is
//! answered instantly and deterministically — the port of
//! `experiments/166-restore-or-ask/stub-run.mts`. Every placement-precision
//! input (claims, the era's placement, the freeze, the raw-to-era bridge)
//! is LLM-independent, so the stub reproduces it. Writes the diagnostics
//! file the vendor-precision check reads (`<out>/diag.json`), the run log
//! and the tree (`<out>/tree`).
//!
//! Names mode (the default) reads the prebuilt index: pairs.json's
//! `libraryIndex` (the index the eval hands its names launches) unless
//! `--library-index` names another — never a run-time build.

use std::path::Path;
use std::time::Duration;

use humanify_ag_testkit::answers::renamed;
use humanify_ag_testkit::stub_server::StubServer;

use super::{Ctx, remove, text, write};

/// One stub run on the biggest bundle (minutes; no LLM wait).
const STUB_RUN_DEADLINE: Duration = Duration::from_secs(4 * 3600);

/// The run's `--config` file (stub-run.mts's, byte for byte: no retries,
/// split on, the log and diagnostics files, and in names mode the index
/// with the structural rule).
pub fn config_text(out: &Path, names_index: Option<&str>) -> String {
    let o = text(out);
    let mut t = format!(
        "[llm]\nretries = 0\n[split]\nenabled = true\n[diagnostics]\nlog_file = \"{o}/run.log\"\ndiagnostics_file = \"{o}/diag.json\"\n"
    );
    if let Some(index) = names_index {
        t.push_str(&format!(
            "[library]\nmatching = \"names\"\nindex = \"{index}\"\n[library.lookup]\nagreement = false\n"
        ));
    }
    t
}

/// Run `bin` on `bundle` into `out` against a fresh stub; the binary's
/// exit code (stderr saved to `<out>/stderr.txt`).
pub fn run(
    ctx: &Ctx,
    bin: &Path,
    bundle: &Path,
    out: &Path,
    prior: Option<&Path>,
    names_index: Option<&str>,
) -> Result<i32, String> {
    remove(out);
    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let toml = out.join("run.toml");
    write(&toml, &config_text(out, names_index))?;
    let stub = StubServer::answering(renamed);
    let mut args: Vec<String> = vec![
        text(bundle),
        "--config".into(),
        text(&toml),
        "--endpoint".into(),
        stub.base_url.clone(),
        "--model".into(),
        "stub".into(),
        "-o".into(),
        text(&out.join("tree")),
    ];
    if let Some(p) = prior {
        args.push("--prior-version".into());
        args.push(text(p));
    }
    let mut s = ctx.spawn(&text(bin), &args, STUB_RUN_DEADLINE);
    s.env.push(("HUMANIFY_API_KEY".into(), "exp166".into()));
    let done = crate::proc::run(&s)?;
    let stderr = String::from_utf8_lossy(&done.stderr).into_owned();
    write(&out.join("stderr.txt"), &stderr)?;
    let code = done.status.code().unwrap_or(1);
    ctx.say(format!("{}: exit {code}", text(out)));
    if code != 0 {
        let tail: String = stderr
            .chars()
            .rev()
            .take(2000)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        ctx.say(tail);
    }
    Ok(code)
}
