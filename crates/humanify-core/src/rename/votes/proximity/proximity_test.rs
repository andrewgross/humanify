//! TS original: `src/rename/proximity.test.ts` (its five cases), plus the
//! inherited-record case the TS carries by accident, plus the
//! extract-then-parallel snapshot's byte-identity pins (perf-inventory
//! item 2: the module lanes' per-group windowing).

use std::collections::HashMap;

use super::{ProximityBinding, ProximityWindow, get_proximate_used_names};

fn binding(line: u32, refs: &[u32]) -> ProximityBinding {
    ProximityBinding {
        decl_line: Some(line),
        ref_lines: refs.to_vec(),
    }
}

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

fn run(
    all: &[&str],
    lines: &[u32],
    bindings: &HashMap<&str, ProximityBinding>,
    total: usize,
    eligible: impl Fn(&str) -> bool,
) -> Vec<String> {
    get_proximate_used_names(
        &names(all),
        lines,
        |n| bindings.get(n).cloned(),
        total,
        eligible,
    )
}

#[test]
fn always_includes_well_known_names() {
    let b = HashMap::from([
        ("exports", binding(1, &[])),
        ("require", binding(2, &[])),
        ("console", binding(3, &[])),
        ("a", binding(1000, &[])),
        ("b", binding(1001, &[])),
    ]);
    let out = run(
        &["exports", "require", "console", "a", "b"],
        &[50],
        &b,
        200,
        |_| true,
    );
    for n in ["exports", "require", "console"] {
        assert!(out.iter().any(|x| x == n), "{n}");
    }
}

#[test]
fn excludes_eligible_names() {
    let b = HashMap::from([
        ("a", binding(50, &[])),
        ("b", binding(50, &[])),
        ("c", binding(50, &[])),
        ("myVar", binding(50, &[])),
    ]);
    let out = run(&["a", "b", "c", "myVar"], &[50], &b, 200, |n| n.len() == 1);
    assert_eq!(out, names(&["myVar"]));
}

#[test]
fn includes_names_within_the_radius_only() {
    let b = HashMap::from([("nearVar", binding(55, &[])), ("farVar", binding(500, &[]))]);
    let out = run(&["nearVar", "farVar"], &[50], &b, 200, |n| n.len() == 1);
    assert_eq!(out, names(&["nearVar"]));
}

#[test]
fn includes_a_name_whose_reference_is_near() {
    let b = HashMap::from([("refVar", binding(500, &[45]))]);
    let out = run(&["refVar"], &[50], &b, 200, |n| n.len() == 1);
    assert_eq!(out, names(&["refVar"]));
}

#[test]
fn returns_every_preserved_name_below_the_threshold() {
    let b = HashMap::from([
        ("nearVar", binding(50, &[])),
        ("farVar", binding(500, &[])),
        ("a", binding(50, &[])),
    ]);
    let out = run(&["nearVar", "farVar", "a"], &[50], &b, 50, |n| n.len() == 1);
    assert_eq!(out, names(&["nearVar", "farVar"]));
}

#[test]
fn an_absent_binding_is_included_even_when_named_after_object_prototype() {
    // 16-findings-queue #22, fixed TS-first: `ownEntry(scopeBindings, name)`
    // — an absent `toString` is absent like any other name and is included
    // "to be safe" (the TS used to read the inherited built-in and exclude
    // it).
    let b: HashMap<&str, ProximityBinding> = HashMap::new();
    let out = run(&["missingVar", "toString"], &[50], &b, 200, |_| false);
    assert_eq!(out, names(&["missingVar", "toString"]));
}

// ---------------------------------------------------------------------
// the extract-then-parallel snapshot (perf-inventory item 2)
//
// The module lanes used to window per group by calling
// `get_proximate_used_names` over the SAME ~25k-name used list, taken
// set and scope bindings once per group (73.9 s of a 122.6 s fresh-run
// round setup). The snapshot extracts those inputs once per wave step,
// on the calling thread (the oxc-side state it reads is not `Sync`), and
// the per-group windowing fans out over plain data. Byte identity is
// pinned here three ways: against the LIVE serial reference
// (`get_proximate_used_names` — the exact function the serial path
// called), against a hardcoded fixture captured from that serial path,
// and for the parallel rejoin order.
// ---------------------------------------------------------------------

/// The serial path the module lanes ran per group, verbatim.
fn serial_windowed(
    all: &[&str],
    lines: &[u32],
    bindings: &HashMap<&str, ProximityBinding>,
    total: usize,
    droppable: impl Fn(&str) -> bool + Clone,
) -> Vec<String> {
    get_proximate_used_names(
        &names(all),
        lines,
        |n| bindings.get(n).cloned(),
        total,
        droppable,
    )
}

/// The snapshot the parallel path reads, built over the same world.
fn snapshot_over(
    all: &[&str],
    bindings: &HashMap<&str, ProximityBinding>,
    total: usize,
    droppable: impl Fn(&str) -> bool + Clone,
) -> ProximityWindow {
    ProximityWindow::new(names(all), total, droppable, |n| bindings.get(n).cloned())
}

