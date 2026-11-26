//! Library imports named per FILE, the way people write them (finding #91,
//! Andrew 2026-10-06: "I do not like the import numbering").
//!
//! The naming stage names a top-level `X = require("<package>")` binding by
//! rule (`naming::plumbing::name_library_imports`, finding #89) while the
//! program is still ONE scope, so every name must be unique bundle-wide:
//! `pathModule`, `pathModule2`, … `pathModule431`. Those names survive the
//! split, so a file whose only path import is `pathModule54` read that way.
//! After the split each file is its own scope, and this pass gives each
//! file's library imports the package's own identifier (`naming::plumbing::
//! conventional_import_name`): `path`, `fs`, `fsPromises`, `childProcess`.
//!
//! The rule, per file, in source order of the require call:
//! - the conventional name, when it is free in THIS file — a legal rename
//!   target (never a reserved word or a language global such as `process`),
//!   not a CommonJS context name (`module`/`exports`/`require`), not on the
//!   never-rename lists, not declared ANYWHERE in the file (any scope), and
//!   not read as a free name (a file that reads a global `os` gets
//!   `osModule`); `crypto` is on the legality owner's global list, so it is
//!   always `cryptoModule`;
//! - else that name + `Module` (`pathModule`) under the same test;
//! - a CommonJS context name has no plain form: `module` → `nodeModule`;
//! - else the binding keeps its bundle name (recorded as kept). NEVER a
//!   number.
//!
//! With a previous release, the imports that had a package name in the
//! previous copy of the file come first, so an import new to the file takes
//! no name from them, and each keeps the name it had there unless a local
//! of that name can see its reads.
//!
//! What counts as a library import here is the naming pass's own shape —
//! a FILE-level binding whose only write is the call (`naming::plumbing::
//! sole_write`) — over the split tree's two require spellings: the free
//! `require("<package>")`, and the runnable tree's `(0, B.require)("…")`
//! where `B` is the file's binding of the bundle context file
//! (`.humanify/_bundle*.js`).
//!
//! Every rename goes through the validated applier and the file's text is
//! rewritten at identifier positions only (`finish::reconcile::
//! rewrite_renamed_text`, shorthand-aware), then must re-parse to the same
//! program (`file_signature`) or the file ships as it was. An EXPORTED
//! import keeps its export key: the key is a string the rewrite never
//! touches, so `Object.defineProperty(module.exports, "pathModule54",
//! { get: () => path })` stays the module's API.
//!
//! The split ledger and `.humanify/humanified.js` keep the BUNDLE names on
//! purpose: the next release's naming, placement (`nameToFiles`) and
//! emit alignment (`emitNames`) all read names in the bundle's space, and
//! this pass re-derives the file names from each file's own content and
//! the previous copy of the file every run.

use std::collections::{HashMap, HashSet};

use oxc_allocator::Allocator;
use oxc_ast::ast::{Argument, Expression};
use oxc_semantic::{Semantic, SymbolId};
use oxc_span::GetSpan;

use crate::babel_view::{DiffLines, unparen};
use crate::error::FileError;
use crate::naming::plumbing::{conventional_import_name, require_specifier, sole_write};
use crate::naming::report::diagnostics::ExtraText;
use crate::output_tree::OutputTree;
use crate::place::layout::METADATA_DIR;
use crate::rename::eligibility::{Eligibility, NeverRename};
use crate::rename::name_profile::NameProfile;
use crate::rename::validated::target::is_valid_rename_target;
use crate::rename::validated::{RenameRequest, RenameState, TrailSpec};
use crate::toolchain::is_commonjs_context_name;
use crate::trail::{Anchor, Attempt, Outcome, StrategyTrail, Tier};

use super::reconcile::rewrite_renamed_text;
use super::relink::parse_or_err;
use super::vendor_inherit::file_signature;

#[cfg(test)]
mod library_names_test;

/// What the pass did to one file.
#[derive(Debug, Default)]
pub struct FileLibraryNames {
    /// The rewritten text — None when nothing changed.
    pub text: Option<String>,
    /// `(bundle name, file name)` per binding renamed, in source order.
    pub named: Vec<(String, String)>,
    /// `(bundle name, why)` per library import left as it was.
    pub kept: Vec<(String, &'static str)>,
    /// The trail rows (`Tier::LibraryImport`), labelled by the file.
    pub trail: Option<ExtraText>,
}

/// The previous release's copy of a file.
pub struct PriorFile<'a> {
    pub file: &'a str,
    pub text: std::borrow::Cow<'a, str>,
    /// Bundle name to the previous files that declared it.
    pub name_to_files: &'a HashMap<String, Vec<String>>,
}

