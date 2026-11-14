use super::{parse_entry_modules, read_prior_entry_order, stable_entry_order};

fn paths(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

#[test]
fn files_still_in_the_prior_keep_its_order() {
    let files = paths(&["a.js", "b.js", "c.js", "d.js"]);
    // bundle order moved d before b; the prior had b, c, d.
    let got = stable_entry_order(
        &[0, 3, 1, 2],
        &files,
        &paths(&["a.js", "b.js", "c.js", "d.js"]),
        &[false; 4],
    );
    assert_eq!(got, vec![0, 1, 2, 3]);
}

#[test]
fn a_new_file_follows_its_bundle_predecessor() {
    let files = paths(&["a.js", "b.js", "c.js", "new.js"]);
    // prior: c, a, b. Bundle: a, new, b, c — new follows a.
    let got = stable_entry_order(
        &[0, 3, 1, 2],
        &files,
        &paths(&["c.js", "a.js", "b.js"]),
        &[false; 4],
    );
    assert_eq!(got, vec![2, 0, 3, 1]);
}

#[test]
fn pinned_files_keep_their_bundle_order_relative_to_each_other() {
    let files = paths(&["a.js", "b.js", "c.js", "d.js"]);
    // prior reversed everything; a and c have load-time effects.
    let got = stable_entry_order(
        &[0, 1, 2, 3],
        &files,
        &paths(&["d.js", "c.js", "b.js", "a.js"]),
        &[true, false, true, false],
    );
    // Unconstrained b, d take prior order (d before b); the constrained
    // slots are filled a then c, in bundle order.
    let pos = |f: usize| got.iter().position(|&x| x == f).unwrap();
    assert!(pos(3) < pos(1), "{got:?}");
    assert!(pos(0) < pos(2), "{got:?}");
    let mut sorted = got.clone();
    sorted.sort_unstable();
    assert_eq!(sorted, vec![0, 1, 2, 3]);
}

#[test]
fn the_entry_parser_reads_only_an_emitted_entry() {
    let entry = "\"use strict\";\n// Entry for the runnable split tree: loads every module.\n\
                 const b = require(\"./.humanify/_bundle.js\");\n\
                 require(\"./src/x.js\");\nrequire(\"./vendor/p/y.js\");\n";
    assert_eq!(
        parse_entry_modules(entry),
        Some(paths(&["src/x.js", "vendor/p/y.js"]))
    );
    assert_eq!(parse_entry_modules("require(\"./src/x.js\");\n"), None);
}

#[test]
fn the_prior_entry_may_carry_an_underscore_prefix() {
    let read = |p: &str| match p {
        "index.js" => Some("module.exports = 1;\n".to_string()),
        "_index.js" => {
            Some("// Entry for the runnable split tree\nrequire(\"./index.js\");\n".to_string())
        }
        _ => None,
    };
    assert_eq!(read_prior_entry_order(read), Some(paths(&["index.js"])));
}

use super::{effect_run_order, pinned_entry_order};

#[test]
fn effect_run_order_follows_node_header_first_loading() {
    // 0 requires 2 in its header; 1 alone. Effects: 1 and 2.
    let headers = vec![vec![2], vec![], vec![]];
    let effect = [false, true, true];
    assert_eq!(
        effect_run_order(&[0, 1, 2], &headers, &effect),
        vec![(2, 0), (1, 1)]
    );
    assert_eq!(
        effect_run_order(&[1, 0, 2], &headers, &effect),
        vec![(1, 1), (2, 0)]
    );
}

#[test]
fn pinning_keeps_every_effect_in_bundle_order() {
    let files = paths(&["a.js", "b.js", "c.js", "d.js", "e.js"]);
    // c loads e in its header; b and e have effects. The prior reversed
    // everything: taken as is, e (through c) would run before b.
    let headers = vec![vec![], vec![], vec![4], vec![], vec![]];
    let effect = [false, true, false, false, true];
    let prior = paths(&["e.js", "d.js", "c.js", "b.js", "a.js"]);
    let bundle = [0, 1, 2, 3, 4];
    let got = pinned_entry_order(&bundle, &files, &prior, &headers, &effect).expect("an order");
    assert_eq!(
        effect_run_order(&got, &headers, &effect)
            .iter()
            .map(|(f, _)| *f)
            .collect::<Vec<_>>(),
        vec![1, 4],
        "{got:?}"
    );
    // the effect-free files that load nothing still take the prior order.
    let pos = |f: usize| got.iter().position(|&x| x == f).unwrap();
    assert!(pos(3) < pos(0), "{got:?}");
}

#[test]
fn without_effects_the_prior_order_is_taken_whole() {
    let files = paths(&["a.js", "b.js", "c.js"]);
    let headers = vec![vec![1], vec![], vec![]];
    let got = pinned_entry_order(
        &[0, 1, 2],
        &files,
        &paths(&["c.js", "b.js", "a.js"]),
        &headers,
        &[false; 3],
    );
    assert_eq!(got, Some(vec![2, 1, 0]));
    // the prior's own order is bundle order: nothing to adopt.
    assert_eq!(
        pinned_entry_order(&[0, 1, 2], &files, &files, &headers, &[false; 3]),
        None
    );
}
