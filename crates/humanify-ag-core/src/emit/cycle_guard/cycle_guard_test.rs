use super::break_load_time_cycles;
use crate::emit::cjs::LoadGraph;

fn files(names: &[&str]) -> Vec<String> {
    names.iter().map(|s| s.to_string()).collect()
}

fn graph(edges: &[(usize, usize)]) -> LoadGraph {
    LoadGraph {
        edges: edges.to_vec(),
        follows: Vec::new(),
    }
}

fn anywhere(_: usize, _: &str, _: &str) -> bool {
    true
}

#[test]
fn no_cycle_moves_nothing() {
    // a.js reads b.js at load; b.js reads nothing at load
    let assignment = files(&["a.js", "b.js", "b.js"]);
    assert_eq!(
        break_load_time_cycles(&assignment, &graph(&[(0, 1), (2, 1)]), &anywhere),
        None
    );
}

/// True when no two files read each other at load (directly or through
/// others) under `assignment` (followers placed with what they follow).
fn acyclic(assignment: &[String], g: &LoadGraph) -> bool {
    let mut eff = assignment.to_vec();
    for &(s, lead) in &g.follows {
        eff[s] = eff[lead].clone();
    }
    let mut next: std::collections::BTreeMap<&str, Vec<&str>> = Default::default();
    for &(r, d) in &g.edges {
        if eff[r] != eff[d] {
            next.entry(&eff[r]).or_default().push(&eff[d]);
        }
    }
    // a cycle exists when some file reaches itself
    next.keys().all(|&start| {
        let mut seen = std::collections::BTreeSet::new();
        let mut todo: Vec<&str> = next[start].clone();
        while let Some(f) = todo.pop() {
            if f == start {
                return false;
            }
            if seen.insert(f) {
                todo.extend(next.get(f).cloned().unwrap_or_default());
            }
        }
        true
    })
}

#[test]
fn a_two_file_cycle_is_broken_by_moving_one_statement() {
    // a.js: 0 declares, 1 reads b.js's 3; b.js: 3 reads 2, and 4 reads
    // a.js's 0 at load: the cycle a.js -> b.js -> a.js. One statement
    // moved breaks it.
    let assignment = files(&["a.js", "a.js", "b.js", "b.js", "b.js"]);
    let g = graph(&[(1, 3), (3, 2), (4, 0)]);
    assert!(!acyclic(&assignment, &g));
    let repair = break_load_time_cycles(&assignment, &g, &anywhere).expect("a repair");
    assert!(acyclic(&repair.assignment, &g), "{repair:?}");
    assert_eq!(repair.moved.len(), 1, "{repair:?}");
    let (s, from, to) = &repair.moved[0];
    assert_eq!(&assignment[*s], from);
    assert_eq!(&repair.assignment[*s], to);
}

#[test]
fn a_move_that_would_only_turn_the_cycle_around_is_not_taken() {
    // b.js's statement 3 reads a.js's 0 AND is read at load by b.js's 2;
    // a.js's 1 reads b.js's 2. Moving 3 to a.js keeps b.js -> a.js (2
    // reads 3); moving 0 into b.js removes every edge out of b.js.
    let assignment = files(&["a.js", "a.js", "b.js", "b.js"]);
    let g = graph(&[(1, 2), (2, 3), (3, 0)]);
    let repair = break_load_time_cycles(&assignment, &g, &anywhere).expect("a repair");
    assert!(acyclic(&repair.assignment, &g), "{repair:?}");
    assert_eq!(repair.moved.len(), 1, "{repair:?}");
}

#[test]
fn a_vendor_statement_never_moves_and_an_unbreakable_cycle_is_left_to_the_emit() {
    let assignment = files(&["vendor/a.js", "vendor/b.js"]);
    let g = graph(&[(0, 1), (1, 0)]);
    let app_only =
        |_: usize, from: &str, to: &str| !from.starts_with("vendor/") && !to.starts_with("vendor/");
    assert_eq!(break_load_time_cycles(&assignment, &g, &app_only), None);
}

#[test]
fn a_namespace_augmentation_moves_with_the_statement_it_follows() {
    // 2 augments 1's namespace and follows it into 1's file. a.js's 0
    // reads b.js's 1; the augmentation 2 reads a.js's 0 at load — so the
    // cycle runs through the follower, and the move that breaks it is
    // made on the statement it follows (1 joins a.js, 2 with it).
    let assignment = files(&["a.js", "b.js", "a.js", "b.js"]);
    let g = LoadGraph {
        edges: vec![(0, 1), (2, 0), (3, 1)],
        follows: vec![(2, 1)],
    };
    let repair = break_load_time_cycles(&assignment, &g, &anywhere).expect("a repair");
    assert!(
        repair.moved.iter().all(|(s, _, _)| *s != 2),
        "the follower itself is never moved: {repair:?}"
    );
    assert!(acyclic(&repair.assignment, &g), "{repair:?}");
}

#[test]
fn a_three_file_cycle_is_broken() {
    // a.js reads b.js (0 reads 2), b.js reads c.js (3 reads 4), c.js
    // reads a.js (5 reads 1)
    let assignment = files(&["a.js", "a.js", "b.js", "b.js", "c.js", "c.js"]);
    let g = graph(&[(0, 2), (3, 4), (5, 1)]);
    let repair = break_load_time_cycles(&assignment, &g, &anywhere).expect("a repair");
    assert!(acyclic(&repair.assignment, &g), "{repair:?}");
    assert_eq!(repair.moved.len(), 1, "{repair:?}");
}

#[test]
fn a_function_split_from_its_load_time_caller_returns_to_the_caller() {
    // The shape that stopped a release: w.js holds the lazy-init helper
    // (0), two inits built with it (2, 3), and `cwd = getCwd()` (4) plus
    // a reader of `cwd` (5); placement put `getCwd` (1) in p.js, whose
    // own init (6) calls the helper at load. w.js -> p.js (4 reads 1),
    // p.js -> w.js (6 reads 0). Moving the helper into p.js or `cwd`
    // into p.js leaves more reads across files; the init joining w.js
    // ties with `getCwd` returning — and the declaration moves.
    let assignment = files(&[
        "w.js", "p.js", "w.js", "w.js", "w.js", "w.js", "p.js", "p.js",
    ]);
    let g = graph(&[(2, 0), (3, 0), (4, 1), (5, 4), (6, 0), (7, 6)]);
    let repair = break_load_time_cycles(&assignment, &g, &anywhere).expect("a repair");
    assert_eq!(repair.moved, vec![(1, "p.js".into(), "w.js".into())]);
    assert!(acyclic(&repair.assignment, &g));
}
