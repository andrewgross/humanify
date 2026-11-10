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
    let err = bands(&root, &labels, &file, &root.join("self-hop-reference.json")).unwrap_err();
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
        None,
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

/// Copy frozen labels into a scratch results dir.
fn frozen(root: &Path, names: &[&str]) {
    for name in names {
        let src = Path::new("/work/dev-harness-proof/oracles/a").join(name);
        let dir = root.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        for e in std::fs::read_dir(&src).expect("the frozen reference cards") {
            let p = e.unwrap().path();
            std::fs::copy(&p, dir.join(p.file_name().unwrap())).unwrap();
        }
    }
}

#[test]
fn the_repeats_cold_self_hops_become_their_modes_reference_range() {
    let root = std::env::temp_dir().join(format!("dev-bands-selfhop-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let names = [
        "ref-names-0357696e-r1",
        "ref-names-0357696e-r2",
        "ref-names-0357696e-r3",
    ];
    frozen(&root, &names);
    // r2's cold self-hop re-rolled more: the range must come from the cards.
    let hop = root.join(names[1]).join("2.1.216-self-hop.json");
    let text = std::fs::read_to_string(&hop).unwrap();
    std::fs::write(&hop, text.replace("\"diffLines\":8,", "\"diffLines\":31,")).unwrap();
    let band_file = root.join("noise-bands.json");
    let hop_file = root.join("self-hop-reference.json");
    let committed = crate::paths::repo_root().join("dev/eval/self-hop-reference.json");
    std::fs::copy(&committed, &hop_file).unwrap();
    let labels: Vec<String> = names.iter().map(|s| s.to_string()).collect();
    let said = bands(&root, &labels, &band_file, &hop_file).unwrap();
    assert!(said.contains("self-hop"), "{said}");
    let r: crate::committed::eval::SelfHopReference =
        serde_json::from_str(&std::fs::read_to_string(&hop_file).unwrap()).unwrap();
    assert_eq!(r.range("names"), Some((8, 31)));
    let names_slot = &r.by_mode["names"];
    assert_eq!(names_slot.version, "2.1.216");
    assert_eq!(names_slot.cold_diff_lines.len(), 3);
    assert_eq!(names_slot.commits[names[0]], "0357696e");
    // The off range is kept.
    assert_eq!(r.range("off"), Some((2, 184)));
}

#[test]
fn a_repeat_without_a_cold_self_hop_leaves_the_reference_alone() {
    let root = std::env::temp_dir().join(format!("dev-bands-noselfhop-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let names = ["ref-names-0357696e-r1", "ref-names-0357696e-r2"];
    frozen(&root, &names);
    std::fs::remove_file(root.join(names[1]).join("2.1.216-self-hop.json")).unwrap();
    let hop_file = root.join("self-hop-reference.json");
    let committed = crate::paths::repo_root().join("dev/eval/self-hop-reference.json");
    std::fs::copy(&committed, &hop_file).unwrap();
    let labels: Vec<String> = names.iter().map(|s| s.to_string()).collect();
    let said = bands(&root, &labels, &root.join("noise-bands.json"), &hop_file).unwrap();
    assert!(said.contains("self-hop reference NOT written"), "{said}");
    assert_eq!(
        std::fs::read_to_string(&hop_file).unwrap(),
        std::fs::read_to_string(&committed).unwrap()
    );
}

/// `eval bands` over two frozen names-mode repeats in a scratch results
/// dir (`tag` names it), the committed bands file copied in, after
/// `setup` has touched the labels: the verdict and the bands file.
fn bands_over(
    tag: &str,
    setup: impl Fn(&Path, &[&str]),
) -> (Result<String, String>, std::path::PathBuf) {
    let root = std::env::temp_dir().join(format!("dev-bands-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let names = ["ref-names-0357696e-r1", "ref-names-0357696e-r2"];
    frozen(&root, &names);
    setup(&root, &names);
    let file = root.join("noise-bands.json");
    std::fs::copy(
        crate::paths::repo_root().join("dev/eval/noise-bands.json"),
        &file,
    )
    .unwrap();
    let labels: Vec<String> = names.iter().map(|s| s.to_string()).collect();
    let said = bands(&root, &labels, &file, &root.join("self-hop-reference.json"));
    (said, file)
}

fn read_bands(file: &Path) -> crate::committed::eval::NoiseBands {
    serde_json::from_str(&std::fs::read_to_string(file).unwrap()).unwrap()
}

use crate::eval::test_support::set_matching;

#[test]
fn the_repeats_matching_spreads_are_written_beside_the_kpi_bands() {
    let (said, file) = bands_over("match", |root, names| {
        set_matching(&root.join(names[0]), 10.0, 4.0);
        set_matching(&root.join(names[1]), 10.0, 6.0);
    });
    assert!(said.unwrap().contains("matching"));
    let b = read_bands(&file);
    let m = b.matching_bands(1, "names").unwrap();
    // Four pairs: mbLLMRenamed 16 vs 24; fnAsked equal.
    assert_eq!(m.get("mbLLMRenamed"), Some(&8.0));
    assert_eq!(m.get("fnAsked"), Some(&0.0));
}

#[test]
fn an_exact_matching_count_that_differs_refuses_the_bands() {
    let (said, file) = bands_over("match-ne", |root, names| {
        set_matching(&root.join(names[0]), 10.0, 4.0);
        set_matching(&root.join(names[1]), 11.0, 4.0);
    });
    let err = said.unwrap_err();
    assert!(err.contains("MATCHING NOT EQUAL: fnAsked"), "{err}");
    // The copied-in committed file is untouched.
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        std::fs::read_to_string(crate::paths::repo_root().join("dev/eval/noise-bands.json"))
            .unwrap()
    );
}

#[test]
fn repeats_without_matching_counts_still_get_kpi_bands_and_say_so() {
    let (said, file) = bands_over("match-none", |_, _| {});
    let said = said.unwrap();
    assert!(said.contains("matching counts NOT measured"), "{said}");
    assert!(read_bands(&file).matching_bands(1, "names").is_none());
}
