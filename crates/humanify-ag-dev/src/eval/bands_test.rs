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

#[test]
fn repeats_over_different_pair_sets_are_refused() {
    let root = std::env::temp_dir().join(format!("dev-bands-5-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let src = Path::new("/work/dev-harness-proof/oracles/a/ref-scratch-0f338ffa-r1");
    for name in ["r1", "r2"] {
        let dir = root.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        for e in std::fs::read_dir(src).expect("the frozen reference cards") {
            let p = e.unwrap().path();
            std::fs::copy(&p, dir.join(p.file_name().unwrap())).unwrap();
        }
    }
    // r2 lacks a pair r1 scored.
    for f in [
        "2.1.198.json",
        "2.1.198-run.json",
        "2.1.198-run-status.json",
    ] {
        std::fs::remove_file(root.join("r2").join(f)).unwrap();
    }
    let file = root.join("noise-bands.json");
    let labels = vec!["r1".to_string(), "r2".to_string()];
    let err = bands(&root, &labels, &file).unwrap_err();
    assert!(err.contains("different pairs"), "{err}");
    assert!(!file.exists(), "nothing written");
}

#[test]
fn a_measured_slot_records_the_pairs_it_was_measured_over() {
    let s = section(
        "abc",
        &["r1".to_string(), "r2".to_string()],
        &["2.1.85->2.1.86".to_string()],
        &[("novel".to_string(), Some(0.0))],
    );
    let pairs = match super::super::card::at(&s, "provenance.pairs") {
        Some(JsValue::Array(a)) => a
            .iter()
            .filter_map(|x| x.as_str().map(str::to_string))
            .collect::<Vec<_>>(),
        other => panic!("provenance.pairs: {other:?}"),
    };
    assert_eq!(pairs, ["2.1.85->2.1.86"]);
}
