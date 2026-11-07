//! The third spelling on record: a JS-written record REFLOWED by the
//! repo's prettier commit hook (printWidth 80, tabWidth 2) — 46 of the
//! committed eval files (`*-boot.json`, `self-hop.json`, two
//! `*.stats.json`), committed through lint-staged. Never written by the
//! harness: this exists so every legacy byte reads back and re-writes.
//!
//! prettier's JSON printer is its JS object/array printer, reproduced here
//! for JSON values (prettier 3, `printDocToString` and `fits` for the doc
//! parts JSON uses):
//! - an object is a group `{ a, b }` that breaks when it does not fit, or
//!   when the ORIGINAL text had a newline between `{` and its first key
//!   (prettier keeps an expanded object expanded; a broken group breaks
//!   every group around it) — that bit is read from
//!   the original text ([`expanded_objects`]), the one input beyond the value;
//! - an array is a group `[a, b]`; an array of two or more objects (each
//!   with more than one key) or arrays (each with more than one element)
//!   always breaks; an array of two or more numbers FILLS its lines;
//! - strings and numbers print as the JS writer spells them (the records
//!   were written by `JSON.stringify`, so that is their original raw text).

use humanify_ag_model::js::{JsValue, stringify};

const WIDTH: usize = 80;

enum Doc {
    Text(String),
    /// A space when flat, a newline when broken.
    Line,
    /// Nothing when flat, a newline when broken.
    SoftLine,
    Indent(Vec<Doc>),
    /// Parts in sequence, in the enclosing mode.
    Concat(Vec<Doc>),
    Group {
        brk: bool,
        parts: Vec<Doc>,
    },
    /// Alternating content, separator, content, ... (prettier `fill`).
    Fill(Vec<Doc>),
}

/// For each object in the original text, in order of its `{`: whether a
/// newline sits between the `{` and the next non-blank character.
pub fn expanded_objects(text: &str) -> Vec<bool> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    let mut in_str = false;
    while i < b.len() {
        let c = b[i];
        if in_str {
            match c {
                b'\\' => i += 1,
                b'"' => in_str = false,
                _ => {}
            }
        } else if c == b'"' {
            in_str = true;
        } else if c == b'{' {
            let rest = &b[i + 1..];
            let first = rest
                .iter()
                .position(|c| !c.is_ascii_whitespace())
                .unwrap_or(rest.len());
            out.push(rest[..first].contains(&b'\n'));
        }
        i += 1;
    }
    out
}

fn is_number(v: &JsValue) -> bool {
    matches!(v, JsValue::Number(_))
}

fn always_breaks(items: &[JsValue]) -> bool {
    items.len() > 1
        && items.iter().all(|v| match (v, &items[0]) {
            (JsValue::Object(o), JsValue::Object(_)) => o.len() > 1,
            (JsValue::Array(a), JsValue::Array(_)) => a.len() > 1,
            _ => false,
        })
}

/// Whether any group inside `parts` breaks: prettier propagates a broken
/// group's break to every enclosing group.
fn has_break(parts: &[Doc]) -> bool {
    parts.iter().any(|d| match d {
        Doc::Group { brk: true, .. } => true,
        Doc::Group { parts, .. } | Doc::Indent(parts) | Doc::Concat(parts) | Doc::Fill(parts) => {
            has_break(parts)
        }
        _ => false,
    })
}

