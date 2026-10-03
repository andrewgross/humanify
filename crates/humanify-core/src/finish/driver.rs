//! The finishing stage over a tree on disk, in the TS's order — the driver
//! half of `src/commands/unified.ts` (`finishSplitOutput` →
//! `reconcilePostSplit` → `carryIntoBundle`) plus `relinkBunModules`.
//!
//! The tree, the ledger and `.humanify/humanified.js` are already written
//! (the TS's `committed` point); everything here rewrites files in place.
//! [`finish_split_output`] then [`reconcile_post_split`] — the reconcile
//! runs only when the finish succeeded (a throw ends the TS stage).

use std::fs;
use std::path::{Path, PathBuf};

use humanify_model::js::{JsValue, stringify};

use crate::place::layout::METADATA_DIR;
use crate::rename::eligibility::{Eligibility, NeverRename};
use crate::rename::name_profile::NameProfile;
use crate::unpack::bun::{bun_manifest_path, find_prior_tree_root};

use super::carry::{CarryResult, carry_renames_into_bundle};
use super::reconcile::{PostSplitInput, PostSplitResult, post_split_reconcile};

use super::relink::{
    FactoryLookup, VendorBridge, bun_relink_runtime_filename, relink_factory_references,
    wrap_extracted_factory,
};
use super::scaffold::{detect_external_packages, read_utf8, write_runnable_scaffold};
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
    pub output_dir: &'a Path,
    /// The runnable emit's file map keys, in emission order — None when the
    /// review tree was written instead (`--split-pure` or a decline).
    pub runnable: Option<&'a [String]>,
    /// `--prior-version` (the prior release's `humanified.js`).
    pub prior_version: Option<&'a Path>,
    /// The input bundle (its directory resolves installed versions).
    pub input_file: &'a Path,
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
}

/// The Bun manifest as the finish reads it (`BunModulesManifest`).
struct Manifest {
    runtime_file: Option<String>,
    /// (fileName, runtimeIdentifier), manifest order.
    factories: Vec<(String, Option<String>)>,
}

