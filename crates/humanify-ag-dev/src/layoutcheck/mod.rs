//! `cargo dev layout check`: a tree's layout record held against the
//! tree and against the previous tree's record.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::Path;

use humanify_ag_core::layout_record::{
    FileEntry, FunctionId, LAYOUT_PATH, Layout, Match, Row, Unit, checksum,
};

/// A tree's text files: path and line count.
pub type Files = BTreeMap<String, u32>;

/// One rule that did not hold: how often, and the first place.
#[derive(Debug, PartialEq, Eq)]
pub struct Failure {
    count: usize,
    what: String,
    first: String,
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.count == 0 {
            return write!(f, "{}", self.what);
        }
        write!(f, "{} {}; first: {}", self.count, self.what, self.first)
    }
}

#[derive(Debug, Default)]
pub struct Checked {
    pub files: usize,
    pub units: usize,
    pub functions: usize,
    pub failures: Vec<Failure>,
}

/// The rules in the order they are reported: singular and plural name.
const RULES: &[(&str, &str)] = &[
    (
        "entry with no file in the tree",
        "entries with no file in the tree",
    ),
    (
        "file under src/ with no entry",
        "files under src/ with no entry",
    ),
    (
        "unit that does not start after the unit before it",
        "units that do not start after the unit before them",
    ),
    (
        "unit past the end of its file",
        "units past the end of their file",
    ),
    (
        "row outside its unit's lines or out of order",
        "rows outside their unit's lines or out of order",
    ),
    (
        "unit with the identity of an earlier unit",
        "units with the identity of an earlier unit",
    ),
    (
        "function with the identity of an earlier function",
        "functions with the identity of an earlier function",
    ),
    (
        "unit of an input file the layout does not list",
        "units of an input file the layout does not list",
    ),
    (
        "function whose partner another function also has",
        "functions whose partner another function also has",
    ),
    (
        "function whose partner the previous layout does not list",
        "functions whose partner the previous layout does not list",
    ),
    (
        "ambiguous previous function the previous layout does not list",
        "ambiguous previous functions the previous layout does not list",
    ),
];

#[derive(Default)]
struct Found {
    by_rule: BTreeMap<usize, (usize, String)>,
}

impl Found {
    fn add(&mut self, rule: usize, place: impl FnOnce() -> String) {
        let entry = self.by_rule.entry(rule).or_insert_with(|| (0, place()));
        entry.0 += 1;
    }

    fn failures(self) -> Vec<Failure> {
        let named = |(rule, (count, first)): (usize, (usize, String))| Failure {
            count,
            what: (if count == 1 {
                RULES[rule].0
            } else {
                RULES[rule].1
            })
            .to_string(),
            first,
        };
        self.by_rule.into_iter().map(named).collect()
    }
}

type UnitKey<'a> = (u32, Option<u32>, Option<&'a str>, Option<u32>, Option<u32>);

fn unit_key(unit: &Unit) -> Option<UnitKey<'_>> {
    let (input, byte) = unit.at?;
    Some((input, byte, unit.linker.as_deref(), unit.target, unit.nth))
}

fn row_line(row: &Row) -> u32 {
    match row {
        Row::Origin { line, .. } | Row::Function { line, .. } => *line,
    }
}

/// One file's units against its line count and the layout's inputs.
fn check_file(path: &str, entry: &FileEntry, lines: u32, inputs: usize, found: &mut Found) {
    let units: Vec<&Unit> = entry.all_units().collect();
    let mut seen = BTreeSet::new();
    for (i, unit) in units.iter().enumerate() {
        let place = || format!("{path} line {}", unit.line);
        if i > 0 && unit.line <= units[i - 1].line {
            found.add(2, place);
        }
        if unit.line > lines || unit.line == 0 {
            found.add(3, place);
        }
        let end = units.get(i + 1).map_or(u32::MAX, |next| next.line);
        let mut last = unit.line;
        for row in &unit.rows {
            let line = row_line(row);
            if line < last || line >= end {
                found.add(4, || format!("{path} line {line}"));
            }
            last = line.max(last);
        }
        if unit_key(unit).is_some_and(|key| !seen.insert(key)) {
            found.add(5, place);
        }
        if unit.at.is_some_and(|at| at.0 as usize >= inputs) {
            found.add(7, place);
        }
    }
    let mut functions = BTreeSet::new();
    for function in entry.functions() {
        let known = *function.id != FunctionId::Unknown;
        if known && !functions.insert((function.input, function.id)) {
            found.add(6, || format!("{path} line {}", function.line));
        }
    }
}

