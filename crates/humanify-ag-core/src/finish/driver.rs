//! The finishing stage over the output tree, in the TS's order — the driver
//! half of `src/commands/unified.ts` (`finishSplitOutput` →
//! `reconcilePostSplit` → `carryIntoBundle`) plus `relinkBunModules`.
//!
//! The tree, the ledger and `.humanify/humanified.js` are already in the
//! [`OutputTree`] (the TS's `committed` point); everything here rewrites
//! the tree's files in memory, and the shell writes the result once.
//! [`finish_split_output`] then [`reconcile_post_split`] — the reconcile
//! runs only when the finish succeeded (a throw ends the TS stage).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::error::FileError;
use crate::modules::vendor_names::BunModulesManifest;
use crate::output_tree::{OutputTree, TreeFile};
use crate::place::layout::METADATA_DIR;
use crate::rename::eligibility::{Eligibility, NeverRename};
use crate::rename::name_profile::NameProfile;
use crate::unpack::bun::{BUN_MODULES_MANIFEST, VENDOR_DIR};

use super::carry::{CarryResult, carry_renames_into_bundle};
use super::reconcile::{PostSplitInput, PostSplitResult, post_split_reconcile};

use super::relink::{
    FactoryLookup, RelinkError, VendorBridge, relink_factory_references, wrap_extracted_factory,
};
use super::scaffold::{detect_external_packages, write_runnable_scaffold};
use super::using::UsingTreeError;
use super::vendor_inherit::VendorBodyInheritor;

#[cfg(test)]
mod driver_test;

/// The finishing kill switches (`--disable` names).
#[derive(Clone, Copy, Debug, Default)]
pub struct FinishSwitches {
    /// `vendor-inherit`.
    pub vendor_inherit_disabled: bool,
    /// `post-split-reconcile`.
    pub post_split_reconcile_disabled: bool,
}

/// What the stage is given.
pub struct FinishInput<'a> {
    /// The runnable emit's file map keys, in emission order — None when the
    /// review tree was written instead (`--split-pure` or a decline).
    pub runnable: Option<&'a [String]>,
    /// The prior release's tree (`--prior-version`), read by the shell.
    pub prior: Option<&'a crate::prior_tree::PriorTree>,
    /// Bundle name to the previous release's files that declared it.
    pub prior_names: Option<&'a std::collections::HashMap<String, Vec<String>>>,
    /// The input bundle (its directory resolves installed versions).
    pub input_file: &'a Path,
    /// The working directory the shell read: a relative `input_file`
    /// resolves against it.
    pub working_dir: &'a Path,
    pub switches: FinishSwitches,
    /// The run's resolved vendor bridges (finding #60): the vendor bodies'
    /// app-scope reads as (raw name → owner file + live accessor), which
    /// the split computed from the manifest's capture records. Empty for
    /// review trees, declines and bundles without captures.
    pub bridges: Vec<VendorBridge>,
    /// The run's minifier name profile (the post-split reconcile's
    /// shape questions; `rename::name_profile`).
    pub name_profile: NameProfile,
    /// The run's never-rename lists (the post-split reconcile's
    /// eligibility — the SAME lists the naming stage used; it hard-coded
    /// Bun's until 2026-10-04, docs/plugin-spec.md I21).
    pub never_rename: NeverRename,
    /// The run's interop helpers (the relink's helper file —
    /// `toolchain::InteropHelpers`, P8).
    pub interop: crate::toolchain::InteropHelpers,
    /// The run's bundle layout (the bundle carry's wrapper body —
    /// `toolchain::BundleLayout`, P9).
    pub layout: crate::toolchain::BundleLayout,
    /// The vendor record THIS run's unpack adapter wrote (None when it
    /// wrote none) — the re-link's only source (toolchain review R6).
    pub vendor_record: Option<&'a BunModulesManifest>,
    /// Every path the split wrote (the tree), so a vendor-folder file the
    /// split placed is never mistaken for an earlier run's leftover.
    pub split_files: Vec<&'a str>,
    /// The embedded files the linked input reads by path (a Bun
    /// code-split app's `file` imports, exp217): a runnable tree carries
    /// each under `embedded/`. Empty for every other input.
    pub embedded_files: &'a [crate::unpack::chunk_link::EmbeddedFile],
    /// The system `diff` the post-split reconcile aligns with, handed in
    /// by the shell (`humanify_ag_io::diff`): core runs no process.
    pub differ: &'a dyn crate::naming::reconcile::hunks::Differ,
}

