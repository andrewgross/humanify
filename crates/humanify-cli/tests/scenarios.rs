//! WPB.4 gate part 2, replayed without Node: every scenario the TS binary
//! was recorded on (test/parity/wpb4-scenarios.json, recorded by
//! test/parity/wpb4-scenarios.mjs --record) runs through the built
//! `humanify` binary in a fresh directory with a minimal environment; exit
//! code, stdout, stderr (the TS crash dump already reduced to its headline
//! in the fixture) and the files left behind must match.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

fn fixture() -> Vec<Value> {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../test/parity/wpb4-scenarios.json"
    );
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn list_files(root: &Path) -> Vec<String> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) {
        let mut entries: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap())
            .collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            let p = e.path();
            let rel = p.strip_prefix(root).unwrap().to_string_lossy().into_owned();
            if e.file_type().unwrap().is_dir() {
                out.push(format!("{rel}/"));
                walk(root, &p, out);
            } else {
                out.push(rel);
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out
}

fn strs(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|a| a.iter().map(|x| x.as_str().unwrap().to_string()).collect())
        .unwrap_or_default()
}

fn last_error_line(s: &str) -> Option<&str> {
    s.lines().rfind(|l| l.starts_with("Error: "))
}

#[test]
fn every_recorded_scenario_matches_the_ts_binary() {
    let scenarios = fixture();
    let bin = PathBuf::from(env!("CARGO_BIN_EXE_humanify"));
    let mut failures = Vec::new();
    for (i, s) in scenarios.iter().enumerate() {
        let name = s["name"].as_str().unwrap();
        let dir = std::env::temp_dir().join(format!("wpb4-scn-{}-{i}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        if let Some(files) = s["files"].as_object() {
            for (rel, content) in files {
                let p = dir.join(rel);
                std::fs::create_dir_all(p.parent().unwrap()).unwrap();
                std::fs::write(p, content.as_str().unwrap()).unwrap();
            }
        }
        let before = list_files(&dir);
        let mut cmd = Command::new(&bin);
        cmd.current_dir(&dir)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .args(strs(&s["argv"]))
            .args(strs(&s["rustExtraArgs"]));
        if let Some(env) = s["env"].as_object() {
            for (k, v) in env {
                cmd.env(k, v.as_str().unwrap());
            }
        }
        let out = cmd.output().unwrap();
        let created: Vec<String> = list_files(&dir)
            .into_iter()
            .filter(|f| !before.contains(f))
            .collect();
        let _ = std::fs::remove_dir_all(&dir);
        let ts = &s["ts"];
        let code = out.status.code();
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        let ts_code = ts["exitCode"].as_i64().map(|c| c as i32);
        let ts_stderr = ts["stderr"].as_str().unwrap();
        let same = if s["compare"] == "headline" {
            code == ts_code && last_error_line(&stderr) == last_error_line(ts_stderr)
        } else {
            code == ts_code
                && stdout == ts["stdout"].as_str().unwrap()
                && stderr == ts_stderr
                && created == strs(&ts["created"])
        };
        if !same {
            failures.push(format!(
                "{name}: exit rust {code:?} ts {ts_code:?}\n  rust stderr {stderr:?}\n  ts   stderr {ts_stderr:?}\n  rust created {created:?}"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    eprintln!("scenarios: {} identical to the TS binary", scenarios.len());
}
