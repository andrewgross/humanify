//! The load-time cycle guard: placement may put code into two files that
//! read each other at load time (a function placed by its new graph
//! neighbours while its load-time caller is kept in its prior file),
//! and the runnable emit cannot write such a pair of files — it declines
//! and only the review tree is written. The guard reads the bundle's
//! [`LoadGraph`] and breaks every cycle one file-to-file edge at a time:
//! the edge's readers join the file they read, or the declarations they
//! read join the readers' file — whichever leaves fewer files on a cycle,
//! then empties no file, then leaves the fewest load-time reads across
//! files (the code stays with what it reads at load), then moves fewer
//! statements, then moves declarations rather than their readers. A
//! statement under `vendor/` or headed for it stays put, and each
//! statement moves at most once.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use super::cjs::LoadGraph;

/// The guard's result: the new per-statement files and every move.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CycleRepair {
    pub assignment: Vec<String>,
    /// (statement, from file, to file), in move order.
    pub moved: Vec<(usize, String, String)>,
}

/// The guard's bound on rounds (each moves one edge's statements).
const MAX_ROUNDS: usize = 16;

/// Moves that break every load-time cycle between files of `assignment`,
/// each statement allowed only when `movable(statement, from, to)`. None:
/// there is no cycle, or the allowed moves cannot break one.
pub fn break_load_time_cycles(
    assignment: &[String],
    graph: &LoadGraph,
    movable: &dyn Fn(usize, &str, &str) -> bool,
) -> Option<CycleRepair> {
    let mut placed = Placed::new(assignment, graph);
    let mut cyclic = placed.cyclic_files(graph);
    if cyclic == 0 {
        return None;
    }
    let mut moved: Vec<(usize, String, String)> = Vec::new();
    for _ in 0..MAX_ROUNDS {
        let best = placed
            .candidates(graph)
            .into_iter()
            .filter(|c| {
                c.statements.iter().all(|&s| {
                    !moved.iter().any(|(m, _, _)| *m == s)
                        && movable(s, &placed.names[placed.file[s]], &placed.names[c.to])
                })
            })
            .map(|c| (placed.with(&c).score(graph, placed.file_count()), c))
            .filter(|(after, _)| after.cyclic < cyclic)
            .min_by(|(a, x), (b, y)| {
                a.cmp(b)
                    .then(x.statements.len().cmp(&y.statements.len()))
                    .then(x.readers.cmp(&y.readers))
            })?;
        let (score, c) = best;
        let after = score.cyclic;
        for &s in &c.statements {
            moved.push((
                s,
                placed.names[placed.file[s]].clone(),
                placed.names[c.to].clone(),
            ));
        }
        placed = placed.with(&c);
        cyclic = after;
        if cyclic == 0 {
            return Some(CycleRepair {
                assignment: placed.assignment(),
                moved,
            });
        }
    }
    None
}

/// How a placement ranks, best first: fewer files on a cycle, no file
/// emptied, fewer load-time reads across files.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Score {
    cyclic: usize,
    emptied: usize,
    cross: usize,
}

/// One candidate move: these statements (each its own lead, not a
/// follower) join `to`.
struct Candidate {
    statements: Vec<usize>,
    to: usize,
    /// The edge's readers move (false: the declarations they read move,
    /// preferred on a tie — a declaration carries no load-time work).
    readers: bool,
}

/// The statements' files as ids; a follower's own entry is ignored (it is
/// placed with the statement it follows).
#[derive(Clone)]
struct Placed {
    names: Vec<String>,
    file: Vec<usize>,
    /// The statement each follower is placed with (itself otherwise).
    lead: Vec<usize>,
}

impl Placed {
    fn new(assignment: &[String], graph: &LoadGraph) -> Placed {
        let mut names: Vec<String> = assignment.to_vec();
        names.sort();
        names.dedup();
        let id: HashMap<&str, usize> = names
            .iter()
            .enumerate()
            .map(|(i, n)| (n.as_str(), i))
            .collect();
        let file = assignment.iter().map(|f| id[f.as_str()]).collect();
        let mut lead: Vec<usize> = (0..assignment.len()).collect();
        for &(s, l) in &graph.follows {
            lead[s] = lead[l];
        }
        Placed { names, file, lead }
    }

    /// The file a statement is written to.
    fn at(&self, s: usize) -> usize {
        self.file[self.lead[s]]
    }

    fn with(&self, c: &Candidate) -> Placed {
        let mut next = self.clone();
        for &s in &c.statements {
            next.file[s] = c.to;
        }
        next
    }

    fn assignment(&self) -> Vec<String> {
        (0..self.file.len())
            .map(|s| self.names[self.file[s]].clone())
            .collect()
    }