/// The vendor record as the finish reads it (`BunModulesManifest`).
struct Manifest {
    runtime_file: Option<String>,
    /// (fileName, runtimeIdentifier), manifest order.
    factories: Vec<(String, Option<String>)>,
}

/// The vendor record THIS run's adapter wrote, as the re-link reads it:
/// None when the run wrote none, when its stamp is not a registered
/// vendor-record adapter's (`UnpackAdapter::of_vendor_record_stamp` — a new
/// vendor-extracting adapter is accepted by registering it, never by
/// editing a list here; docs/plugin-spec.md I14), or when it is
/// factory-less.
///
/// The record comes from the run, never from the output folder: the finish
/// used to read whatever `vendor/_bun-modules.json` sat there, and nothing
/// clears that folder, so a run into the folder of an EARLIER run re-linked
/// the earlier run's vendor files (toolchain review R6).
fn this_runs_manifest(record: Option<&BunModulesManifest>) -> Option<Manifest> {
    let record = record?;
    crate::unpack::UnpackAdapter::of_vendor_record_stamp(record.adapter)?;
    let factories: Vec<(String, Option<String>)> = record
        .factories
        .iter()
        .map(|e| (e.file_name.clone(), e.runtime_identifier.clone()))
        .collect();
    if factories.is_empty() {
        return None;
    }
    Some(Manifest {
        runtime_file: record.runtime_file.clone(),
        factories,
    })
}

/// Files under the output folder's `vendor/` that THIS run did not write —
/// neither its vendor record (and the files it lists) nor the split's
/// tree. They can only be left from an earlier run into the same folder.
/// The finish never re-links, rewrites or deletes them (they may be the
/// user's); it names them so nobody mistakes them for this run's output.
/// A folder that cannot be listed is skipped.
fn leftover_vendor_files(
    tree: &OutputTree,
    record: Option<&BunModulesManifest>,
    split_files: &[&str],
) -> Vec<String> {
    let output_dir = tree.root();
    let mut written: HashSet<String> = split_files.iter().map(|f| (*f).to_string()).collect();
    if let Some(r) = record {
        written.insert(format!("{VENDOR_DIR}/{BUN_MODULES_MANIFEST}"));
        written.extend(r.factories.iter().map(|e| e.file_name.clone()));
    }
    let mut leftovers = Vec::new();
    let mut stack = vec![output_dir.join(VENDOR_DIR)];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = tree.list(&dir) else {
            continue;
        };
        for entry in entries {
            let path = dir.join(&entry.name);
            if entry.leads_to_dir {
                stack.push(path);
                continue;
            }
            let rel = crate::libdetect::relative_posix(output_dir, &path);
            if !written.contains(&rel) {
                leftovers.push(rel);
            }
        }
    }
    leftovers.sort();
    leftovers
}

/// The loud line for [`leftover_vendor_files`].
fn leftover_warning(leftovers: &[String]) -> String {
    let shown: Vec<&str> = leftovers.iter().take(5).map(String::as_str).collect();
    format!(
        "WARNING — {} file(s) under {VENDOR_DIR}/ were not written by this run (left from an \
         earlier run into this output folder?); they are not part of this tree and were left \
         untouched, not re-linked — use an empty output folder for a clean tree: {}{}",
        leftovers.len(),
        shown.join(", "),
        if leftovers.len() > shown.len() {
            ", …"
        } else {
            ""
        }
    )
}

/// `runnableEntryFile(files)`: the first key matching `/^_*index\.js$/`.
pub fn runnable_entry_file(keys: &[String]) -> String {
    keys.iter()
        .find(|k| {
            k.strip_suffix("index.js")
                .is_some_and(|lead| lead.bytes().all(|b| b == b'_'))
        })
        .cloned()
        .unwrap_or_else(|| "index.js".into())
}

