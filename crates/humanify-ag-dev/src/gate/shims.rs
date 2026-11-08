//! `cargo dev shims`: the JavaScript that remains after the harness moved
//! to Rust parses. What remains is SHIMS only — code that runs a tool and
//! holds no harness logic (docs/dev-harness.md §2.4): the webcrack shim
//! the pipeline runs for webpack / browserify unpacking (Andrew: kept).
//! The successor of the TS gate's typecheck / lint / knip stages, whose
//! subject (the harness TypeScript) is ported. `node --check` parses each
//! file (node strips TypeScript types itself); behaviour is the e2e
//! stage's to prove.

use std::path::Path;
use std::time::Duration;

use crate::HostEnv;
use crate::proc::{self, Spawn};
use crate::settings::Parsed;

/// Every shim file, relative to the repository root.
pub const SHIMS: &[&str] = &["tools/webcrack-shim.ts"];

const DEADLINE: Duration = Duration::from_secs(60);

/// The parse probe, run as `node -e PROBE <file>`.
const PROBE: &str = "const m = require('node:module'), fs = require('node:fs'), vm = require('node:vm'); \
const f = process.argv[1]; \
new vm.SourceTextModule(m.stripTypeScriptTypes(fs.readFileSync(f, 'utf8')), { identifier: f });";

/// Check `shims` under `root`: each exists and parses.
pub fn check(root: &Path, shims: &[&str], host: &HostEnv) -> (u8, String, String) {
    let mut out = String::new();
    let mut err = String::new();
    for shim in shims {
        let file = root.join(shim);
        if !file.is_file() {
            err.push_str(&format!(
                "{shim}: missing (delete it from SHIMS, or restore it)\n"
            ));
            continue;
        }
        let file = file.display().to_string();
        let args = [
            "--no-warnings",
            "--experimental-vm-modules",
            "-e",
            PROBE,
            &file,
        ];
        let mut spawn = Spawn::new("node", &args, DEADLINE);
        if host.path.is_some() {
            spawn = spawn.env("PATH", &host.child_path());
        }
        match proc::run(&spawn) {
            Ok(done) if done.status.success() => out.push_str(&format!("  ok  {shim}\n")),
            Ok(done) => err.push_str(&format!(
                "{shim}: does not parse\n{}",
                String::from_utf8_lossy(&done.stderr)
            )),
            Err(e) => err.push_str(&format!("{shim}: {e}\n")),
        }
    }
    (u8::from(!err.is_empty()), out, err)
}

/// Check every shim of this repository.
pub fn run(_p: &Parsed, host: &HostEnv) -> (u8, String, String) {
    check(&crate::paths::repo_root(), SHIMS, host)
}

#[cfg(test)]
#[path = "shims_test.rs"]
mod shims_test;