impl PriorFile<'_> {
    /// Did the previous copy of the file declare this bundle name?
    fn declared(&self, name: &str) -> bool {
        let homes = self.name_to_files.get(name);
        homes.is_some_and(|files| files.iter().any(|f| f == self.file))
    }
}

/// The previous release's tree, as this pass reads it.
#[derive(Clone, Copy)]
pub struct PriorNames<'a> {
    pub files: &'a crate::prior_tree::PriorFiles,
    /// Bundle name to the files that declared it.
    pub name_to_files: &'a HashMap<String, Vec<String>>,
}

/// What the pass did to the tree.
#[derive(Debug, Default)]
pub struct LibraryNamesReport {
    /// Bindings renamed.
    pub named: usize,
    /// Files rewritten.
    pub files: usize,
    /// `(why, count)` of the library imports kept, by reason.
    pub kept: Vec<(&'static str, usize)>,
    /// Per-file trail rows, in file order.
    pub trail: Vec<ExtraText>,
}

impl LibraryNamesReport {
    /// The run log's line.
    pub fn message(&self) -> String {
        let kept: usize = self.kept.iter().map(|(_, n)| n).sum();
        let reasons: Vec<String> = self.kept.iter().map(|(r, n)| format!("{r} x{n}")).collect();
        format!(
            "Library imports: named {} binding(s) by package across {} file(s){}",
            self.named,
            self.files,
            if kept > 0 {
                format!(" ({kept} kept: {})", reasons.join(", "))
            } else {
                String::new()
            }
        )
    }
}

/// One library import of a file.
struct Import {
    symbol: SymbolId,
    /// The require call's start (source order).
    at: u32,
    spec: String,
}

/// The names the pass may try for `spec`, in order.
fn candidates(spec: &str) -> Vec<String> {
    let Some(base) = conventional_import_name(spec) else {
        return Vec::new();
    };
    if is_commonjs_context_name(&base) {
        let mut chars = base.chars();
        let head = chars.next().map(|c| c.to_ascii_uppercase());
        return vec![format!(
            "node{}{}",
            head.unwrap_or_default(),
            chars.as_str()
        )];
    }
    vec![base.clone(), format!("{base}Module")]
}

/// Can `name` stand as a file-level binding in a file that already uses
/// the names in `taken`? A global the file reads is in `taken`; the
/// name-legality owner's globals (`process`, `Buffer`, `crypto`) are never
/// targets, read or not (`is_valid_rename_target`, the naming stage's own
/// rule).
fn is_free(name: &str, taken: &HashSet<String>, eligible: &Eligibility) -> bool {
    is_valid_rename_target(name)
        && !is_commonjs_context_name(name)
        && eligible.is_eligible(name)
        && !taken.contains(name)
}

/// The bundle context file's binding: `require("<…>/.humanify/_bundle*.js")`.
fn is_bundle_context(semantic: &Semantic<'_>, symbol: SymbolId) -> bool {
    let Some(spec) = sole_write(semantic, symbol).and_then(require_specifier) else {
        return false;
    };
    let mut parts = spec.rsplit('/');
    let file = parts.next().unwrap_or("");
    parts.next() == Some(METADATA_DIR) && file.starts_with("_bundle") && file.ends_with(".js")
}

/// The specifier of a split file's library require: the free
/// `require("<spec>")`, or `(0, B.require)("<spec>")` / `B.require(…)`
/// through the file's bundle context binding `B`.
fn file_require_specifier<'a>(
    semantic: &Semantic<'_>,
    value: &'a Expression<'a>,
) -> Option<&'a str> {
    let Expression::CallExpression(call) = unparen(value) else {
        return None;
    };
    if call.optional || call.arguments.len() != 1 {
        return None;
    }
    let Argument::StringLiteral(spec) = &call.arguments[0] else {
        return None;
    };
    let scoping = semantic.scoping();
    let resolved = |id: &oxc_ast::ast::IdentifierReference<'_>| {
        id.reference_id
            .get()
            .and_then(|r| scoping.get_reference(r).symbol_id())
    };
    let callee = match unparen(&call.callee) {
        Expression::SequenceExpression(seq) if seq.expressions.len() == 2 => {
            match &seq.expressions[0] {
                Expression::NumericLiteral(n) if n.value == 0.0 => unparen(&seq.expressions[1]),
                _ => return None,
            }
        }
        other => other,
    };
    let ok = match callee {
        Expression::Identifier(id) => id.name == "require" && resolved(id).is_none(),
        Expression::StaticMemberExpression(m) => {
            m.property.name == "require"
                && matches!(unparen(&m.object), Expression::Identifier(b)
                if resolved(b).is_some_and(|s| {
                    scoping.symbol_scope_id(s) == scoping.root_scope_id()
                        && is_bundle_context(semantic, s)
                }))
        }
        _ => false,
    };
    ok.then_some(spec.value.as_str())
}

