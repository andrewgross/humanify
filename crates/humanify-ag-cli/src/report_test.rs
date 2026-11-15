use crate::report::report_runnable_declined;

#[test]
fn a_run_whose_runnable_emit_declined_is_marked_failed_with_one_error_line() {
    let r = report_runnable_declined(Some(
        "runnable emit: load-time reference cycle: src/a.js -> src/b.js -> src/a.js",
    ));
    assert!(r.fails_run, "a declined runnable emit fails the run");
    assert_eq!(
        r.messages,
        vec![
            "ERROR: no runnable tree was written (runnable emit: load-time reference cycle: \
src/a.js -> src/b.js -> src/a.js); only the byte-exact review tree was written, and this run \
is marked failed."
                .to_string()
        ]
    );
}

#[test]
fn a_run_with_a_runnable_tree_or_none_asked_reports_nothing() {
    let r = report_runnable_declined(None);
    assert!(!r.fails_run);
    assert!(r.messages.is_empty());
}
