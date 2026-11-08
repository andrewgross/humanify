//! The recorders the runners' launch tests run them against — the method
//! of `run-launch.test.ts` and `test/launch-recorder.ts` (exp191): run the
//! REAL runner end to end with a recorder in place of the binary (and of
//! cargo, npx, taskset, bun), normalise the run-specific paths, and compare
//! the log with the committed golden. The recorder scripts in
//! `recorders/` are those files' recorders byte for byte (the run.sh one
//! is held by its sha: the golden records the `--bin` file's sha256).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use super::Ctx;
use crate::env::HostEnv;

pub const RUN_RECORDER: &str = include_str!("recorders/run-launch.sh");
pub const BIN_RECORDER: &str = include_str!("recorders/bin.sh");
pub const CARGO_RECORDER: &str = include_str!("recorders/cargo.sh");

static SEQ: AtomicUsize = AtomicUsize::new(0);

/// One recording: a scratch root holding `calls.log`, the recorders, a
/// HOME and the sandbox.
pub struct Rec {
    pub root: PathBuf,
    pub shims: PathBuf,
    pub home: PathBuf,
}

/// Write an executable script. Written by a CHILD process, never through
/// a write fd of this one: a sibling test's fork would inherit that fd
/// until its exec, and executing the script then fails with ETXTBSY (the
/// flake lane L1's clean gate caught).
pub fn write_exec(path: &Path, body: &str) {
    std::fs::create_dir_all(path.parent().expect("a parent")).expect("mkdir");
    let mut s = crate::proc::Spawn::new(
        "sh",
        &[
            "-c",
            "cat > \"$1\" && chmod 755 \"$1\"",
            "_",
            &path.display().to_string(),
        ],
        std::time::Duration::from_secs(60),
    );
    s.stdin = Some(body.as_bytes().to_vec());
    crate::proc::run_ok(&s).unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
}

