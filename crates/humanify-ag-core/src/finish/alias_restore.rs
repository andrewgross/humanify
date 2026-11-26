//! A file's import alias goes back to the one its previous copy used.
//!
//! The emit picks each alias while the file's locals still have the names
//! the naming stage gave them (`emit::import_alias`); the post-split
//! reconcile then gives many locals their previous names back, and an
//! alias that made way for such a local has no reason left. This pass
//! runs after the reconcile and renames the alias back wherever the
//! validated rename allows it. A top-level binding the file does not
//! export, holding the name the previous copy gave the import, is new
//! to that name and makes way for the alias.

use std::collections::HashMap;

use oxc_allocator::Allocator;

use crate::babel_view::DiffLines;
use crate::emit::import_alias::parse_require_aliases;
use crate::error::FileError;
use crate::naming::validation::resolve_conflict;
use crate::output_tree::OutputTree;
use crate::prior_tree::PriorFiles;
use crate::rename::eligibility::{Eligibility, NeverRename};
use crate::rename::name_profile::NameProfile;
use crate::rename::validated::{RenameRequest, RenameState, TrailSpec};
use crate::trail::Anchor;

use super::library_names::{may_keep, names_in_use, split_js_files};
use super::reconcile::rewrite_renamed_text;
use super::relink::parse_or_err;
use super::vendor_inherit::file_signature;

#[cfg(test)]
mod alias_restore_test;

/// The file's text with its previous aliases back; None when none moved.
pub fn restore_aliases_in_file(
    file: &str,
    text: &str,
    prior_text: &str,
    eligible: &Eligibility,
    profile: NameProfile,
) -> Option<String> {
    let mut before: HashMap<String, String> = HashMap::new();
    for (module, alias) in parse_require_aliases(file, prior_text) {
        before.entry(module).or_insert(alias);
    }
    let wanted: Vec<(String, &String)> = parse_require_aliases(file, text)
        .into_iter()
        .filter_map(|(module, alias)| Some((alias, before.get(&module)?)))
        .filter(|(alias, was)| alias != *was)
        .collect();
    if wanted.is_empty() {
        return None;
    }
    let allocator = Allocator::default();
    let Ok(ingest) = parse_or_err(&allocator, text) else {
        return None;
    };
    let semantic = ingest.semantic();
    let mut state = RenameState::new(semantic, Anchor::Generated, profile);
    let scope = state.view().program_scope();
    let mut taken = names_in_use(semantic);
    let mut restored = 0usize;
    for (alias, was) in &wanted {
        if !may_keep(was, semantic, eligible) {
            continue;
        }
        if state.get_binding(scope, was).is_some() && !exports_binding(text, was) {
            let away = resolve_conflict(was, |c| taken.contains(c));
            let moved = state.attempt_validated_rename(
                RenameRequest {
                    scope,
                    old_name: was,
                    new_name: &away,
                    expected: None,
                },
                TrailSpec::Untrailed {
                    why: "alias-restore",
                },
            );
            if !moved.applied {
                continue;
            }
            taken.insert(away);
        }
        let attempt = state.attempt_validated_rename(
            RenameRequest {
                scope,
                old_name: alias,
                new_name: was,
                expected: None,
            },
            TrailSpec::Untrailed {
                why: "alias-restore",
            },
        );
        restored += usize::from(attempt.applied);
    }
    if restored == 0 {
        return None;
    }
    let rewritten = rewrite_renamed_text(semantic, &state, text, &DiffLines::new(text));
    let same = file_signature(&rewritten).is_some_and(|s| Some(s) == file_signature(text));
    same.then_some(rewritten)
}

/// Does the file export the top-level binding `name`?
fn exports_binding(text: &str, name: &str) -> bool {
    let getter = format!("get: () => {name},");
    text.lines()
        .any(|l| l.starts_with("Object.defineProperty(module.exports, ") && l.contains(&getter))
}

/// Restore the aliases of every split file that has a previous copy;
/// returns how many files changed.
pub fn restore_import_aliases(
    tree: &mut OutputTree,
    files: &[&str],
    prior: &PriorFiles,
    never_rename: NeverRename,
    profile: NameProfile,
) -> Result<usize, FileError> {
    let eligible = Eligibility::new(never_rename);
    let files = split_js_files(files);
    let reading: &OutputTree = tree;
    let works = crate::par::map_ordered(&files, |file| {
        let Some(prior_text) = prior.text_lossy(file) else {
            return Ok(None);
        };
        let text = reading.read_lossy(&reading.root().join(file))?;
        Ok::<_, FileError>(restore_aliases_in_file(
            file,
            &text,
            &prior_text,
            &eligible,
            profile,
        ))
    });
    let mut changed = 0;
    for (file, work) in files.iter().zip(works) {
        if let Some(text) = work? {
            tree.put_rel(file, text);
            changed += 1;
        }
    }
    Ok(changed)
}