#[test]
fn the_snapshot_windows_byte_identically_to_the_serial_path() {
    // A scope over the windowing threshold with every shape the serial
    // loop distinguishes: well-known names, an in-window decl, an
    // in-window REFERENCE, out-of-window names, absent bindings,
    // droppables (eligible + not taken), and a TAKEN eligible name (a
    // collision-fix name: never droppable).
    let bindings = HashMap::from([
        ("console", binding(4, &[])),
        ("inDecl", binding(150, &[])),
        ("inRef", binding(9000, &[180])),
        ("farAway", binding(9500, &[])),
        ("takenElig", binding(10000, &[])),
    ]);
    let all = [
        "console",
        "inDecl",
        "inRef",
        "farAway",
        "absentVar",
        "a",
        "takenElig",
    ];
    let total = 200;
    // `is_droppable`: eligible (single-letter) and not taken — the
    // module lane's rule (`is_eligible(n) && !taken.contains(n)`).
    let droppable = |n: &str| n.len() == 1 && n != "takenElig";
    let w = snapshot_over(&all, &bindings, total, droppable);
    for lines in [
        vec![120],        // one batch line: radius 20..220
        vec![120, 900],   // a span: radius 20..1000
        vec![9400],       // only farAway's window
        vec![],           // Math.min(...[]) = Infinity: nothing windows in
        vec![0, 4000000], // a window covering everything
    ] {
        assert_eq!(
            w.windowed(&lines),
            serial_windowed(&all, &lines, &bindings, total, |n: &str| {
                n.len() == 1 && n != "takenElig"
            }),
            "batch lines {lines:?}"
        );
    }
    // Below the threshold every preserved name is included, bindings
    // never consulted — the snapshot must replay that arm byte-for-byte.
    let small = ["refVar", "droppable1", "z"];
    let small_bindings = HashMap::from([("refVar", binding(500, &[450]))]);
    let w = snapshot_over(&small, &small_bindings, 99, |n: &str| {
        n.starts_with("droppable")
    });
    for lines in [Vec::<u32>::new(), vec![42]] {
        assert_eq!(
            w.windowed(&lines),
            serial_windowed(&small, &lines, &small_bindings, 99, |n: &str| {
                n.starts_with("droppable")
            }),
            "below-threshold batch lines {lines:?}"
        );
    }
}

#[test]
fn the_snapshot_reproduces_the_serial_plan_fixture() {
    // The fixture PLAN, captured from the serial path on main (the five
    // original cases' world plus a taken name): hardcoded, so a future
    // change to EITHER implementation has to keep the plan frozen.
    let bindings = HashMap::from([
        ("exports", binding(1, &[])),
        ("require", binding(2, &[])),
        ("console", binding(3, &[])),
        ("a", binding(1000, &[])),
        ("b", binding(1001, &[])),
        ("myVar", binding(50, &[])),
        ("nearVar", binding(55, &[])),
        ("farVar", binding(500, &[])),
        ("refVar", binding(900, &[45])),
        ("absentVar", ProximityBinding::default()),
    ]);
    // Well-known first (used-list order), then the windowed preserved
    // names; `a`/`b` are droppable, absent `toString` is kept to be safe.
    let plan: Vec<String> = [
        "exports", "require", "console", "myVar", "nearVar", "refVar", "toString",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    let all = [
        "exports",
        "a",
        "myVar",
        "require",
        "nearVar",
        "farVar",
        "b",
        "refVar",
        "console",
        "absentVar",
        "toString",
    ];
    let droppable = |n: &str| n.len() == 1;
    let w = snapshot_over(&all, &bindings, 200, droppable);
    assert_eq!(w.windowed(&[50]), plan);
    // ...and the serial path still produces the same plan itself.
    assert_eq!(
        serial_windowed(&all, &[50], &bindings, 200, |n: &str| n.len() == 1),
        plan
    );
}

#[test]
fn parallel_group_windowing_rejoins_in_the_serial_plan_order() {
    // The determinism pin: N groups windowed on the rayon pool must
    // produce, in group order, exactly the lane plan the serial path
    // produced group by group.
    let mut bindings = HashMap::new();
    let mut all: Vec<&str> = Vec::new();
    for i in 0..300u32 {
        let name = format!("n{i}");
        let leaked: &'static str = Box::leak(name.clone().into_boxed_str());
        bindings.insert(leaked, binding(i * 10, &[i * 10 + 1]));
        all.push(leaked);
    }
    let droppable = |n: &str| n.ends_with('7');
    let w = snapshot_over(&all, &bindings, 300, droppable);
    // 40 disjoint line windows = 40 module-lane groups.
    let group_lines: Vec<Vec<u32>> = (0..40).map(|g| vec![g * 75, g * 75 + 30]).collect();
    let serial: Vec<Vec<String>> = group_lines
        .iter()
        .map(|l| serial_windowed(&all, l, &bindings, 300, |n: &str| n.ends_with('7')))
        .collect();
    let parallel = crate::par::map_ordered(&group_lines, |l| w.windowed(l));
    assert_eq!(parallel, serial);
}
