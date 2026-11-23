//! The run: read both trees and both inputs, label every changed line,
//! and print totals that add up to what `diff -rN` counts.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::Path;
use std::time::Instant;

use serde_json::json;

use super::{InputFile, Label, Labeller, Tree, Truth};
use crate::eval::difftool::Differ;
use crate::runners::tree_diff::{DIAGNOSTIC_ONLY, changed_lines_in_trees};

const SURFACES: [&str; 4] = ["src", "vendor", "metadata", "other"];
const EXAMPLES_PER_LABEL: usize = 20;
pub const GROUPS: [&str; 4] = ["real", "noise", "wiring", "unknown"];

pub fn surface(path: &str) -> &'static str {
    match path.split('/').next() {
        Some("src") => "src",
        Some("vendor") => "vendor",
        Some(".humanify") => "metadata",
        _ => "other",
    }
}

fn range(text: &str) -> Option<(u32, u32)> {
    let (first, last) = text.split_once(',').unwrap_or((text, text));
    Some((first.parse().ok()?, last.parse().ok()?))
}

/// The changed line numbers of each side, from a normal-format diff.
pub fn changed_lines(diff: &str) -> Result<(Vec<u32>, Vec<u32>), String> {
    let (mut from, mut to) = (Vec::new(), Vec::new());
    for header in diff
        .lines()
        .filter(|l| l.starts_with(|c: char| c.is_ascii_digit()))
    {
        let parsed = header.find(['a', 'c', 'd']).and_then(|at| {
            Some((
                range(&header[..at])?,
                &header[at..=at],
                range(&header[at + 1..])?,
            ))
        });
        let Some((left, kind, right)) = parsed else {
            return Err(format!("not a diff header: {header}"));
        };
        if kind != "a" {
            from.extend(left.0..=left.1);
        }
        if kind != "d" {
            to.extend(right.0..=right.1);
        }
    }
    Ok((from, to))
}

pub fn read_input(dir: &Path) -> Result<Vec<InputFile>, String> {
    let error = |e: std::io::Error| format!("{}: {e}", dir.display());
    let mut texts = Vec::new();
    for entry in std::fs::read_dir(dir).map_err(error)? {
        let name = entry
            .map_err(error)?
            .file_name()
            .to_string_lossy()
            .into_owned();
        if name == "index.js" || (name.starts_with("chunk-") && name.ends_with(".js")) {
            let bytes = std::fs::read(dir.join(&name)).map_err(error)?;
            texts.push((name, String::from_utf8_lossy(&bytes).into_owned()));
        }
    }
    texts.sort();
    Ok(humanify_ag_core::par::map_ordered(
        &texts,
        |(name, text)| InputFile::parse(name, text),
    ))
}

fn counted(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    !DIAGNOSTIC_ONLY
        .iter()
        .any(|d| d.rsplit('/').next() == Some(name))
}

/// Every changed line: `(in the to tree, path, line)`.
fn changed<'a>(
    roots: (&Path, &Path),
    from: &'a Tree,
    to: &'a Tree,
    not_text: &mut u64,
) -> Result<Vec<(bool, &'a str, u32)>, String> {
    let paths: BTreeSet<&str> = from
        .paths()
        .chain(to.paths())
        .filter(|p| counted(p))
        .collect();
    let mut out = Vec::new();
    for path in paths {
        let (a, b) = (from.text(path), to.text(path));
        if (from.has(path) && a.is_none()) || (to.has(path) && b.is_none()) {
            *not_text += 1;
            continue;
        }
        let (lines_from, lines_to) = match (a, b) {
            (Some(a), Some(b)) if a == b => continue,
            (Some(_), Some(_)) => {
                changed_lines(&Differ.normal_diff(&roots.0.join(path), &roots.1.join(path))?)?
            }
            _ => (
                (1..=from.line_count(path).unwrap_or(0)).collect(),
                (1..=to.line_count(path).unwrap_or(0)).collect(),
            ),
        };
        out.extend(lines_from.into_iter().map(|l| (false, path, l)));
        out.extend(lines_to.into_iter().map(|l| (true, path, l)));
    }
    Ok(out)
}

