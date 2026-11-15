use std::sync::{Arc, Mutex};

use super::Failures;
use crate::progress::LineRenderer;

#[test]
fn a_declined_runnable_emit_closes_the_run_with_exit_1_and_its_reason() {
    let printed = Arc::new(Mutex::new(String::new()));
    let sink = printed.clone();
    let mut renderer = LineRenderer::new(
        Box::new(move |s: &str| sink.lock().unwrap().push_str(s)),
        Box::new(|| 0),
    );
    let failures = Failures {
        runnable_declined: Some(
            "runnable emit: load-time reference cycle: src/a.js -> src/b.js -> src/a.js".into(),
        ),
        ..Failures::default()
    };
    assert_eq!(failures.close(&mut renderer), 1);
    let printed = printed.lock().unwrap().clone();
    let errors: Vec<&str> = printed
        .lines()
        .filter(|l| l.starts_with("ERROR:"))
        .collect();
    assert_eq!(errors.len(), 1, "one error line: {printed}");
    assert!(errors[0].contains("src/a.js -> src/b.js -> src/a.js"));
}

#[test]
fn a_clean_run_closes_with_exit_0() {
    let mut renderer = LineRenderer::new(Box::new(|_: &str| {}), Box::new(|| 0));
    assert_eq!(Failures::default().close(&mut renderer), 0);
}
