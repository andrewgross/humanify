//! The kill-switch/env guard tests (WP1.1's exit gate; TS analog:
//! src/kill-switches.test.ts:112-138). The env guard is the port of the
//! "no process.env outside the allowlist" rule; in Rust the rule is "no
//! std::env outside the ONE env module" (02 §2).

use std::path::Path;

/// Walk every crate source and fail on `std::env` outside
/// crates/humanify-cli/src/env.rs.
#[test]
fn std_env_confined_to_the_env_module() {
    let crates_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut offenders: Vec<String> = Vec::new();
    let mut examined = 0usize;
    walk(&crates_dir, &mut |path, src| {
        examined += 1;
        // Only src/ trees are guarded: tests/ may read env for scratch
        // paths, and the guard's own matcher lines must not self-report.
        let s = path.to_string_lossy();
        let guarded = s.contains("/src/") && !s.contains("/tests/");
        let is_env_module = path.ends_with("humanify-cli/src/env.rs");
        for (line_no, line) in src.lines().enumerate() {
            let trimmed = line.trim_start();
            let is_comment = trimmed.starts_with("//") || trimmed.starts_with("//!");
            // The benign call: a scratch-directory path convention, not an
            // ambient-config read (no decision input; 02 §2's hazard class
            // is environment VARIABLES).
            let benign = line.contains("std::env::temp_dir");
            if guarded
                && !is_env_module
                && !is_comment
                && !benign
                && (line.contains("std::env::") || line.contains("env::var("))
            {
                offenders.push(format!(
                    "{}:{}: {}",
                    path.display(),
                    line_no + 1,
                    line.trim()
                ));
            }
        }
    });
    assert!(
        examined > 0,
        "the guard examined nothing — fixture gave it no sources"
    );
    assert!(
        offenders.is_empty(),
        "std::env escapes the env module — every env read belongs to \
         crates/humanify-cli/src/env.rs (02 §2):\n{}",
        offenders.join("\n")
    );
}

fn walk(dir: &Path, f: &mut impl FnMut(&Path, &str)) {
    let mut entries: Vec<std::fs::DirEntry> = match std::fs::read_dir(dir) {
        Ok(rd) => rd.filter_map(|e| e.ok()).collect(),
        Err(_) => return,
    };
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = dir.join(entry.file_name());
        if entry.file_name() == "target" {
            continue;
        }
        if path.is_dir() {
            walk(&path, f);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            let src = std::fs::read_to_string(&path).unwrap_or_default();
            f(&path, &src);
        }
    }
}