fn doc(v: &JsValue, hints: &mut std::slice::Iter<'_, bool>) -> Doc {
    match v {
        JsValue::Object(o) => {
            let brk = hints.next().copied().unwrap_or(false);
            if o.is_empty() {
                return Doc::Text("{}".into());
            }
            let mut inner = vec![Doc::Line];
            for (i, (k, val)) in o.entries().iter().enumerate() {
                if i > 0 {
                    inner.push(Doc::Text(",".into()));
                    inner.push(Doc::Line);
                }
                inner.push(Doc::Text(format!(
                    "{}: ",
                    stringify(&JsValue::str(k.as_str()))
                )));
                inner.push(doc(val, hints));
            }
            Doc::Group {
                brk: brk || has_break(&inner),
                parts: vec![
                    Doc::Text("{".into()),
                    Doc::Indent(inner),
                    Doc::Line,
                    Doc::Text("}".into()),
                ],
            }
        }
        JsValue::Array(items) if items.is_empty() => Doc::Text("[]".into()),
        JsValue::Array(items) => {
            let fill = items.len() > 1 && items.iter().all(is_number);
            let mut seq = Vec::new();
            for (i, item) in items.iter().enumerate() {
                let last = i + 1 == items.len();
                if fill {
                    // prettier's concise array: each content carries its
                    // own comma; the separator is the bare line.
                    let mut content = vec![doc(item, hints)];
                    if !last {
                        content.push(Doc::Text(",".into()));
                    }
                    seq.push(Doc::Concat(content));
                    if !last {
                        seq.push(Doc::Line);
                    }
                    continue;
                }
                if i > 0 {
                    seq.push(Doc::Text(",".into()));
                    seq.push(Doc::Line);
                }
                seq.push(doc(item, hints));
            }
            let body = if fill { vec![Doc::Fill(seq)] } else { seq };
            let mut inner = vec![Doc::SoftLine];
            inner.extend(body);
            Doc::Group {
                brk: always_breaks(items) || has_break(&inner),
                parts: vec![
                    Doc::Text("[".into()),
                    Doc::Indent(inner),
                    Doc::SoftLine,
                    Doc::Text("]".into()),
                ],
            }
        }
        scalar => Doc::Text(stringify(scalar)),
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Flat,
    Break,
}

/// A command: a doc, or the rest of a fill (prettier pushes the remaining
/// fill parts back as a new fill).
#[derive(Clone, Copy)]
enum Item<'a> {
    D(&'a Doc),
    Fill(&'a [Doc]),
}

type Cmd<'a> = (usize, Mode, Item<'a>);

fn parts_of<'a>(item: Item<'a>) -> &'a [Doc] {
    match item {
        Item::Fill(p) => p,
        Item::D(Doc::Indent(p) | Doc::Concat(p) | Doc::Fill(p) | Doc::Group { parts: p, .. }) => p,
        Item::D(_) => &[],
    }
}

