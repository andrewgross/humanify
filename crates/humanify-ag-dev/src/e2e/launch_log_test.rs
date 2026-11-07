use std::time::Duration;

use humanify_ag_model::js::JsValue;

use super::*;
use crate::proc::{Spawn, run_ok};

#[test]
fn shell_quote_writes_each_argument_exactly_as_bash_printf_q() {
    let samples = [
        "",
        "plain",
        "/a/b-c_d.e/index.js",
        "--prior-version",
        "http://127.0.0.1:8000/v1",
        "a=b",
        "a,b",
        "~x",
        "x~",
        "#a",
        "a#",
        "a b",
        "it's",
        "q\"q",
        "a:b@c%d+e",
        "a^b",
        "x{y}",
        "$HOME",
        "a;b|c&d",
        "(x)<y>!*?[z]`w`\\",
        "é",
    ];
    let mut args = vec![
        "-c",
        "for a in \"$@\"; do printf \"%q\\n\" \"$a\"; done",
        "_",
    ];
    args.extend(samples);
    let bash = run_ok(&Spawn::new("bash", &args, Duration::from_secs(30)).env("LC_ALL", "C.UTF-8"))
        .expect("bash runs");
    let want: Vec<&str> = bash.split('\n').take(samples.len()).collect();
    let got: Vec<String> = samples.iter().map(|s| shell_quote(s).unwrap()).collect();
    assert_eq!(got, want);
}

#[test]
fn shell_quote_refuses_a_control_character() {
    assert!(
        shell_quote("a\nb")
            .unwrap_err()
            .contains("control character")
    );
}

#[test]
fn the_config_file_is_one_table_per_section_in_first_seen_order() {
    let text = render_config(&[
        setting("llm.retries", JsValue::Number(0.0)),
        setting("library.index_store_dir", JsValue::str("/r/store")),
        setting("split.enabled", JsValue::Bool(true)),
        setting("naming.schedule", JsValue::str("sequential")),
        setting("llm.concurrency", JsValue::Number(472.0)),
        setting("library.skip", JsValue::str_array(&["a", "b"])),
    ])
    .unwrap();
    assert_eq!(
        text,
        "[llm]\nretries = 0\nconcurrency = 472\n[library]\nindex_store_dir = \"/r/store\"\n\
         skip = [\"a\", \"b\"]\n[split]\nenabled = true\n[naming]\nschedule = \"sequential\"\n"
    );
}

#[test]
fn a_key_set_twice_is_refused() {
    let e = render_config(&[
        setting("split.enabled", JsValue::Bool(false)),
        setting("split.enabled", JsValue::Bool(true)),
    ])
    .unwrap_err();
    assert_eq!(e, "split.enabled is set twice (false and true)");
}

#[test]
fn a_launch_is_logged_in_the_recorder_shape_with_its_config_lines() {
    let dir = std::env::temp_dir().join(format!("dev-launch-log-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let config = dir.join("fresh.toml");
    write_config(
        &config,
        &[
            setting("llm.retries", JsValue::Number(0.0)),
            setting("split.enabled", JsValue::Bool(true)),
        ],
    )
    .unwrap();
    let mut log = LaunchLog::default();
    let argv: Vec<String> = ["in put.js", "--config", config.to_str().unwrap()]
        .iter()
        .map(|s| s.to_string())
        .collect();
    log.record("/bin/h", "e2e", &argv).unwrap();
    log.record("/bin/h", "", &["detect".to_string()]).unwrap();
    let d = dir.display().to_string();
    assert_eq!(
        log.normalized(&[(d, "<TMP>".into())]),
        "/bin/h HUMANIFY_API_KEY=e2e :: in\\ put.js --config <TMP>/fresh.toml \n\
         humanify-config| [llm]\nhumanify-config| retries = 0\nhumanify-config| [split]\n\
         humanify-config| enabled = true\n/bin/h HUMANIFY_API_KEY= :: detect \n"
    );
    assert_eq!(log.len(), 2);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_port_is_normalized_everywhere_and_only_in_an_endpoint() {
    assert_eq!(
        normalize_port(
            "a http://127.0.0.1:4242/v1 b http://127.0.0.1:99/v1\nhttp://127.0.0.1:x/v1"
        ),
        "a http://127.0.0.1:<PORT>/v1 b http://127.0.0.1:<PORT>/v1\nhttp://127.0.0.1:x/v1"
    );
}

#[test]
fn first_difference_names_the_line() {
    assert_eq!(
        first_difference("a\nb\n", "a\nc\n"),
        "line 2:\n  golden: c\n  now:    b"
    );
    assert_eq!(first_difference("a\n", "a\n"), "no line differs");
    assert_eq!(
        first_difference("a\n", "a\nb"),
        "line 2:\n  golden: b\n  now:    "
    );
}
