//! `run stub`: the config is stub-run.mts's byte for byte, and the binary
//! is launched against the one stub with every ask answered.

use std::path::Path;

use super::stub::{config_text, run};
use crate::runners::recording_test::{Rec, write_exec};

#[test]
fn the_config_is_stub_run_mts_s() {
    let names = config_text(Path::new("/o"), Some("/ix"));
    assert_eq!(
        names,
        "[llm]\nretries = 0\n[split]\nenabled = true\n[diagnostics]\nlog_file = \"/o/run.log\"\ndiagnostics_file = \"/o/diag.json\"\n[library]\nmatching = \"names\"\nindex = \"/ix\"\n[library.lookup]\nagreement = false\n"
    );
    assert!(!config_text(Path::new("/o"), None).contains("[library]"));
}

#[test]
fn the_binary_asks_the_stub_and_gets_every_name_renamed() {
    let rec = Rec::new("stub");
    // a "binary" that asks the endpoint it was handed once, like the pipeline
    let bin = rec.root.join("fake-bin");
    write_exec(
        &bin,
        "#!/usr/bin/env bash\nep=\"\"; prev=\"\"; for a in \"$@\"; do [[ \"$prev\" == --endpoint ]] && ep=\"$a\"; prev=\"$a\"; done\n\
         curl -s -X POST \"$ep/chat/completions\" -H 'content-type: application/json' \
         -d '{\"messages\":[{\"role\":\"user\",\"content\":\"Rename: a\"}]}' > \"$RECORDER_ROOT/answer\"\necho \"$HUMANIFY_API_KEY\" > \"$RECORDER_ROOT/key\"\nexit 0\n",
    );
    let bundle = rec.root.join("in.js");
    std::fs::write(&bundle, "a\n").unwrap();
    let ctx = rec.ctx(&rec.root);
    let out = rec.root.join("out");
    assert_eq!(
        run(&ctx, &bin, &bundle, &out, None, Some("/ix")).unwrap(),
        0
    );
    assert!(out.join("run.toml").is_file());
    let answer = std::fs::read_to_string(rec.root.join("answer")).unwrap();
    assert!(answer.contains("\"chat.completion\""), "{answer}");
    assert_eq!(
        std::fs::read_to_string(rec.root.join("key"))
            .unwrap()
            .trim(),
        "exp166"
    );
}