#[derive(Default)]
struct Tally {
    lines: BTreeMap<(Label, &'static str), u64>,
    examples: BTreeMap<Label, Vec<String>>,
}

impl Tally {
    fn add(&mut self, label: Label, to_side: bool, path: &str, line: u32, text: &str) {
        *self.lines.entry((label, surface(path))).or_default() += 1;
        let examples = self.examples.entry(label).or_default();
        if examples.len() < EXAMPLES_PER_LABEL {
            let sign = if to_side { '+' } else { '-' };
            let short: String = text.chars().take(160).collect();
            examples.push(format!("{sign} {path}:{line}: {short}"));
        }
    }

    fn of(&self, label: Label) -> u64 {
        SURFACES.iter().map(|s| self.cell(label, s)).sum()
    }

    fn cell(&self, label: Label, surface: &'static str) -> u64 {
        self.lines.get(&(label, surface)).copied().unwrap_or(0)
    }

    fn total(&self) -> u64 {
        self.lines.values().sum()
    }
}

fn percent(part: u64, whole: u64) -> String {
    format!("{:.1}%", 100.0 * part as f64 / whole.max(1) as f64)
}

fn table(tally: &Tally) -> String {
    let total = tally.total();
    let mut out = String::new();
    let code = |label: Label| tally.of(label) - tally.cell(label, "metadata");
    let code_total: u64 = Label::ALL.iter().map(|l| code(*l)).sum();
    let _ = writeln!(
        out,
        "{:<8} {:>10} {:>7}   {:>10} {:>7}",
        "group", "all lines", "share", "no .humanify", "share"
    );
    for group in GROUPS {
        let of_group = |count: &dyn Fn(Label) -> u64| -> u64 {
            let in_group = Label::ALL.iter().filter(|l| l.describe().1 == group);
            in_group.map(|l| count(*l)).sum()
        };
        let (all, without) = (of_group(&|l| tally.of(l)), of_group(&code));
        let _ = writeln!(
            out,
            "{group:<8} {all:>10} {:>7}   {without:>10} {:>7}",
            percent(all, total),
            percent(without, code_total)
        );
    }
    let _ = writeln!(
        out,
        "\n{:<20} {:<8} {:>10} {:>7} {:>10} {:>10} {:>10} {:>8}  meaning",
        "label", "group", "lines", "share", "src", "vendor", "metadata", "other"
    );
    let mut labels = Label::ALL;
    labels.sort_by_key(|l| std::cmp::Reverse(tally.of(*l)));
    for label in labels {
        let (name, group, meaning) = label.describe();
        let _ = writeln!(
            out,
            "{name:<20} {group:<8} {:>10} {:>7} {:>10} {:>10} {:>10} {:>8}  {meaning}",
            tally.of(label),
            percent(tally.of(label), total),
            tally.cell(label, "src"),
            tally.cell(label, "vendor"),
            tally.cell(label, "metadata"),
            tally.cell(label, "other"),
        );
    }
    out
}

fn as_json(tally: &Tally, diff_total: u64, truth: &Truth) -> serde_json::Value {
    let labels: BTreeMap<&str, serde_json::Value> = Label::ALL
        .iter()
        .map(|l| {
            let by_surface: BTreeMap<&str, u64> =
                SURFACES.iter().map(|s| (*s, tally.cell(*l, s))).collect();
            let (name, group, _) = l.describe();
            let examples = tally.examples.get(l).cloned().unwrap_or_default();
            (name, json!({"group": group, "lines": tally.of(*l), "bySurface": by_surface, "examples": examples}))
        })
        .collect();
    let group_lines = |group: &str, without_metadata: bool| -> u64 {
        let in_group = Label::ALL.iter().filter(|l| l.describe().1 == group);
        let metadata = |l: Label| {
            if without_metadata {
                tally.cell(l, "metadata")
            } else {
                0
            }
        };
        in_group.map(|l| tally.of(*l) - metadata(*l)).sum()
    };
    let groups: BTreeMap<&str, serde_json::Value> = GROUPS
        .iter()
        .map(|g| {
            (
                *g,
                json!({"lines": group_lines(g, false), "outsideMetadata": group_lines(g, true)}),
            )
        })
        .collect();
    json!({"changedLines": tally.total(), "diffSays": diff_total, "unchangedInputPairs": truth.pairs,
        "groups": groups,
        "pairedInputFiles": truth.paired_files(),
        "unpairedInputFiles": {"from": truth.unpaired_files(false), "to": truth.unpaired_files(true)}, "labels": labels})
}

/// One labelled diff.
pub struct Labelled {
    pub report: String,
    pub card: serde_json::Value,
    /// One row per changed line: side, path, line number, label.
    pub lines: String,
    /// Whether the labelled lines add up to what `diff -rN` counts.
    pub adds_up: bool,
}

/// Label every changed line between two trees.
pub fn label_trees(trees: (&Path, &Path), inputs: (&Path, &Path)) -> Result<Labelled, String> {
    let started = Instant::now();
    let (from, to) = humanify_ag_core::par::join(|| Tree::read(trees.0), || Tree::read(trees.1));
    let (from, to) = (from?, to?);
    let truth = Truth::compare(read_input(inputs.0)?, read_input(inputs.1)?);
    let read_seconds = started.elapsed().as_secs_f64();
    let labeller = Labeller::new(&truth, &from, &to);
    let mut not_text = 0;
    let mut tally = Tally::default();
    let mut lines = String::new();
    for (to_side, path, line) in changed(trees, &from, &to, &mut not_text)? {
        let tree = if to_side { &to } else { &from };
        let label = labeller.label(to_side, path, line);
        let side = if to_side { '+' } else { '-' };
        lines.push_str(&format!("{side}\t{path}\t{line}\t{}\n", label.describe().0));
        tally.add(
            label,
            to_side,
            path,
            line,
            tree.line(path, line).unwrap_or(""),
        );
    }
    let label_seconds = started.elapsed().as_secs_f64() - read_seconds;
    let diff_total = changed_lines_in_trees(trees.0, trees.1)?;
    let (paired, files_from, files_to) = truth.file_counts();
    let mut report = format!(
        "diff label: {} -> {}\nchanged lines: {} (diff -rN counts {diff_total})\n\
         unchanged input pairs: {}; input files paired: {paired} of {files_from} -> {files_to}; \
         files that are not text, not compared: {not_text}\n\
         time: read and compare inputs {read_seconds:.1}s, diff and label {label_seconds:.1}s\n\n",
        trees.0.display(),
        trees.1.display(),
        tally.total(),
        truth.pairs,
    );
    report.push_str(&table(&tally));
    Ok(Labelled {
        report,
        card: as_json(&tally, diff_total, &truth),
        lines,
        adds_up: tally.total() == diff_total,
    })
}

/// The `diff label` verb: the report text; `Err` when the totals differ
/// from the diff's count. With `out`, the card goes there and the
/// per-line listing beside it as `<out>.lines.tsv`.
pub fn run(
    trees: (&Path, &Path),
    inputs: (&Path, &Path),
    out: Option<&Path>,
) -> Result<String, String> {
    let labelled = label_trees(trees, inputs)?;
    if let Some(out) = out {
        std::fs::write(out, format!("{:#}\n", labelled.card))
            .map_err(|e| format!("{}: {e}", out.display()))?;
        let listing = out.with_extension("lines.tsv");
        std::fs::write(&listing, &labelled.lines)
            .map_err(|e| format!("{}: {e}", listing.display()))?;
    }
    if labelled.adds_up {
        Ok(labelled.report)
    } else {
        Err(format!("{}\n{NOT_ADDING_UP}", labelled.report))
    }
}

pub const NOT_ADDING_UP: &str = "!! the labelled lines differ from the diff's count";

#[cfg(test)]
#[path = "report_test.rs"]
mod report_test;
