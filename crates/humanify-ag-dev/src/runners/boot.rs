//! The boot gate, once (the port of `experiments/lib/boot-gate.sh`): does
//! a humanified Claude Code tree BOOT (`--version` must echo the version)
//! and answer a live prompt (`-p "say exactly: boot-ok"`, pinned to the
//! boot model)? `--version` alone proves the module graph loads, not that
//! the tree runs — both halves, always (the prompt half can be switched
//! off, and then says `skipped`).
//!
//! bun and the walked CLI stay external tools. The launch is the shell
//! gate's exactly — `timeout <s> env -u <session vars> bun run.cjs ...` in
//! the tree (the boot-gate golden holds argv, cwd and which variables
//! reach bun) — so the deadline is `timeout`'s and the scrub is `env`'s.
//!
//! A run without bun is not gated: [`require_bun`] is fatal before any
//! pair runs (bun lives in `~/.bun/bin`, which the child PATH puts first;
//! the gate once silently skipped because it was only off PATH).

use std::path::Path;
use std::time::Duration;

use super::{Ctx, text};

/// The ONE environment a walked CLI boots under. A launching agent session
/// exports state the walked CLI must not inherit:
/// - its MODEL overrides (ANTHROPIC_MODEL, the per-tier defaults, the small
///   fast model): the walked CLI would remap the pinned model and the live
///   prompt fails "issue with the selected model" (2026-10-02; the small
///   fast model found leaking by the exp191 golden);
/// - its ENDPOINT override and auth (ANTHROPIC_BASE_URL, ...): the request
///   would die at the launcher's endpoint (2026-10-27, exp156);
/// - its SESSION identity (CLAUDECODE, CLAUDE_CODE_*): the walked CLI would
///   act as a child of the launching session, or refuse to start.
///
/// CLAUDE_CONFIG_DIR is deliberately KEPT: it says where the walked CLI
/// finds its credentials, and the live prompt needs them.
pub const SCRUBBED: &[&str] = &[
    "ANTHROPIC_MODEL",
    "ANTHROPIC_DEFAULT_OPUS_MODEL",
    "ANTHROPIC_DEFAULT_SONNET_MODEL",
    "ANTHROPIC_DEFAULT_HAIKU_MODEL",
    "ANTHROPIC_DEFAULT_FABLE_MODEL",
    "CLAUDE_CODE_SUBAGENT_MODEL",
    "ANTHROPIC_SMALL_FAST_MODEL",
    "ANTHROPIC_BASE_URL",
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "CLAUDECODE",
    "CLAUDE_CODE_ENTRYPOINT",
    "CLAUDE_CODE_CHILD_SESSION",
    "CLAUDE_CODE_SESSION_ID",
    "CLAUDE_CODE_MESSAGING_SOCKET",
    "CLAUDE_CODE_MESSAGING_TOKEN",
    "CLAUDE_CODE_BRIDGE_SESSION_ID",
];

/// `timeout`'s deadline for `--version`, and for the live prompt.
const VERSION_SECS: u64 = 60;
const PROMPT_SECS: u64 = 120;

/// Fatal when bun is not on the child PATH.
pub fn require_bun(ctx: &Ctx) -> Result<(), String> {
    let found = ctx
        .host
        .child_path()
        .split(':')
        .any(|d| !d.is_empty() && Path::new(d).join("bun").is_file());
    if found {
        Ok(())
    } else {
        Err(
            "FATAL: `bun` is not on PATH (looked in $PATH and $HOME/.bun/bin).\n       \
             The boot gate cannot run, and a run without it is not gated.\n       \
             Install bun (https://bun.sh) into $HOME/.bun/bin."
                .into(),
        )
    }
}

/// One boot probe: the last line of its output, quotes stripped (the shell
/// gate's `2>&1 | tail -1` and `${v//\"/}`). stdout's last line when it
/// wrote one, else stderr's: an answer is printed on stdout, a failure on
/// stderr.
fn probe(ctx: &Ctx, tree: &Path, secs: u64, bun_args: &[&str]) -> String {
    let mut args = vec![secs.to_string(), "env".to_string()];
    for v in SCRUBBED {
        args.push("-u".into());
        args.push((*v).into());
    }
    args.push("bun".into());
    args.extend(bun_args.iter().map(|a| a.to_string()));
    let mut s = ctx.spawn("timeout", &args, Duration::from_secs(secs + 30));
    s.cwd = Some(tree.to_path_buf());
    s.env.push(("PWD".into(), text(tree)));
    let out = match crate::proc::run(&s) {
        Ok(done) => {
            let last = |b: &[u8]| {
                String::from_utf8_lossy(b)
                    .lines()
                    .last()
                    .unwrap_or("")
                    .to_string()
            };
            let o = last(&done.stdout);
            if o.is_empty() { last(&done.stderr) } else { o }
        }
        Err(e) => e,
    };
    out.replace('"', "")
}

/// The tree's `--version` line.
pub fn boot_version(ctx: &Ctx, tree: &Path) -> String {
    probe(ctx, tree, VERSION_SECS, &["run.cjs", "--version"])
}

/// The tree's answer to the live prompt, pinned to `model`; contains
/// `boot-ok` when the tree runs.
pub fn boot_prompt(ctx: &Ctx, tree: &Path, model: &str) -> String {
    probe(
        ctx,
        tree,
        PROMPT_SECS,
        &["run.cjs", "-p", "say exactly: boot-ok", "--model", model],
    )
}

/// Both halves of one tree's boot, judged.
pub struct Boot {
    pub version: String,
    pub prompt: String,
    pub version_ok: bool,
    pub prompt_ok: bool,
}

impl Boot {
    pub fn ok(&self) -> bool {
        self.version_ok && self.prompt_ok
    }
}

/// Boot `tree` (which must hold `run.cjs`) as version `version`.
pub fn boot(ctx: &Ctx, tree: &Path, version: &str, prompt_on: bool, model: &str) -> Boot {
    let v = boot_version(ctx, tree);
    let p = if prompt_on {
        boot_prompt(ctx, tree, model)
    } else {
        "skipped".to_string()
    };
    Boot {
        version_ok: v.contains(version),
        prompt_ok: !prompt_on || p.contains("boot-ok"),
        version: v,
        prompt: p,
    }
}

/// `boot-gate <tree> <version>`: both halves, printed; exit 0 when the
/// tree boots.
pub fn run_verb(ctx: &Ctx, tree: &Path, version: &str, model: &str) -> u8 {
    if let Err(e) = require_bun(ctx) {
        ctx.say(e);
        return 1;
    }
    let b = boot(ctx, tree, version, true, model);
    ctx.say(format!("version: {}", b.version));
    ctx.say(format!("prompt: {}", b.prompt));
    ctx.say(format!(
        "BOOT GATE: {} (--version {}, -p {}, model {model})",
        if b.ok() { "OK" } else { "FAILED" },
        b.version_ok,
        b.prompt_ok
    ));
    if b.ok() { 0 } else { 1 }
}