/// The file-level library imports, in source order of the require call.
fn library_imports(semantic: &Semantic<'_>) -> Vec<Import> {
    let scoping = semantic.scoping();
    let mut out: Vec<Import> = scoping
        .iter_bindings_in(scoping.root_scope_id())
        .filter(|&s| scoping.symbol_redeclarations(s).is_empty())
        .filter_map(|symbol| {
            let value = sole_write(semantic, symbol)?;
            let spec = file_require_specifier(semantic, value)?;
            conventional_import_name(spec)?;
            Some(Import {
                symbol,
                at: value.span().start,
                spec: spec.to_string(),
            })
        })
        .collect();
    out.sort_by_key(|i| i.at);
    out
}

/// What the previous copy of the file says about this file's imports.
#[derive(Default)]
struct Previous {
    /// The imports that had a package name there: declared there, and not
    /// written there under their bundle name.
    held: HashSet<SymbolId>,
    /// The package name each of them had, known when the two copies hold
    /// equally many such imports of the package (paired in source order).
    names: HashMap<SymbolId, String>,
}

fn previous(prior: &PriorFile<'_>, semantic: &Semantic<'_>, imports: &[Import]) -> Previous {
    let allocator = Allocator::default();
    let Ok(ingest) = parse_or_err(&allocator, &prior.text) else {
        return Previous::default();
    };
    let before = ingest.semantic();
    let prior_imports = library_imports(before);
    let prior_name = |i: &Import| before.scoping().symbol_name(i.symbol);
    let written: HashSet<&str> = prior_imports.iter().map(prior_name).collect();
    let holders: Vec<&Import> = imports
        .iter()
        .filter(|i| {
            let name = semantic.scoping().symbol_name(i.symbol);
            prior.declared(name)
                && (!written.contains(name) || candidates(&i.spec).iter().any(|c| c == name))
        })
        .collect();
    let mut names = HashMap::new();
    let mut specs: Vec<&str> = holders.iter().map(|i| i.spec.as_str()).collect();
    specs.sort_unstable();
    specs.dedup();
    for spec in specs {
        let wanted = candidates(spec);
        let had: Vec<&str> = prior_imports
            .iter()
            .filter(|i| i.spec == spec)
            .map(prior_name)
            .filter(|n| wanted.iter().any(|c| c == n))
            .collect();
        let now: Vec<&&Import> = holders.iter().filter(|i| i.spec == spec).collect();
        if had.len() == now.len() {
            names.extend(now.iter().zip(had).map(|(i, n)| (i.symbol, n.to_string())));
        }
    }
    Previous {
        held: holders.iter().map(|i| i.symbol).collect(),
        names,
    }
}

/// May a previous holder keep `name`? A local elsewhere in the file may
/// share it: the rename itself refuses a local that can see the import's
/// reads.
pub(super) fn may_keep(name: &str, semantic: &Semantic<'_>, eligible: &Eligibility) -> bool {
    let read_free = semantic
        .scoping()
        .root_unresolved_references()
        .keys()
        .any(|k| k.as_str() == name);
    is_valid_rename_target(name)
        && !is_commonjs_context_name(name)
        && eligible.is_eligible(name)
        && !read_free
}

/// Every name the file uses: each declared name (any scope) and each name
/// it reads free.
pub(super) fn names_in_use(semantic: &Semantic<'_>) -> HashSet<String> {
    let scoping = semantic.scoping();
    let mut taken: HashSet<String> = scoping.symbol_names().map(str::to_string).collect();
    taken.extend(
        scoping
            .root_unresolved_references()
            .keys()
            .map(|k| k.to_string()),
    );
    taken
}

