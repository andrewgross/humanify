use super::*;

#[test]
fn a_band_is_the_largest_pairwise_disagreement() {
    let t = |novel: f64, noise: f64| {
        JsValue::parse(&format!(
            r#"{{"novel":{novel},"unchangedChurned":{noise}}}"#
        ))
        .unwrap()
    };
    let b = compute_bands(&[t(10.0, 5.0), t(10.0, 9.0), t(10.0, 6.0)]);
    let get = |k: &str| b.iter().find(|(x, _)| x == k).unwrap().1;
    assert_eq!(get("novel"), Some(0.0));
    assert_eq!(get("noise"), Some(4.0));
    assert_eq!(get("treeLn"), None);
}

#[test]
fn slots_keep_every_other_scorer_and_mode() {
    let mut file = match JsValue::parse(r#"{"provenance":{"commit":"old"},"bands":{"noise":17},"byMode":{"names":{"bands":{"noise":397}}}}"#).unwrap() {
        JsValue::Object(o) => o,
        _ => unreachable!(),
    };
    let slot = |n: f64| {
        obj(vec![
            ("provenance", obj(vec![("commit", text("new"))])),
            ("bands", obj(vec![("noise", JsValue::Number(n))])),
        ])
    };
    write_slot(&mut file, 2, "names", slot(300.0));
    write_slot(&mut file, 2, "off", slot(20.0));
    let v = JsValue::Object(file);
    let n = |p: &str| crate::eval::card::number_at(&v, p);
    assert_eq!(n("bands.noise"), Some(17.0));
    assert_eq!(n("byMode.names.bands.noise"), Some(397.0));
    assert_eq!(n("byScorer.2.bands.noise"), Some(20.0));
    assert_eq!(n("byScorer.2.byMode.names.bands.noise"), Some(300.0));
    // The loader reads what the writer wrote.
}