/// The new layout's partners against the previous layout's functions.
fn check_partners(layout: &Layout, previous: &Layout, found: &mut Found) {
    let listed: BTreeSet<(u32, &FunctionId)> = (previous.files.get(&previous.named_text))
        .into_iter()
        .flat_map(|e| e.functions())
        .map(|f| (f.input, f.id))
        .collect();
    let mut claimed = BTreeSet::new();
    for (path, entry) in &layout.files {
        for function in entry.functions() {
            let (Match::Exact(Some(partner)) | Match::Close(Some(partner))) = function.matched
            else {
                continue;
            };
            let place = || format!("{path} line {}", function.line);
            let key = (partner.0, &partner.1);
            if !listed.contains(&key) {
                found.add(9, place);
            } else if !claimed.insert((path.as_str(), key)) {
                found.add(8, place);
            }
        }
    }
    for (input, id) in &layout.prior_ambiguous {
        if !listed.contains(&(*input, id)) {
            found.add(10, || format!("input {input} {id:?}"));
        }
    }
}

/// Every rule over one layout. `previous`: the previous tree's layout
/// text and its parse.
pub fn check(layout: &Layout, files: &Files, previous: Option<(&str, &Layout)>) -> Checked {
    let mut found = Found::default();
    let mut checked = Checked {
        files: layout.files.len(),
        ..Checked::default()
    };
    for (path, entry) in &layout.files {
        checked.units += entry.all_units().count();
        let Some(lines) = files.get(path) else {
            found.add(0, || path.clone());
            continue;
        };
        check_file(path, entry, *lines, layout.inputs.len(), &mut found);
    }
    let named = layout.files.get(&layout.named_text);
    checked.functions = named.map_or(0, |e| e.functions().count());
    for path in files.keys().filter(|p| p.starts_with("src/")) {
        if !layout.files.contains_key(path) {
            found.add(1, || path.clone());
        }
    }
    let mut failures = Vec::new();
    if let Some((text, previous)) = previous {
        let sum = checksum(text);
        if layout.prior_layout.as_deref() != Some(sum.as_str()) {
            let names = layout.prior_layout.as_deref().unwrap_or("null");
            failures.push(Failure {
                count: 0,
                what: format!(
                    "the layout names previous layout {names}, the previous tree's is {sum}"
                ),
                first: String::new(),
            });
        }
        check_partners(layout, previous, &mut found);
    }
    let mut by_rule = found.failures();
    let after =
        by_rule.partition_point(|f| !f.what.contains("partner") && !f.what.contains("ambiguous"));
    by_rule.splice(after..after, failures);
    checked.failures = by_rule;
    checked
}

/// A tree's layout record: its text and its parse.
pub fn read_layout(tree: &Path) -> Result<(String, Layout), String> {
    let path = tree.join(LAYOUT_PATH);
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let layout = Layout::parse(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok((text, layout))
}

fn text_files(tree: &Path) -> Result<Files, String> {
    let files = humanify_ag_io::tree::read_tree(tree, &|_| true).map_err(|e| e.to_string())?;
    let lines = |bytes: &Vec<u8>| {
        let ends = bytes.iter().filter(|b| **b == b'\n').count();
        let open = bytes.last().is_some_and(|b| *b != b'\n');
        (ends + usize::from(open)) as u32
    };
    Ok(files.iter().map(|(p, b)| (p.clone(), lines(b))).collect())
}

/// A tree's layout held against the tree and, when given, the previous
/// tree. `Err`: the tree or a layout could not be read.
pub fn check_tree(tree: &Path, previous: Option<&Path>) -> Result<Checked, String> {
    let (_, layout) = read_layout(tree)?;
    let previous = previous.map(read_layout).transpose()?;
    let previous = previous
        .as_ref()
        .map(|(text, layout)| (text.as_str(), layout));
    Ok(check(&layout, &text_files(tree)?, previous))
}

/// The `layout check` verb: the report, and whether every rule held.
pub fn run(tree: &Path, previous: Option<&Path>) -> Result<(String, bool), String> {
    let checked = check_tree(tree, previous)?;
    let mut report = format!(
        "layout check: {}\n{} files, {} units, {} functions in the named text\n",
        tree.display(),
        checked.files,
        checked.units,
        checked.functions
    );
    for failure in &checked.failures {
        report.push_str(&format!("FAIL: {failure}\n"));
    }
    if checked.failures.is_empty() {
        report.push_str("every rule holds\n");
    }
    Ok((report, checked.failures.is_empty()))
}

#[cfg(test)]
#[path = "check_test.rs"]
mod check_test;