/// Name one file's library imports. Infallible: a file that does not parse,
/// or whose rewrite is not the same program, ships as it was.
pub fn name_library_imports_in_file(
    file: &str,
    text: &str,
    prior: Option<&PriorFile<'_>>,
    eligible: &Eligibility,
    profile: NameProfile,
) -> FileLibraryNames {
    let mut out = FileLibraryNames::default();
    let allocator = Allocator::default();
    let Ok(ingest) = parse_or_err(&allocator, text) else {
        return out;
    };
    let semantic = ingest.semantic();
    let mut imports = library_imports(semantic);
    if imports.is_empty() {
        return out;
    }
    let previous = prior.map(|p| previous(p, semantic, &imports));
    let previous = previous.unwrap_or_default();
    imports.sort_by_key(|i| (!previous.held.contains(&i.symbol), i.at));
    let mut taken = names_in_use(semantic);
    taken.extend(previous.names.values().cloned());
    let mut state = RenameState::with_trail(
        semantic,
        Anchor::Generated,
        StrategyTrail::enabled(),
        profile,
    );
    for import in &imports {
        let current = semantic.scoping().symbol_name(import.symbol).to_string();
        let Some(binding) = state.view().binding_of_symbol(import.symbol) else {
            out.kept.push((current, "no-binding"));
            continue;
        };
        let kept = previous.names.get(&import.symbol);
        let kept = kept.filter(|n| may_keep(n, semantic, eligible)).cloned();
        let pick = kept.or_else(|| {
            candidates(&import.spec)
                .into_iter()
                .find(|c| *c == current || is_free(c, &taken, eligible))
        });
        let Some(name) = pick else {
            out.kept.push((current, "no-free-name"));
            continue;
        };
        if name == current {
            continue;
        }
        let attempt = state.attempt_validated_rename(
            RenameRequest {
                scope: state.scope_of_binding(binding),
                old_name: &current,
                new_name: &name,
                expected: Some(binding),
            },
            TrailSpec::CallerRecords {
                tier: Tier::LibraryImport,
            },
        );
        if !attempt.applied {
            let why = attempt.reason.map_or("refused", |r| r.as_str());
            let row = Attempt::new(Tier::LibraryImport, Outcome::Rejected)
                .reason(why)
                .proposed(&name);
            state.record(binding, &current, row, true);
            out.kept.push((current, "refused"));
            continue;
        }
        let row = Attempt::new(Tier::LibraryImport, Outcome::Applied).proposed(&name);
        state.record(binding, &current, row, true);
        taken.insert(name.clone());
        out.named.push((current, name));
    }
    let rows = state.trail().entries().to_vec();
    out.trail = (!rows.is_empty()).then(|| ExtraText {
        file: file.to_string(),
        text: text.to_string(),
        rows,
    });
    if out.named.is_empty() {
        return out;
    }
    let rewritten = rewrite_renamed_text(semantic, &state, text, &DiffLines::new(text));
    if file_signature(&rewritten).is_none() || file_signature(&rewritten) != file_signature(text) {
        let named = std::mem::take(&mut out.named);
        out.kept.extend(
            named
                .into_iter()
                .map(|(from, _)| (from, "not-same-program")),
        );
        return out;
    }
    out.text = Some(rewritten);
    out
}

/// The split's own JavaScript files: everything outside `.humanify/`.
pub(super) fn split_js_files<'a>(files: &[&'a str]) -> Vec<&'a str> {
    let metadata = format!("{METADATA_DIR}/");
    let own = |f: &&str| f.ends_with(".js") && !f.starts_with(&metadata);
    files.iter().copied().filter(own).collect()
}

/// Name the library imports of every split file in the output tree
/// (`files` are the paths the split wrote; `.humanify/` is skipped).
pub fn name_library_imports_in_tree(
    tree: &mut OutputTree,
    files: &[&str],
    prior: Option<PriorNames<'_>>,
    never_rename: NeverRename,
    profile: NameProfile,
) -> Result<LibraryNamesReport, FileError> {
    let eligible = Eligibility::new(never_rename);
    let files = split_js_files(files);
    let reading: &OutputTree = tree;
    let works = crate::par::map_ordered(&files, |file| {
        let text = reading.read_lossy(&reading.root().join(file))?;
        let prior = prior.and_then(|p| {
            Some(PriorFile {
                file,
                text: p.files.text_lossy(file)?,
                name_to_files: p.name_to_files,
            })
        });
        Ok::<_, FileError>(name_library_imports_in_file(
            file,
            &text,
            prior.as_ref(),
            &eligible,
            profile,
        ))
    });
    let mut report = LibraryNamesReport::default();
    for (file, work) in files.iter().zip(works) {
        let work = work?;
        for (_, why) in &work.kept {
            match report.kept.iter_mut().find(|(r, _)| r == why) {
                Some((_, n)) => *n += 1,
                None => report.kept.push((why, 1)),
            }
        }
        report.trail.extend(work.trail);
        if let Some(text) = work.text {
            tree.put_rel(file, text);
            report.named += work.named.len();
            report.files += 1;
        }
    }
    Ok(report)
}