/// Why the finishing stage failed (the TS's "Post-split step failed").
#[derive(Debug, thiserror::Error)]
pub enum FinishError {
    /// Reading, writing or making a file or directory failed.
    #[error(transparent)]
    File(#[from] FileError),
    /// One file of the tree could not be re-linked.
    #[error("{file}: {source}")]
    Relink { file: String, source: RelinkError },
    /// The `using` desugar failed.
    #[error(transparent)]
    Using(#[from] UsingTreeError),
    /// The split ledger is not valid JSON.
    #[error("{}: {source}", path.display())]
    Ledger {
        path: PathBuf,
        source: serde_json::Error,
    },
}

/// What the stage did (the TS's progress lines, in order).
#[derive(Debug, Default)]
pub struct FinishReport {
    pub messages: Vec<String>,
    /// The per-file library-import names (finding #91).
    pub library_names: Option<super::library_names::LibraryNamesReport>,
}

/// `relinkBunModules(outputDir, manifest, splitFiles, { priorRoot })`.
fn relink_bun_modules(
    tree: &mut OutputTree,
    manifest: &Manifest,
    split_files: &[String],
    prior: Option<&crate::prior_tree::PriorFiles>,
    bridges: &[VendorBridge],
    interop: crate::toolchain::InteropHelpers,
    report: &mut FinishReport,
) -> Result<(), FinishError> {
    let lookup: FactoryLookup = manifest
        .factories
        .iter()
        .filter_map(|(file, id)| id.clone().map(|id| (id, file.clone())))
        .collect();
    tree.put_rel(&interop.runtime_file(), interop.relink_runtime());
    let mut inherit = prior.map(VendorBodyInheritor::new);
    let mut bridged_reads = 0usize;
    for (file_name, _) in &manifest.factories {
        let abs = tree.root().join(file_name);
        let body = tree.read_lossy(&abs)?;
        let (rendered, bridged) = wrap_extracted_factory(
            &body, file_name, &lookup, bridges, interop,
        )
        .map_err(|source| FinishError::Relink {
            file: file_name.clone(),
            source,
        })?;
        bridged_reads += bridged;
        let bytes = match inherit.as_mut() {
            Some(i) => i.bytes_for(file_name, rendered),
            None => rendered,
        };
        tree.put(abs, bytes);
    }
    if bridged_reads > 0 {
        report.messages.push(format!(
            "Vendor bridge: {bridged_reads} app-scope read{} resolved through {} owner binding{}",
            if bridged_reads == 1 { "" } else { "s" },
            bridges.len(),
            if bridges.len() == 1 { "" } else { "s" },
        ));
    }
    if let Some(i) = &inherit {
        let s = i.stats();
        report.messages.push(format!(
            "vendor bodies: inherited {}/{} unchanged libraries from the prior release",
            s.inherited, s.considered
        ));
    }
    for rel in split_files {
        let abs = tree.root().join(rel);
        let code = tree.read_lossy(&abs)?;
        let out =
            relink_factory_references(&code, rel, &lookup).map_err(|e| FinishError::Relink {
                file: rel.clone(),
                source: e.into(),
            })?;
        tree.put(abs, out);
    }
    if let Some(runtime) = &manifest.runtime_file {
        let path = tree.root().join(runtime);
        tree.remove(&path);
    }
    Ok(())
}

/// `finishSplitOutput`: re-link, then (runnable only) the `using`
/// desugar and the scaffold. Returns whether a Bun re-link ran.
pub fn finish_split_output(
    input: &FinishInput<'_>,
    tree: &mut OutputTree,
    report: &mut FinishReport,
) -> Result<bool, FinishError> {
    let manifest = this_runs_manifest(input.vendor_record);
    let leftovers = leftover_vendor_files(tree, input.vendor_record, &input.split_files);
    if !leftovers.is_empty() {
        report.messages.push(leftover_warning(&leftovers));
    }
    if let (Some(runnable), Some(manifest)) = (input.runnable, manifest.as_ref()) {
        // The inherit reads a prior tree that has a vendor manifest.
        let inherit_from = if input.switches.vendor_inherit_disabled {
            None
        } else {
            input
                .prior
                .filter(|p| p.has_vendor_manifest())
                .map(|p| &p.files)
        };
        {
            // The finish's constituents were dark (docs/perf-inventory.md
            // item 3: 40 s serial per with-prior hop, no internal spans) —
            // a span each for relink, the using desugar, the scaffold,
            // then (in reconcile_post_split) the ledger read, the
            // per-file reconcile, the apply and the carry.
            let _ph = crate::profiling::phase("split:finish:relink");
            relink_bun_modules(
                tree,
                manifest,
                runnable,
                inherit_from,
                &input.bridges,
                input.interop,
                report,
            )?;
        }
        report.messages.push(format!(
            "Re-linked {} Bun factory module(s) into the runnable graph",
            manifest.factories.len()
        ));
    }
    if input.runnable.is_none()
        && let Some(runtime) = manifest.as_ref().and_then(|m| m.runtime_file.as_ref())
    {
        let path = tree.root().join(runtime);
        tree.remove(&path);
    }
    if let Some(runnable) = input.runnable {
        let desugared = {
            let _ph = crate::profiling::phase("split:finish:desugar-using");
            super::using::desugar_using_in_tree(tree)?
        };
        report
            .messages
            .push(super::using::desugar_summary(tree.root(), desugared));
        let entry = runnable_entry_file(runnable);
        let externals = {
            let _ph = crate::profiling::phase("split:finish:scaffold");
            let externals = detect_external_packages(tree)?;
            write_runnable_scaffold(
                tree,
                &entry,
                &externals,
                input.input_file.parent(),
                input.working_dir,
            );
            externals
        };
        if !input.embedded_files.is_empty() {
            let copied = super::scaffold::write_embedded_files(tree, input.embedded_files);
            report.messages.push(format!(
                "Embedded files: {copied} copied to {}/ (the linked program reads them by path)",
                crate::unpack::chunk_link::EMBEDDED_DIR
            ));
        }
        let deps = if externals.is_empty() {
            "no external deps".to_string()
        } else {
            let shown: Vec<&str> = externals.iter().take(6).map(String::as_str).collect();
            format!(
                "{} external dep(s): {}{}",
                externals.len(),
                shown.join(", "),
                if externals.len() > 6 { ", …" } else { "" }
            )
        };
        report.messages.push(format!(
            "Runnable scaffold: run.cjs + package.json ({deps}) — `npm install && node run.cjs [arguments]`"
        ));
    }
    Ok(input.runnable.is_some() && manifest.is_some())
}

/// The finishing stage in the TS order (tryStableSplit after the commit):
/// [`finish_split_output`], then — only when it succeeded — the post-split
/// reconcile + bundle carry. Returns whether a Bun re-link ran; an Err is
/// the TS's "Post-split step failed" (the tree stays on disk). The ONE
/// owner of this order: the pipeline and the `finish` verb both call it.
pub fn finish_stage(
    input: &FinishInput<'_>,
    tree: &mut OutputTree,
    report: &mut FinishReport,
) -> Result<(bool, Option<ReconcileReport>), FinishError> {
    let relinked = finish_split_output(input, tree, report)?;
    // Each split file names its own library imports (`path`, not the
    // bundle-wide `pathModule54`; finding #91) on the FINAL text, before
    // the reconcile reads it against a prior tree that went through the
    // same pass.
    let library_names = {
        let _ph = crate::profiling::phase("split:finish:library-names");
        super::library_names::name_library_imports_in_tree(
            tree,
            &input.split_files,
            prior_names(input),
            input.never_rename,
            input.name_profile,
        )?
    };
    report.messages.push(library_names.message());
    report.library_names = Some(library_names);
    let reconciled = reconcile_post_split(
        tree,
        input.prior.map(|p| &p.files),
        input.switches,
        PostSplitRules {
            name_profile: input.name_profile,
            never_rename: input.never_rename,
            differ: input.differ,
        },
        input.layout,
        report,
    )?;
    if let Some(prior) = input.prior {
        let _ph = crate::profiling::phase("split:finish:alias-restore");
        let files = super::alias_restore::restore_import_aliases(
            tree,
            &input.split_files,
            &prior.files,
            input.never_rename,
            input.name_profile,
        )?;
        report.messages.push(format!(
            "Import aliases: {files} file(s) took a previous alias back"
        ));
    }
    Ok((relinked, reconciled))
}

// ---------------------------------------------------------------------------
// The post-split reconcile + bundle carry (`reconcilePostSplit`,
// `carryIntoBundle`)
// ---------------------------------------------------------------------------

/// The previous tree as the library-import pass reads it.
fn prior_names<'a>(input: &FinishInput<'a>) -> Option<super::library_names::PriorNames<'a>> {
    Some(super::library_names::PriorNames {
        files: &input.prior?.files,
        name_to_files: input.prior_names?,
    })
}

