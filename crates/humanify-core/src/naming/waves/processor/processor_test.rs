//! The processor's pure helpers (processor.ts extractRetrySnippet,
//! buildRetryUsedNames) and the JS Set/Record order semantics they lean on.

use super::{build_retry_used_names, extract_retry_snippet};
use crate::naming::waves::jsset::{JsRecord, JsSet};

#[test]
fn short_code_is_sent_whole_on_retries() {
    let code = "function f(a) {\n  return a;\n}";
    assert_eq!(extract_retry_snippet(code, &["a".to_string()]), code);
}

#[test]
fn long_code_keeps_the_signature_and_referencing_lines_with_context() {
    let mut lines = vec!["function f(a) {".to_string()];
    for i in 0..40 {
        lines.push(format!("  x{i}();"));
    }
    lines[20] = "  use(Qr);".to_string();
    lines.push("}".to_string());
    let code = lines.join("\n");
    let out = extract_retry_snippet(&code, &["Qr".to_string()]);
    assert_eq!(
        out,
        "function f(a) {\n  // …\n  x17();\n  x18();\n  use(Qr);\n  x20();\n  x21();\n  // …"
    );
    // `$` is an identifier character: `$Qr` does not hold `Qr`.
    assert!(
        !extract_retry_snippet(&code.replace("use(Qr)", "use($Qr)"), &["Qr".to_string()])
            .contains("use(")
    );
}

#[test]
fn retry_used_names_lead_with_the_collided_suggestions_capped_at_25() {
    let mut prev = JsRecord::default();
    prev.set("a", "taken");
    prev.set("b", "taken");
    let windowed: Vec<String> = (0..40).map(|i| format!("n{i}")).collect();
    let out = build_retry_used_names(&windowed, &prev);
    assert_eq!(out[0], "taken");
    assert_eq!(out.len(), 25);
    assert_eq!(out[1], "n0");
}

#[test]
fn a_js_set_moves_a_renamed_member_to_the_end() {
    let mut s = JsSet::new();
    for n in ["a", "b", "c"] {
        s.add(n);
    }
    s.add("a");
    assert_eq!(s.to_vec(), ["a", "b", "c"]);
    s.delete("a");
    s.add("z");
    assert_eq!(s.to_vec(), ["b", "c", "z"]);
    let mut r = JsRecord::default();
    r.set("x", "1");
    r.set("y", "2");
    r.set("x", "3");
    assert_eq!(
        r.0,
        vec![("x".into(), "3".into()), ("y".into(), "2".into())]
    );
}
