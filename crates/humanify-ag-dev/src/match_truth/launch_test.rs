//! match-truth's launches against lane L0's golden
//! (dev/eval/goldens/match-truth.golden.txt),
//! byte for byte. The golden was recorded from the TS runner (run.ts) with
//! recorders in place of npm and the binary (test/launch-recorder.ts); the
//! recorders here are the same two scripts, and the Rust runner is pointed
//! at the TS runner's default paths under the same scratch layout
//! (`<TMP>/cwd/experiments/092-match-ground-truth/{.corpus,.runs}`), so
//! every launched line must come out identical.

use std::path::{Path, PathBuf};

use crate::HostEnv;
use crate::committed::eval::launch_goldens;
use crate::paths::repo_root;
use crate::settings;

use super::ACCEPTED;

/// test/launch-recorder.ts's BIN_RECORDER, the parts match-truth reaches:
/// log cwd, the inherited API key and the argv; `format -o` writes a file,
/// `match -o` copies the scoreable fixture dump.
const BIN_RECORDER: &str = r#"#!/usr/bin/env bash
LOG="$RECORDER_ROOT/calls.log"
me="$0"
echo "$me (in $PWD) HUMANIFY_API_KEY=${HUMANIFY_API_KEY:-} :: $(printf '%q ' "$@")" >> "$LOG"
out="" prev=""
for a in "$@"; do
  [[ "$prev" == "-o" ]] && out="$a"
  prev="$a"
done
case "${1:-}" in
  format) [[ -n "$out" ]] && echo "formatted" > "$out"; exit 0 ;;
  match) [[ -n "$out" ]] && cp "$RECORDER_MATCH_DUMP" "$out"; exit 0 ;;
esac
exit 0
"#;

/// test/launch-goldens.test.ts's NPM_RECORDER, verbatim.
const NPM_RECORDER: &str = r#"#!/bin/bash
echo "npm (in $PWD) :: $*" >> "$RECORDER_ROOT/calls.log"
spec="$2" dest="$5"
file="$(cat "$RECORDER_ROOT/npm-files/$spec")"
t="$(mktemp -d)"; mkdir -p "$t/package/$(dirname "$file")"
echo "// $spec" > "$t/package/$file"
name="${spec%@*}"; ver="${spec##*@}"
tar -czf "$dest/$name-$ver.tgz" -C "$t" package; rm -rf "$t"
"#;

fn write_exec(path: &Path, body: &str) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

fn scratch_root() -> PathBuf {
    let root = std::env::temp_dir().join(format!("launch-matchtruth-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    root
}

#[test]
fn the_launches_are_l0s_golden_byte_for_byte() {
    let root = scratch_root();
    let cwd = root.join("cwd");
    let bin = cwd.join("target/release/humanify-ag");
    write_exec(&bin, BIN_RECORDER);
    let shims = root.join("shims");
    write_exec(&shims.join("npm"), NPM_RECORDER);
    std::fs::create_dir_all(root.join("npm-files")).unwrap();
    for e in super::corpus::CORPUS {
        for v in [e.old_version, e.new_version] {
            std::fs::write(
                root.join("npm-files").join(format!("{}@{v}", e.package)),
                e.file,
            )
            .unwrap();
        }
    }
    std::fs::write(root.join("calls.log"), "").unwrap();
    let exp = cwd.join("experiments/092-match-ground-truth");
    let args: Vec<String> = [
        "--corpus-dir",
        &exp.join(".corpus").display().to_string(),
        "--out",
        &exp.join(".runs").display().to_string(),
        "--bin",
        &bin.display().to_string(),
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    let parsed = settings::parse("match-truth", ACCEPTED, &[], &args).unwrap();
    // npm resolves to the recorder through PATH; the two variables the
    // recorders read go to every child (never set on the test process).
    let host = HostEnv {
        // The recorders need only bash and the base tools (tar, gzip,
        // mktemp, cat, dirname).
        path: Some(format!("{}:/usr/local/bin:/usr/bin:/bin", shims.display())),
        cwd: Some(cwd.clone()),
        ..HostEnv::default()
    };
    let extra = [
        ("RECORDER_ROOT".to_string(), root.display().to_string()),
        (
            "RECORDER_MATCH_DUMP".to_string(),
            repo_root()
                .join("crates/humanify-ag-dev/src/match_truth/fixtures/dump.json")
                .display()
                .to_string(),
        ),
    ];
    let (code, _out, err) = super::run_with_env(&parsed, &host, &extra);
    assert_eq!(code, 0, "{err}");
    let calls = std::fs::read_to_string(root.join("calls.log"))
        .unwrap()
        .replace(&root.display().to_string(), "<TMP>");
    let goldens = launch_goldens(&repo_root()).unwrap();
    let golden = &goldens["match-truth.golden.txt"].value;
    if calls != *golden {
        let first = calls.lines().zip(golden.lines()).position(|(a, b)| a != b);
        panic!(
            "the match-truth launches differ from L0's golden (first differing line {first:?}):\n{calls}"
        );
    }
    assert!(
        exp.join(".runs/scorecard.json").exists(),
        "the scorecard is written into --out"
    );
    std::fs::remove_dir_all(&root).unwrap();
}
