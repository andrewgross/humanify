//! The golden is vendor_precision.py's card for the same synthetic
//! diagnostics (`--truth none`), recorded from the Python before the port:
//! duplicate trail indexes, a moved module that both worlds see, a null
//! raw span, a scoped vendor folder and a file at the vendor root.

use super::*;

#[test]
fn the_card_is_vendor_precision_py_s() {
    let diag: Value =
        serde_json::from_str(include_str!("testdata/vendor-precision-diag.json")).unwrap();
    let card = check(&diag, None, None).unwrap();
    assert_eq!(
        card_text(&card),
        include_str!("testdata/vendor-precision-card.golden.txt")
    );
}

#[test]
fn a_run_without_the_bridge_says_so() {
    let diag: Value = serde_json::from_str(r#"{"placementTrails":{"trails":[]}}"#).unwrap();
    assert_eq!(
        card_text(&check(&diag, None, None).unwrap()),
        "{\n \"noBridge\": 1\n}\n"
    );
}

#[test]
fn vendor_package_reads_scopes_and_the_root() {
    assert_eq!(
        vendor_package("vendor/@s/p/x.js"),
        Some(Some("@s/p".into()))
    );
    assert_eq!(vendor_package("vendor/@s/x.js"), Some(Some("@s".into())));
    assert_eq!(vendor_package("vendor/x.js"), Some(None));
    assert_eq!(vendor_package("src/x.js"), None);
}