/// What the reconcile stage did, for the gate's report.
#[derive(Debug, Default)]
pub struct ReconcileReport {
    pub result: PostSplitResult,
    pub carry: Option<CarryResult>,
}

// default: a file the folder does not hold (or cannot read) is absent
// to the reconcile, which skips it
#[allow(clippy::disallowed_methods)]
fn read_opt(tree: &OutputTree, file: &str) -> Option<String> {
    tree.read(&tree.root().join(file))
        .ok()
        .map(|b| String::from_utf8_lossy(&b).into_owned())
}

/// What the post-split reconcile renames with: the run's minifier name
/// profile and its lists of names left alone (the SAME the naming stage
/// used), and the system `diff` the shell hands in.
#[derive(Clone, Copy)]
pub struct PostSplitRules<'r> {
    pub name_profile: NameProfile,
    pub never_rename: NeverRename,
    pub differ: &'r dyn crate::naming::reconcile::hunks::Differ,
}

/// `reconcilePostSplit(opts, ledger, isEligible)` over the output tree:
/// the ledger is `.humanify/split-ledger.json` (rewritten when a file
/// changed), the bundle `.humanify/humanified.js`.
pub fn reconcile_post_split(
    tree: &mut OutputTree,
    prior: Option<&crate::prior_tree::PriorFiles>,
    switches: FinishSwitches,
    rules: PostSplitRules<'_>,
    layout: crate::toolchain::BundleLayout,
    report: &mut FinishReport,
) -> Result<Option<ReconcileReport>, FinishError> {
    let PostSplitRules {
        name_profile,
        never_rename,
        differ,
    } = rules;
    let Some(prior) = prior else {
        return Ok(None);
    };
    let ledger_path = tree.root().join(METADATA_DIR).join("split-ledger.json");
    let (mut ledger, eligible) = {
        let _ph = crate::profiling::phase("split:finish:reconcile-ledger");
        (
            serde_json::from_str::<crate::place::ledger::SplitLedger>(
                &tree.read_lossy(&ledger_path)?,
            )
            .map_err(|source| FinishError::Ledger {
                path: ledger_path.clone(),
                source,
            })?,
            Eligibility::new(never_rename),
        )
    };
    let reading: &OutputTree = tree;
    let read_fresh = |f: &str| read_opt(reading, f);
    let read_prior = |f: &str| prior.text_lossy(f).map(std::borrow::Cow::into_owned);
    let mut result = {
        // The per-file read + compute bulk (the reads interleave with the
        // diffing inside post_split_reconcile — one span spans it all).
        let _ph = crate::profiling::phase("split:finish:reconcile");
        post_split_reconcile(PostSplitInput {
            ledger: &mut ledger,
            read_fresh: &read_fresh,
            read_prior: &read_prior,
            eligible: &eligible,
            name_profile,
            disabled: switches.post_split_reconcile_disabled,
            differ,
        })
    };
    if let Some(warning) = super::reconcile::diff_failure_warning(&result) {
        report.messages.push(warning);
    }
    if result.changed.is_empty() {
        report.messages.push(format!(
            "Post-split reconcile: no changes (considered {} file(s))",
            result.stats.considered
        ));
        return Ok(Some(ReconcileReport {
            result,
            carry: None,
        }));
    }
    {
        let _ph = crate::profiling::phase("split:finish:reconcile-apply");
        for (file, text) in &result.changed {
            tree.put_rel(file, text.as_str());
        }
        tree.put(ledger_path, ledger.to_json());
        follow_exports(tree, &mut result.renames);
    }
    let carry = {
        let _ph = crate::profiling::phase("split:finish:carry");
        carry_into_bundle(tree, &ledger, &result.renames, layout, report)
    };
    report.messages.push(format!(
        "Post-split reconcile: restored {} prior name(s) across {} of {} file(s){}",
        result.renames.len(),
        result.stats.changed,
        result.stats.considered,
        if result.stats.discarded > 0 {
            format!(" ({} discarded)", result.stats.discarded)
        } else {
            String::new()
        }
    ));
    if result.stats.incoherent > 0 {
        report.messages.push(format!(
            "Post-split reconcile: WARNING — {} ledger entr(ies) still name a binding this pass renamed away",
            result.stats.incoherent
        ));
    }
    Ok(Some(ReconcileReport { result, carry }))
}

