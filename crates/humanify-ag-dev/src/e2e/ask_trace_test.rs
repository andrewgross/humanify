//! ask-trace (ported from test/ask-trace-harness.test.ts and the ask-trace
//! half of test/launch-goldens.test.ts): its argument reading, its summary,
//! and its launches, which must equal lane L0's recorded golden byte for
//! byte.

use humanify_ag_model::js::{JsObject, JsValue};

use super::*;
use crate::e2e::launch_log::normalize_port;
use crate::settings;

/// The settings `ask-trace` reads (its verb-table entry).
const ACCEPTED: &[&str] = &[
    "run.out",
    "dev.prior",
    "dev.collide",
    "dev.sequential",
    "run.pipeline_set",
];

fn parsed(args: &[&str]) -> settings::Parsed {
    let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    settings::parse("ask-trace", ACCEPTED, &[], &args).expect("the arguments parse")
}

#[test]
fn its_own_flags_the_pipeline_sets_and_the_passthrough_are_read() {
    let a = args_of(
        &parsed(&[
            "in.js",
            "--out",
            "o",
            "--collide",
            "boom",
            "--sequential",
            "--pipeline-set",
            "split.enabled=true",
            "--pipeline-set",
            "naming.batch_size=10",
            "--",
            "--model",
            "m",
        ]),
        Path::new("/cwd"),
    )
    .unwrap();
    assert_eq!(a.input, PathBuf::from("/cwd/in.js"));
    assert_eq!(a.out, Some(PathBuf::from("/cwd/o")));
    assert_eq!(a.collide.as_deref(), Some("boom"));
    assert!(a.sequential);
    assert_eq!(
        a.settings,
        vec![
            ("split.enabled".to_string(), JsValue::Bool(true)),
            ("naming.batch_size".to_string(), JsValue::Number(10.0)),
        ]
    );
    assert_eq!(a.passthrough, vec!["--model", "m"]);
    let bare = args_of(&parsed(&["/abs/in.js"]), Path::new("/cwd")).unwrap();
    assert_eq!(bare.input, PathBuf::from("/abs/in.js"));
    assert_eq!(bare.out, None);
    assert!(bare.passthrough.is_empty());
}

#[test]
fn no_input_is_refused_with_the_usage() {
    assert!(
        args_of(&parsed(&[]), Path::new("/cwd"))
            .unwrap_err()
            .contains("usage:")
    );
}

fn row(reason: &str, is_retry: bool) -> JsObject {
    let mut o = JsObject::new();
    o.insert("reason", JsValue::str(reason));
    o.insert("isRetry", JsValue::Bool(is_retry));
    o
}

#[test]
fn the_summary_is_totals_first_with_a_per_reason_table() {
    let text = summarize(&[
        row("retry", true),
        row("retry", true),
        row("fresh", false),
        row("module-lane", false),
    ]);
    assert_eq!(
        text,
        "TOTAL 4 ask(s)\n  fresh            1\n  module-lane      1\n  retry            2\n  \
         REMAINING        2 re-ask(s)"
    );
}

/// test/launch-recorder.ts's BIN_RECORDER, its log path written in (the
/// TS read it from $RECORDER_ROOT): logs argv through `printf %q` and the
/// `--config` file's lines, and writes one ask row where `dump_asks` says.
fn recorder(log: &Path) -> String {
    format!(
        r#"#!/usr/bin/env bash
LOG="{}"
me="$0"
echo "$me (in $PWD) HUMANIFY_API_KEY=${{HUMANIFY_API_KEY:-}} :: $(printf '%q ' "$@")" >> "$LOG"
cfg="" out="" prev=""
for a in "$@"; do
  [[ "$prev" == "--config" ]] && cfg="$a"
  [[ "$prev" == "-o" ]] && out="$a"
  prev="$a"
done
if [[ -n "$cfg" && -f "$cfg" ]]; then
  sed 's/^/humanify-config| /; s/ $//' "$cfg" >> "$LOG"
  asks="$(sed -n 's/^dump_asks = "\(.*\)"$/\1/p' "$cfg")"
  [[ -n "$asks" ]] && echo '{{"seq":0,"reason":"fresh","isRetry":false}}' > "$asks"
fi
if [[ -n "$out" ]]; then
  mkdir -p "$out/.humanify"
  echo "bundle" > "$out/.humanify/humanified.js"
fi
exit 0
"#,
        log.display()
    )
}

fn write_exec(file: &Path, body: &str) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, body).unwrap();
    std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
fn the_launches_equal_the_recorded_golden() {
    let root = std::env::temp_dir().join(format!("dev-launch-asktrace-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let root = root.canonicalize().unwrap();
    let repo = root.join("repo");
    let log = root.join("calls.log");
    std::fs::write(&log, "").unwrap();
    write_exec(&repo.join("target/release/humanify-ag"), &recorder(&log));
    std::fs::write(root.join("input.js"), "input\n").unwrap();
    std::fs::write(root.join("prior.js"), "prior\n").unwrap();
    let r = |p: &str| root.join(p).display().to_string();
    let legs: Vec<Vec<String>> = vec![
        vec![r("input.js"), "--out".into(), r("out-default")],
        vec![
            r("input.js"),
            "--out".into(),
            r("out-prior"),
            "--prior".into(),
            r("prior.js"),
            "--sequential".into(),
            "--collide".into(),
            "takenName".into(),
            "--pipeline-set".into(),
            "naming.batch_size=10".into(),
            "--".into(),
            "--model".into(),
            "m".into(),
        ],
        vec![
            r("input.js"),
            "--out".into(),
            r("out-split"),
            "--pipeline-set".into(),
            "split.enabled=true".into(),
        ],
    ];
    for leg in &legs {
        let p = settings::parse("ask-trace", ACCEPTED, &[], leg).unwrap();
        let args = args_of(&p, &repo).unwrap();
        let (code, stdout, stderr) = trace(&repo, &args);
        assert_eq!(code, 0, "{stdout}{stderr}");
        assert!(stdout.starts_with("TOTAL 1 ask(s)\n"), "{stdout}");
    }
    let recorded = normalize_port(&std::fs::read_to_string(&log).unwrap())
        .replace(&repo.display().to_string(), "<REPO>")
        .replace(&root.display().to_string(), "<TMP>");
    let golden = std::fs::read_to_string(
        crate::paths::repo_root()
            .join(crate::paths::LAUNCH_GOLDENS)
            .join("ask-trace.golden.txt"),
    )
    .unwrap();
    assert_eq!(
        recorded,
        golden,
        "{}",
        crate::e2e::launch_log::first_difference(&recorded, &golden)
    );
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn a_missing_binary_names_the_build_command() {
    let repo = std::env::temp_dir().join(format!("dev-asktrace-nobin-{}", std::process::id()));
    let args = args_of(&parsed(&["/in.js"]), Path::new("/")).unwrap();
    let (code, _, stderr) = trace(&repo, &args);
    assert_eq!(code, 1);
    assert!(stderr.contains("cargo build --release --locked -p humanify-ag-cli"));
}
