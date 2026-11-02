//! The API key has ONE source: `HUMANIFY_API_KEY` in the process
//! environment (exp179, Andrew 2026-11-01). A `.env` file in the working
//! directory is NOT read — it was, through `dotenvy`, until exp179 — so a
//! key that lives only there is a missing key, refused before any work.

use std::path::PathBuf;
use std::process::Command;

struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn a_dot_env_file_is_not_a_key_source() {
    let dir = std::env::temp_dir().join(format!("humanify-dotenv-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let s = Scratch(dir);
    std::fs::write(s.0.join("bundle.js"), "var a = 1;\n").unwrap();
    std::fs::write(s.0.join(".env"), "HUMANIFY_API_KEY=from-dot-env\n").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_humanify"))
        .current_dir(&s.0)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .args(["bundle.js", "--library-matching", "off", "-o", "out"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{stderr}");
    assert!(
        stderr.contains("API key required: set HUMANIFY_API_KEY in the environment"),
        "{stderr}"
    );
    assert!(
        !stderr.contains(".env"),
        "no .env is offered as a source: {stderr}"
    );
    assert!(!s.0.join("out").exists(), "nothing written");
}