/// Each renamed top-level binding's export key, and the key's uses in
/// the tree's other files, take the binding's name.
fn follow_exports(tree: &mut OutputTree, renames: &mut [super::reconcile::PostSplitRename]) {
    if !renames.iter().any(|r| r.top_level) {
        return;
    }
    let root = tree.root().to_path_buf();
    let files: std::collections::BTreeMap<String, String> = tree
        .files()
        .filter_map(|(path, file)| {
            let TreeFile::Bytes(bytes) = file else {
                return None;
            };
            let Ok(rel) = path.strip_prefix(&root) else {
                return None;
            };
            let rel = rel.to_str()?;
            (rel.ends_with(".js") && !rel.starts_with(METADATA_DIR))
                .then(|| (rel.to_string(), String::from_utf8_lossy(bytes).into_owned()))
        })
        .collect();
    let followed = super::export_names::follow_export_renames(renames, &files);
    for (file, text) in followed.changed {
        tree.put_rel(&file, text);
    }
    for r in renames.iter_mut().filter(|r| r.top_level) {
        r.key_moved = followed
            .moved
            .contains(&(r.file.clone(), r.from_name.clone()));
    }
}

/// `carryIntoBundle`: give `.humanify/humanified.js` the names the tree
/// shipped; abstentions are counted by reason.
// default: legacy, untriaged: a Result error becomes a value here
#[allow(clippy::disallowed_methods)]
fn carry_into_bundle(
    tree: &mut OutputTree,
    ledger: &crate::place::ledger::SplitLedger,
    renames: &[super::reconcile::PostSplitRename],
    layout: crate::toolchain::BundleLayout,
    report: &mut FinishReport,
) -> Option<CarryResult> {
    let bundle_path = tree.root().join(METADATA_DIR).join("humanified.js");
    let bundle = tree.read_lossy(&bundle_path).ok()?;
    let carry = match carry_renames_into_bundle(&bundle, ledger, renames, layout) {
        Ok(c) => c,
        Err(_) => return None, // "bundle carry skipped" (debug only)
    };
    if let Some(code) = &carry.code {
        tree.put(bundle_path, code.as_str());
    }
    let missed: usize = carry.abstained.iter().map(|(_, n)| n).sum();
    let mut by_reason = carry.abstained.clone();
    by_reason.sort_by_key(|x| std::cmp::Reverse(x.1));
    let reasons: Vec<String> = by_reason.iter().map(|(r, n)| format!("{r} x{n}")).collect();
    report.messages.push(format!(
        "Post-split reconcile: carried {}/{} name(s) into the bundle for the next release{}",
        carry.carried,
        renames.len(),
        if missed > 0 {
            format!(" ({missed} abstained: {})", reasons.join(", "))
        } else {
            String::new()
        }
    ));
    Some(carry)
}
