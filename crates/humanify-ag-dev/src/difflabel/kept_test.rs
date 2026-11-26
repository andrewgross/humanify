use super::{ChangedLine, refine};
use crate::difflabel::Label;

fn line(hunk: u32, to_side: bool, text: &str) -> ChangedLine<'_> {
    ChangedLine {
        hunk,
        to_side,
        text,
        label: Label::InputChanged,
    }
}

fn labels(mut lines: Vec<ChangedLine<'_>>) -> Vec<Label> {
    refine(&mut lines);
    lines.iter().map(|l| l.label).collect()
}

#[test]
fn a_line_the_hunk_has_on_both_sides_under_other_names_is_kept_renamed() {
    let got = labels(vec![
        line(0, false, "  const total = computeTotal(items, taxRate);"),
        line(0, true, "  const sum = computeTotal(entries, taxRate);"),
        line(0, true, "  reportTotal(sum, entries.length);"),
    ]);
    assert_eq!(
        got,
        [Label::KeptRenamed, Label::KeptRenamed, Label::InputChanged]
    );
}

#[test]
fn a_line_the_hunk_has_on_both_sides_at_another_indentation_is_reindented() {
    let got = labels(vec![
        line(0, false, "  await writeSnapshot(path, body);"),
        line(0, true, "    await writeSnapshot(path, body);"),
    ]);
    assert_eq!(got, [Label::InputReindented, Label::InputReindented]);
}

#[test]
fn a_line_removed_in_one_hunk_and_added_in_another_is_kept_moved() {
    let got = labels(vec![
        line(0, false, "  const session = openSession(config, options);"),
        line(1, true, "  const handle = openSession(settings, options);"),
        line(1, true, "  const other = closeSession(handle);"),
    ]);
    assert_eq!(
        got,
        [Label::KeptMoved, Label::KeptMoved, Label::InputChanged]
    );
}

#[test]
fn a_short_line_pairs_in_its_hunk_in_order_only() {
    let got = labels(vec![
        line(0, false, "  return a;"),
        line(0, false, "  x = y;"),
        line(0, true, "  p = q;"),
        line(0, true, "  return b;"),
    ]);
    let renamed = got.iter().filter(|l| **l == Label::KeptRenamed).count();
    assert_eq!(renamed, 2);
}

#[test]
fn a_short_line_pairs_with_nothing_in_another_hunk() {
    let got = labels(vec![line(1, false, "  x = y;"), line(2, true, "  p = q;")]);
    assert_eq!(got, [Label::InputChanged; 2]);
}

#[test]
fn a_line_with_no_name_is_not_renamed() {
    let got = labels(vec![
        line(0, false, "  } else {"),
        line(0, true, "    } else {"),
        line(0, false, "  return true;"),
        line(0, true, "  return true ;"),
    ]);
    assert_eq!(
        got,
        [
            Label::InputReindented,
            Label::InputReindented,
            Label::InputChanged,
            Label::InputChanged
        ]
    );
}

#[test]
fn each_line_pairs_once() {
    let got = labels(vec![
        line(0, false, "  const total = computeTotal(items, taxRate);"),
        line(0, true, "  const sum = computeTotal(entries, taxRate);"),
        line(0, true, "  const net = computeTotal(others, taxRate);"),
    ]);
    assert_eq!(
        got,
        [Label::KeptRenamed, Label::KeptRenamed, Label::InputChanged]
    );
}

#[test]
fn a_line_with_another_label_is_left_alone() {
    let mut lines = vec![
        line(0, false, "  const total = computeTotal(items, taxRate);"),
        line(0, true, "  const sum = computeTotal(entries, taxRate);"),
    ];
    lines[0].label = Label::Renamed;
    refine(&mut lines);
    assert_eq!(lines[0].label, Label::Renamed);
    assert_eq!(lines[1].label, Label::InputChanged);
}