/// `loadBunManifest(outputDir)`: None when absent, not stamped by an
/// adapter that writes this vendor record (the unpack registry's answer,
/// `UnpackAdapter::of_vendor_record_stamp` — a new vendor-extracting
/// adapter is accepted by registering it, never by editing a list here;
/// docs/plugin-spec.md I14), or factory-less.
fn load_bun_manifest(output_dir: &Path) -> Result<Option<Manifest>, String> {
    let path = bun_manifest_path(output_dir);
    if !path.exists() {
        return Ok(None);
    }
    let text = read_utf8(&path)?;
    let JsValue::Object(obj) = JsValue::parse(&text)? else {
        return Err(format!("{}: not an object", path.display()));
    };
    let str_field = |o: &humanify_model::js::JsObject, k: &str| match o.get(k) {
        Some(JsValue::String(s)) => Some(s.clone()),
        _ => None,
    };
    let stamp = str_field(&obj, "adapter");
    if stamp
        .as_deref()
        .and_then(crate::unpack::UnpackAdapter::of_vendor_record_stamp)
        .is_none()
    {
        return Ok(None);
    }
    let factories: Vec<(String, Option<String>)> = match obj.get("factories") {
        Some(JsValue::Array(items)) => items
            .iter()
            .filter_map(|f| match f {
                JsValue::Object(o) => {
                    Some((str_field(o, "fileName")?, str_field(o, "runtimeIdentifier")))
                }
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    };
    if factories.is_empty() {
        return Ok(None);
    }
    Ok(Some(Manifest {
        runtime_file: str_field(&obj, "runtimeFile"),
        factories,
    }))
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

fn write_file(path: &Path, text: &str) -> Result<(), String> {
    fs::write(path, text).map_err(|e| format!("write {}: {e}", path.display()))
}

/// What the stage did (the TS's progress lines, in order).
#[derive(Debug, Default)]
pub struct FinishReport {
    pub messages: Vec<String>,
}

/// `relinkBunModules(outputDir, manifest, splitFiles, { priorRoot })`.
fn relink_bun_modules(
    output_dir: &Path,
    manifest: &Manifest,
    split_files: &[String],
    prior_root: Option<&Path>,
    bridges: &[VendorBridge],
    interop: crate::toolchain::InteropHelpers,
    report: &mut FinishReport,
) -> Result<(), String> {
    let lookup: FactoryLookup = manifest
        .factories
        .iter()
        .filter_map(|(file, id)| id.clone().map(|id| (id, file.clone())))
        .collect();
    let runtime_path = output_dir.join(bun_relink_runtime_filename());
    if let Some(dir) = runtime_path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("mkdir {}: {e}", dir.display()))?;
    }
    write_file(&runtime_path, interop.relink_runtime())?;
    let mut inherit = prior_root.map(VendorBodyInheritor::new);
    let mut bridged_reads = 0usize;
    for (file_name, _) in &manifest.factories {
        let abs = output_dir.join(file_name);
        let body = read_utf8(&abs)?;
        let (rendered, bridged) = wrap_extracted_factory(&body, file_name, &lookup, bridges)
            .map_err(|e| format!("{file_name}: {e}"))?;
        bridged_reads += bridged;
        let bytes = match inherit.as_mut() {
            Some(i) => i.bytes_for(file_name, rendered),
            None => rendered,
        };
        write_file(&abs, &bytes)?;
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
        let abs = output_dir.join(rel);
        let code = read_utf8(&abs)?;
        let out =
            relink_factory_references(&code, rel, &lookup).map_err(|e| format!("{rel}: {e}"))?;
        write_file(&abs, &out)?;
    }
    if let Some(runtime) = &manifest.runtime_file {
        let _ = fs::remove_file(output_dir.join(runtime));
    }
    Ok(())
}

/// `finishSplitOutput`: re-link, then (runnable only) the `using`
/// desugar and the scaffold. Returns whether a Bun re-link ran.
pub fn finish_split_output(
    input: &FinishInput<'_>,
    report: &mut FinishReport,
) -> Result<bool, String> {
    let output_dir = input.output_dir;
    let manifest = load_bun_manifest(output_dir)?;
    if let (Some(runnable), Some(manifest)) = (input.runnable, manifest.as_ref()) {
        let prior_root: Option<PathBuf> = if input.switches.vendor_inherit_disabled {
            None
        } else {
            input.prior_version.and_then(find_prior_tree_root)
        };
        {
            // The finish's constituents were dark (docs/perf-inventory.md
            // item 3: 40 s serial per with-prior hop, no internal spans) —
            // a span each for relink, the using desugar, the scaffold,
            // then (in reconcile_post_split) the ledger read, the
            // per-file reconcile, the apply and the carry.
            let _ph = crate::profiling::phase("split:finish:relink");
            relink_bun_modules(
                output_dir,
                manifest,
                runnable,
                prior_root.as_deref(),
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
        let _ = fs::remove_file(output_dir.join(runtime));
    }
    if let Some(runnable) = input.runnable {
        let desugared = {
            let _ph = crate::profiling::phase("split:finish:desugar-using");
            super::using::desugar_using_in_tree(output_dir)?
        };
        report
            .messages
            .push(super::using::desugar_summary(output_dir, desugared));
        let entry = runnable_entry_file(runnable);
        let externals = {
            let _ph = crate::profiling::phase("split:finish:scaffold");
            let externals = detect_external_packages(output_dir)?;
            write_runnable_scaffold(output_dir, &entry, &externals, input.input_file.parent())?;
            externals
        };
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
            "Runnable scaffold: run.cjs + package.json ({deps}) — `npm install && node run.cjs --version`"
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
    report: &mut FinishReport,
) -> Result<(bool, Option<ReconcileReport>), String> {
    let relinked = finish_split_output(input, report)?;
    let reconciled = reconcile_post_split(
        input.output_dir,
        input.prior_version,
        input.switches,
        input.name_profile,
        input.never_rename,
        input.layout,
        report,
    )?;
    Ok((relinked, reconciled))
}

// ---------------------------------------------------------------------------
// The post-split reconcile + bundle carry (`reconcilePostSplit`,
// `carryIntoBundle`)
// ---------------------------------------------------------------------------

/// `splitTreeRootOf(priorFile)`: the prior tree's root (the dir holding
/// `.humanify/`, or the file's own dir).
pub fn split_tree_root_of(prior_file: &Path) -> PathBuf {
    let dir = prior_file.parent().unwrap_or(Path::new(""));
    if dir.file_name().is_some_and(|n| n == METADATA_DIR) {
        dir.parent().unwrap_or(Path::new("")).to_path_buf()
    } else {
        dir.to_path_buf()
    }
}

/// What the reconcile stage did, for the gate's report.
#[derive(Debug, Default)]
pub struct ReconcileReport {
    pub result: PostSplitResult,
    pub carry: Option<CarryResult>,
}

fn read_opt(root: &Path, file: &str) -> Option<String> {
    std::fs::read(root.join(file))
        .ok()
        .map(|b| String::from_utf8_lossy(&b).into_owned())
}

/// `reconcilePostSplit(opts, ledger, isEligible)` over the tree on disk:
/// the ledger is `.humanify/split-ledger.json` (rewritten when a file
/// changed), the bundle `.humanify/humanified.js`.
pub fn reconcile_post_split(
    output_dir: &Path,
    prior_version: Option<&Path>,
    switches: FinishSwitches,
    name_profile: NameProfile,
    never_rename: NeverRename,
    layout: crate::toolchain::BundleLayout,
    report: &mut FinishReport,
) -> Result<Option<ReconcileReport>, String> {
    let Some(prior_version) = prior_version else {
        return Ok(None);
    };
    let prior_root = split_tree_root_of(prior_version);
    let ledger_path = output_dir.join(METADATA_DIR).join("split-ledger.json");
    let (mut ledger, eligible) = {
        let _ph = crate::profiling::phase("split:finish:reconcile-ledger");
        (
            JsValue::parse(&read_utf8(&ledger_path)?)?,
            Eligibility::new(never_rename),
        )
    };
    let read_fresh = |f: &str| read_opt(output_dir, f);
    let read_prior = |f: &str| read_opt(&prior_root, f);
    let result = {
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
        })
    };
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
            write_file(&output_dir.join(file), text)?;
        }
        write_file(&ledger_path, &stringify(&ledger))?;
    }
    let carry = {
        let _ph = crate::profiling::phase("split:finish:carry");
        carry_into_bundle(
            output_dir,
            &ledger,
            &result.renames,
            name_profile,
            layout,
            report,
        )
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

/// `carryIntoBundle`: give `.humanify/humanified.js` the names the tree
/// shipped; abstentions are counted by reason.
fn carry_into_bundle(
    output_dir: &Path,
    ledger: &JsValue,
    renames: &[super::reconcile::PostSplitRename],
    profile: NameProfile,
    layout: crate::toolchain::BundleLayout,
    report: &mut FinishReport,
) -> Option<CarryResult> {
    let bundle_path = output_dir.join(METADATA_DIR).join("humanified.js");
    let bundle = read_utf8(&bundle_path).ok()?;
    let carry = match carry_renames_into_bundle(&bundle, ledger, renames, profile, layout) {
        Ok(c) => c,
        Err(_) => return None, // "bundle carry skipped" (debug only)
    };
    if let Some(code) = &carry.code
        && write_file(&bundle_path, code).is_err()
    {
        return None;
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
