//! A file's export key follows its binding: when the post-split
//! reconcile gives a top-level binding its prior name, the key the file
//! exports it under and every other file's use of that key take the
//! name too.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use oxc_allocator::Allocator;
use oxc_ast::AstKind;
use oxc_ast::ast::Expression;

use crate::emit::import_alias::parse_require_aliases;

use super::reconcile::PostSplitRename;
use super::relink::parse_or_err;

fn accessor_prefix(key: &str) -> String {
    format!("Object.defineProperty(module.exports, \"{key}\", {{")
}

/// `text` with the accessor line of each key in `keys` moved to its new
/// key; a key is left out of the result when the file does not export
/// it for the renamed binding, or already exports the new key.
fn rekeyed<'r>(text: &str, keys: &[(&'r str, &'r str)]) -> (String, HashMap<&'r str, &'r str>) {
    let mut moved = HashMap::new();
    let mut out = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        let hit = keys.iter().find(|(from, to)| {
            line.starts_with(&accessor_prefix(from))
                && line.contains(&format!("get: () => {to},"))
                && !text.lines().any(|l| l.starts_with(&accessor_prefix(to)))
        });
        match hit {
            Some((from, to)) => {
                out.push_str(&accessor_prefix(to));
                out.push_str(&line[accessor_prefix(from).len()..]);
                moved.insert(*from, *to);
            }
            None => out.push_str(line),
        }
    }
    (out, moved)
}

/// `text` with every `<alias>.<from>` read of a moved key renamed, or
/// None when the text needs a rename and does not parse.
fn reads_renamed(
    text: &str,
    by_alias: &HashMap<&str, &HashMap<&str, &str>>,
) -> Option<Option<String>> {
    let mentioned = by_alias
        .iter()
        .any(|(alias, keys)| keys.keys().any(|k| text.contains(&format!("{alias}.{k}"))));
    if !mentioned {
        return Some(None);
    }
    let allocator = Allocator::default();
    let Ok(ingest) = parse_or_err(&allocator, text) else {
        return None;
    };
    let scoping = ingest.semantic().scoping();
    let mut edits: Vec<(usize, usize, &str)> = Vec::new();
    for node in ingest.semantic().nodes().iter() {
        let AstKind::StaticMemberExpression(m) = node.kind() else {
            continue;
        };
        let Expression::Identifier(object) = &m.object else {
            continue;
        };
        let Some(to) = by_alias
            .get(object.name.as_str())
            .and_then(|keys| keys.get(m.property.name.as_str()))
        else {
            continue;
        };
        let top_level = scoping
            .get_reference(object.reference_id())
            .symbol_id()
            .is_some_and(|s| scoping.symbol_scope_id(s) == scoping.root_scope_id());
        if top_level {
            let span = m.property.span;
            edits.push((span.start as usize, span.end as usize, to));
        }
    }
    if edits.is_empty() {
        return Some(None);
    }
    edits.sort_unstable();
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    for (start, end, to) in edits {
        out.push_str(&text[at..start]);
        out.push_str(to);
        at = end;
    }
    out.push_str(&text[at..]);
    Some(Some(out))
}

/// What [`follow_export_renames`] did.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Followed {
    /// The files whose text changed, in path order.
    pub changed: Vec<(String, String)>,
    /// The (file, old key) of every export key that moved.
    pub moved: BTreeSet<(String, String)>,
}

/// `files` is every JavaScript file of the tree by its path from the
/// tree root. Nothing changes when a file that reads a moved key does
/// not parse.
pub fn follow_export_renames(
    renames: &[PostSplitRename],
    files: &BTreeMap<String, String>,
) -> Followed {
    let mut wanted: BTreeMap<&str, Vec<(&str, &str)>> = BTreeMap::new();
    for r in renames.iter().filter(|r| r.top_level) {
        wanted
            .entry(r.file.as_str())
            .or_default()
            .push((r.from_name.as_str(), r.to_name.as_str()));
    }
    let mut changed: BTreeMap<&str, String> = BTreeMap::new();
    let mut moved: HashMap<&str, HashMap<&str, &str>> = HashMap::new();
    for (file, keys) in &wanted {
        let Some(text) = files.get(*file) else {
            continue;
        };
        let (text, keys) = rekeyed(text, keys);
        if !keys.is_empty() {
            changed.insert(file, text);
            moved.insert(file, keys);
        }
    }
    if moved.is_empty() {
        return Followed::default();
    }
    for (file, text) in files {
        let aliases = parse_require_aliases(file, text);
        let by_alias: HashMap<&str, &HashMap<&str, &str>> = aliases
            .iter()
            .filter_map(|(module, alias)| Some((alias.as_str(), moved.get(module.as_str())?)))
            .collect();
        if by_alias.is_empty() {
            continue;
        }
        let current = changed
            .get(file.as_str())
            .map_or(text.as_str(), String::as_str);
        match reads_renamed(current, &by_alias) {
            None => return Followed::default(),
            Some(None) => {}
            Some(Some(text)) => {
                changed.insert(file, text);
            }
        }
    }
    Followed {
        changed: changed
            .into_iter()
            .map(|(f, t)| (f.to_string(), t))
            .collect(),
        moved: moved
            .iter()
            .flat_map(|(f, keys)| keys.keys().map(|k| (f.to_string(), k.to_string())))
            .collect(),
    }
}

#[cfg(test)]
mod export_names_test;