    /// file -> files it reads at load.
    fn file_edges(&self, graph: &LoadGraph) -> BTreeMap<usize, BTreeSet<usize>> {
        let mut out: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
        for &(r, d) in &graph.edges {
            let (fr, fd) = (self.at(r), self.at(d));
            if fr != fd {
                out.entry(fr).or_default().insert(fd);
            }
        }
        out
    }

    /// How many files hold at least one statement.
    fn file_count(&self) -> usize {
        let held: BTreeSet<usize> = (0..self.file.len()).map(|s| self.at(s)).collect();
        held.len()
    }

    /// This placement's [`Score`]; `files_before` is the file count it
    /// was made from.
    fn score(&self, graph: &LoadGraph, files_before: usize) -> Score {
        let cross = graph
            .edges
            .iter()
            .filter(|&&(r, d)| self.at(r) != self.at(d))
            .count();
        Score {
            cyclic: self.cyclic_files(graph),
            emptied: files_before.saturating_sub(self.file_count()),
            cross,
        }
    }

    /// How many files sit on a load-time cycle.
    fn cyclic_files(&self, graph: &LoadGraph) -> usize {
        let edges = self.file_edges(graph);
        let component = components(self.names.len(), &edges);
        let mut size = vec![0usize; self.names.len()];
        for &c in &component {
            size[c] += 1;
        }
        component.iter().filter(|&&c| size[c] > 1).count()
    }

    /// For every file-to-file load-time edge inside one cycle: its
    /// readers joining the file they read, and the statements they read
    /// joining the readers' file (each a statement's lead).
    fn candidates(&self, graph: &LoadGraph) -> Vec<Candidate> {
        let component = components(self.names.len(), &self.file_edges(graph));
        let mut size = vec![0usize; self.names.len()];
        for &c in &component {
            size[c] += 1;
        }
        let mut by_edge: BTreeMap<(usize, usize), (BTreeSet<usize>, BTreeSet<usize>)> =
            BTreeMap::new();
        for &(r, d) in &graph.edges {
            let (fr, fd) = (self.at(r), self.at(d));
            if fr != fd && component[fr] == component[fd] && size[component[fr]] > 1 {
                let e = by_edge.entry((fr, fd)).or_default();
                e.0.insert(self.lead[r]);
                e.1.insert(self.lead[d]);
            }
        }
        let mut out = Vec::new();
        for ((fr, fd), (readers, read)) in by_edge {
            out.push(Candidate {
                statements: readers.into_iter().collect(),
                to: fd,
                readers: true,
            });
            out.push(Candidate {
                statements: read.into_iter().collect(),
                to: fr,
                readers: false,
            });
        }
        out
    }
}

/// Strongly connected components (Tarjan, iterative): each node's
/// component id.
fn components(n: usize, edges: &BTreeMap<usize, BTreeSet<usize>>) -> Vec<usize> {
    const UNSEEN: usize = usize::MAX;
    let succ: Vec<Vec<usize>> = (0..n)
        .map(|v| {
            edges
                .get(&v)
                .map_or_else(Vec::new, |s| s.iter().copied().collect())
        })
        .collect();
    let (mut index, mut low, mut comp) = (vec![UNSEEN; n], vec![0; n], vec![UNSEEN; n]);
    let mut on_stack = vec![false; n];
    let (mut stack, mut next_index, mut next_comp) = (Vec::new(), 0, 0);
    for root in 0..n {
        if index[root] != UNSEEN {
            continue;
        }
        let mut call: Vec<(usize, usize)> = vec![(root, 0)];
        index[root] = next_index;
        low[root] = next_index;
        next_index += 1;
        stack.push(root);
        on_stack[root] = true;
        while let Some(&mut (v, ref mut i)) = call.last_mut() {
            if let Some(&w) = succ[v].get(*i) {
                *i += 1;
                if index[w] == UNSEEN {
                    index[w] = next_index;
                    low[w] = next_index;
                    next_index += 1;
                    stack.push(w);
                    on_stack[w] = true;
                    call.push((w, 0));
                } else if on_stack[w] {
                    low[v] = low[v].min(index[w]);
                }
                continue;
            }
            call.pop();
            if let Some(&(parent, _)) = call.last() {
                low[parent] = low[parent].min(low[v]);
            }
            if low[v] == index[v] {
                while let Some(w) = stack.pop() {
                    on_stack[w] = false;
                    comp[w] = next_comp;
                    if w == v {
                        break;
                    }
                }
                next_comp += 1;
            }
        }
    }
    comp
}

#[cfg(test)]
#[path = "cycle_guard/cycle_guard_test.rs"]
mod cycle_guard_test;
