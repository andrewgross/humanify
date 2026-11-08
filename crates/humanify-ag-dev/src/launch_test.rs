use super::*;

fn s(a: &[&str]) -> Vec<String> {
    a.iter().map(|x| x.to_string()).collect()
}

#[test]
fn settings_render_as_pipeline_config_ts_renders_them() {
    let text = render_config(&s(&[
        "llm.reasoning_effort=low",
        "llm.concurrency=472",
        "split.enabled=true",
        "diagnostics.log_file=/r/2.1.85-rebase.log",
        "library.index=/work/library-index/cb64",
        "library.lookup.agreement=false",
        "naming.schedule=sequential",
        "naming.batch_size=10",
        "x.list=[\"a\",\"b\"]",
        "x.quoted=\"q\"",
    ]))
    .unwrap();
    assert_eq!(
        text,
        "[llm]\nreasoning_effort = \"low\"\nconcurrency = 472\n[split]\nenabled = true\n\
         [diagnostics]\nlog_file = \"/r/2.1.85-rebase.log\"\n[library]\n\
         index = \"/work/library-index/cb64\"\n[library.lookup]\nagreement = false\n\
         [naming]\nschedule = \"sequential\"\nbatch_size = 10\n\
         [x]\nlist = [\"a\", \"b\"]\nquoted = \"q\"\n"
    );
}

#[test]
fn a_key_set_twice_or_a_malformed_entry_is_refused() {
    let e = render_config(&s(&["llm.concurrency=1", "llm.concurrency=2"])).unwrap_err();
    assert!(e.contains("set twice"), "{e}");
    assert!(render_config(&s(&["nodot=1"])).is_err());
    assert!(render_config(&s(&["Upper.key=1"])).is_err());
    assert!(render_config(&s(&["novalue"])).is_err());
}

#[test]
fn a_launch_argv_keeps_the_golden_order() {
    let a = HumanifyArgs {
        input: "in.js",
        config: "c.toml",
        endpoint: "http://e/v1",
        model: "m",
        out: "o",
        prior: Some("p.js"),
        verbose: true,
    };
    assert_eq!(
        a.argv().join(" "),
        "in.js --config c.toml --endpoint http://e/v1 --model m -o o --prior-version p.js -vv"
    );
}