impl Rec {
    pub fn new(prefix: &str) -> Rec {
        let n = SEQ.fetch_add(1, Ordering::SeqCst);
        let root = std::env::temp_dir().join(format!("l3-{prefix}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let shims = root.join("shims");
        let home = root.join("home");
        std::fs::create_dir_all(&shims).expect("mkdir shims");
        std::fs::create_dir_all(home.join(".cargo/bin")).expect("mkdir home");
        write_exec(&root.join("bin-recorder"), BIN_RECORDER);
        write_exec(&home.join(".cargo/bin/cargo"), CARGO_RECORDER);
        std::fs::write(root.join("calls.log"), "").expect("calls.log");
        Rec { root, shims, home }
    }

    /// A context whose children find the recorders first and log here.
    pub fn ctx(&self, repo: &Path) -> Ctx {
        let host = HostEnv {
            home: Some(self.home.clone()),
            path: Some(format!("{}:/usr/bin:/bin", self.shims.display())),
            held_lock: true,
            ..Default::default()
        };
        let mut ctx = Ctx::new(repo.to_path_buf(), host, self.root.clone());
        ctx.echo = false;
        ctx.child_env
            .push(("RECORDER_ROOT".into(), self.root.display().to_string()));
        ctx.child_env.push(("NODE_OPTIONS".into(), String::new()));
        ctx
    }

    pub fn calls(&self) -> String {
        std::fs::read_to_string(self.root.join("calls.log")).unwrap_or_default()
    }

    /// The log with the run-specific paths replaced (`<REPO>` for the
    /// sandbox repo, `<TMP>` for the scratch root).
    pub fn normalise(&self, text: &str, repo: &Path) -> String {
        text.replace(&repo.display().to_string(), "<REPO>")
            .replace(&self.root.display().to_string(), "<TMP>")
    }
}

impl Drop for Rec {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn git(repo: &Path, args: &[&str]) {
    let env = [
        ("GIT_AUTHOR_NAME", "launch-recorder"),
        ("GIT_AUTHOR_EMAIL", "recorder@example.invalid"),
        ("GIT_COMMITTER_NAME", "launch-recorder"),
        ("GIT_COMMITTER_EMAIL", "recorder@example.invalid"),
        ("GIT_AUTHOR_DATE", "2026-11-06T00:00:00Z"),
        ("GIT_COMMITTER_DATE", "2026-11-06T00:00:00Z"),
    ];
    let mut s = crate::proc::Spawn::new("git", args, std::time::Duration::from_secs(60)).cwd(repo);
    for (k, v) in env {
        s = s.env(k, v);
    }
    crate::proc::run_ok(&s).unwrap_or_else(|e| panic!("git {args:?}: {e}"));
}

/// One commit of a sandbox repo: (repo-relative path, content) files; the
/// tree is REPLACED by each commit's files.
pub struct Commit<'a> {
    pub files: Vec<(&'a str, String)>,
    pub branch: Option<&'a str>,
}

/// A sandbox git repo under `<root>/repo`; the last commit is HEAD on main.
pub fn sandbox_repo(rec: &Rec, commits: &[Commit]) -> PathBuf {
    let repo = rec.root.join("repo");
    std::fs::create_dir_all(&repo).expect("mkdir repo");
    git(&repo, &["init", "-q", "-b", "main"]);
    std::fs::write(repo.join(".gitignore"), "node_modules\ntarget/\n").expect("gitignore");
    for c in commits {
        let tracked = crate::proc::git(&repo, &["ls-files"]).unwrap_or_default();
        for f in tracked.lines().filter(|f| *f != ".gitignore") {
            let _ = std::fs::remove_file(repo.join(f));
        }
        for (rel, body) in &c.files {
            let dest = repo.join(rel);
            std::fs::create_dir_all(dest.parent().expect("a parent")).expect("mkdir");
            std::fs::write(&dest, body).expect("write");
        }
        git(&repo, &["add", "-A"]);
        git(
            &repo,
            &[
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                c.branch.unwrap_or("head"),
            ],
        );
        if let Some(b) = c.branch {
            git(&repo, &["branch", b]);
        }
    }
    repo
}

/// The committed eval files a sandbox needs, read from this checkout.
pub fn committed(rel: &str) -> String {
    std::fs::read_to_string(crate::paths::repo_root().join(rel)).expect("a committed file")
}

/// A pipeline crate's Cargo.toml (L1's reader needs the package and the
/// `[[bin]]` at src/main.rs).
pub fn cargo_toml(package: &str, binary: &str) -> String {
    format!(
        "[package]\nname = \"{package}\"\n\n[[bin]]\nname = \"{binary}\"\npath = \"src/main.rs\"\n"
    )
}

/// A committed launch golden.
pub fn golden(name: &str) -> String {
    let dir = if name.starts_with("run-launch") {
        crate::paths::EVAL_DIR
    } else {
        crate::paths::LAUNCH_GOLDENS
    };
    committed(&format!("{dir}/{name}"))
}

/// Compare a recorded log with its golden; on a difference, name the
/// first differing line (and leave the recording beside the temp dir).
pub fn assert_golden(name: &str, recorded: &str) {
    let want = golden(name);
    if recorded == want {
        return;
    }
    let path = std::env::temp_dir().join(format!("l3-golden-{name}"));
    let _ = std::fs::write(&path, recorded);
    let (w, r): (Vec<&str>, Vec<&str>) = (want.lines().collect(), recorded.lines().collect());
    let i = w
        .iter()
        .zip(&r)
        .position(|(a, b)| a != b)
        .unwrap_or(w.len().min(r.len()));
    panic!(
        "the {name} launches changed (recording at {}): line {}:\n golden: {}\n    now: {}",
        path.display(),
        i + 1,
        w.get(i).unwrap_or(&"<end>"),
        r.get(i).unwrap_or(&"<end>")
    );
}