/// prettier's `fits`: does `next` (then, unless `must_be_flat`, the rest of
/// the line from `rest`) fit in `width` columns? A line in break mode ends
/// the measurement: it fits.
fn fits(next: Cmd<'_>, rest: &[Cmd<'_>], mut width: isize, must_be_flat: bool) -> bool {
    let mut stack: Vec<(Mode, Item<'_>)> = vec![(next.1, next.2)];
    let mut rest_idx = rest.len();
    loop {
        let Some((mode, item)) = stack.pop() else {
            if must_be_flat || rest_idx == 0 {
                return true;
            }
            rest_idx -= 1;
            stack.push((rest[rest_idx].1, rest[rest_idx].2));
            continue;
        };
        match item {
            Item::D(Doc::Text(s)) => width -= s.chars().count() as isize,
            Item::D(Doc::Line | Doc::SoftLine) if mode == Mode::Break => return true,
            Item::D(Doc::Line) => width -= 1,
            Item::D(Doc::SoftLine) => {}
            Item::D(Doc::Group { brk, parts }) => {
                let m = if *brk && !must_be_flat {
                    Mode::Break
                } else {
                    mode
                };
                stack.extend(parts.iter().rev().map(|p| (m, Item::D(p))));
            }
            other => stack.extend(parts_of(other).iter().rev().map(|p| (mode, Item::D(p)))),
        }
        if width < 0 {
            return false;
        }
    }
}

fn newline(out: &mut String, ind: usize) {
    while out.ends_with(' ') {
        out.pop();
    }
    out.push('\n');
    out.extend(std::iter::repeat_n(' ', ind));
}

fn print(root: &Doc) -> String {
    let mut out = String::new();
    let mut col = 0usize;
    let mut cmds: Vec<Cmd<'_>> = vec![(0, Mode::Break, Item::D(root))];
    while let Some((ind, mode, item)) = cmds.pop() {
        let room = WIDTH as isize - col as isize;
        match item {
            Item::D(Doc::Text(s)) => {
                out.push_str(s);
                col += s.chars().count();
            }
            Item::D(Doc::Line | Doc::SoftLine) if mode == Mode::Break => {
                newline(&mut out, ind);
                col = ind;
            }
            Item::D(Doc::Line) => {
                out.push(' ');
                col += 1;
            }
            Item::D(Doc::SoftLine) => {}
            Item::D(Doc::Indent(parts)) => {
                cmds.extend(parts.iter().rev().map(|p| (ind + 2, mode, Item::D(p))))
            }
            Item::D(Doc::Concat(parts)) => {
                cmds.extend(parts.iter().rev().map(|p| (ind, mode, Item::D(p))))
            }
            Item::D(Doc::Group { brk, parts }) => {
                let m = if *brk {
                    Mode::Break
                } else if mode == Mode::Flat || fits((ind, Mode::Flat, item), &cmds, room, false) {
                    Mode::Flat
                } else {
                    Mode::Break
                };
                cmds.extend(parts.iter().rev().map(|p| (ind, m, Item::D(p))));
            }
            Item::D(Doc::Fill(parts)) => fill_step(parts, ind, mode, room, &mut cmds),
            Item::Fill(parts) => fill_step(parts, ind, mode, room, &mut cmds),
        }
    }
    out
}

/// One step of prettier's `fill`: the first content flat if it fits; the
/// separator after it flat when the content after it fits too; the rest
/// pushed back as a fill.
fn fill_step<'a>(parts: &'a [Doc], ind: usize, mode: Mode, room: isize, cmds: &mut Vec<Cmd<'a>>) {
    let Some(content) = parts.first() else {
        return;
    };
    let flat = |d: &'a Doc| (ind, Mode::Flat, Item::D(d));
    let brk = |d: &'a Doc| (ind, Mode::Break, Item::D(d));
    let content_fits = fits(flat(content), &[], room, true);
    if parts.len() == 1 {
        cmds.push(if content_fits {
            flat(content)
        } else {
            brk(content)
        });
        return;
    }
    let sep = &parts[1];
    if parts.len() == 2 {
        if content_fits {
            cmds.extend([flat(sep), flat(content)]);
        } else {
            cmds.extend([brk(sep), brk(content)]);
        }
        return;
    }
    let pair_fits = measure_flat(Item::D(content))
        + measure_flat(Item::D(sep))
        + measure_flat(Item::D(&parts[2]))
        <= room;
    cmds.push((ind, mode, Item::Fill(&parts[2..])));
    if pair_fits {
        cmds.extend([flat(sep), flat(content)]);
    } else if content_fits {
        cmds.extend([brk(sep), flat(content)]);
    } else {
        cmds.extend([brk(sep), brk(content)]);
    }
}

/// The width of an item printed flat.
fn measure_flat(item: Item<'_>) -> isize {
    let mut width = 0isize;
    let mut stack = vec![item];
    while let Some(i) = stack.pop() {
        match i {
            Item::D(Doc::Text(s)) => width += s.chars().count() as isize,
            Item::D(Doc::Line) => width += 1,
            Item::D(Doc::SoftLine) => {}
            other => stack.extend(parts_of(other).iter().rev().map(Item::D)),
        }
    }
    width
}

/// `value` as prettier reflows its `JSON.stringify(value, null, 2)` text,
/// given which objects the original kept expanded.
pub fn reflow(value: &JsValue, expanded: &[bool]) -> String {
    let mut hints = expanded.iter();
    let d = doc(value, &mut hints);
    let mut out = print(&d);
    out.push('\n');
    out
}
