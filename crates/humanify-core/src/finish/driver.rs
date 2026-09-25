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

use humanify_model::js::{JsObject, JsValue, cmp_utf16, stringify, stringify_pretty};

use crate::place::layout::METADATA_DIR;
use crate::rename::eligibility::Eligibility;
use crate::unpack::bun::{bun_manifest_path, find_prior_tree_root};

use super::carry::{CarryResult, carry_renames_into_bundle};
use super::reconcile::{PostSplitInput, PostSplitResult, post_split_reconcile};

use super::relink::{
    BUN_RELINK_RUNTIME, FactoryLookup, bun_relink_runtime_filename, relink_factory_references,
    wrap_extracted_factory,
};
use super::scaffold::{detect_external_packages, read_utf8, write_runnable_scaffold};
use super::vendor_inherit::VendorBodyInheritor;

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
}

/// The Bun manifest as the finish reads it (`BunModulesManifest`).
struct Manifest {
    runtime_file: Option<String>,
    /// (fileName, runtimeIdentifier), manifest order.
    factories: Vec<(String, Option<String>)>,
}

/// `loadBunManifest(outputDir)`: None when absent, not the Bun adapter's,
/// or factory-less.
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
    if str_field(&obj, "adapter").as_deref() != Some("bun") {
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
    write_file(&runtime_path, BUN_RELINK_RUNTIME)?;
    let mut inherit = prior_root.map(VendorBodyInheritor::new);
    for (file_name, _) in &manifest.factories {
        let abs = output_dir.join(file_name);
        let body = read_utf8(&abs)?;
        let rendered = wrap_extracted_factory(&body, file_name, &lookup)
            .map_err(|e| format!("{file_name}: {e}"))?;
        let bytes = match inherit.as_mut() {
            Some(i) => i.bytes_for(file_name, rendered),
            None => rendered,
        };
        write_file(&abs, &bytes)?;
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
        relink_bun_modules(
            output_dir,
            manifest,
            runnable,
            prior_root.as_deref(),
            report,
        )?;
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
        let desugared = super::using::desugar_using_in_tree(output_dir)?;
        report
            .messages
            .push(super::using::desugar_summary(output_dir, desugared));
        let entry = runnable_entry_file(runnable);
        let externals = detect_external_packages(output_dir)?;
        write_runnable_scaffold(output_dir, &entry, &externals, input.input_file.parent())?;
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
    report: &mut FinishReport,
) -> Result<Option<ReconcileReport>, String> {
    let Some(prior_version) = prior_version else {
        return Ok(None);
    };
    let prior_root = split_tree_root_of(prior_version);
    let ledger_path = output_dir.join(METADATA_DIR).join("split-ledger.json");
    let mut ledger = JsValue::parse(&read_utf8(&ledger_path)?)?;
    let eligible = Eligibility::new(Some("bun"), Some("bun"));
    let read_fresh = |f: &str| read_opt(output_dir, f);
    let read_prior = |f: &str| read_opt(&prior_root, f);
    let result = post_split_reconcile(PostSplitInput {
        ledger: &mut ledger,
        read_fresh: &read_fresh,
        read_prior: &read_prior,
        eligible: &eligible,
        disabled: switches.post_split_reconcile_disabled,
    });
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
    for (file, text) in &result.changed {
        write_file(&output_dir.join(file), text)?;
    }
    write_file(&ledger_path, &stringify(&ledger))?;
    let carry = carry_into_bundle(output_dir, &ledger, &result.renames, report);
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
    report: &mut FinishReport,
) -> Option<CarryResult> {
    let bundle_path = output_dir.join(METADATA_DIR).join("humanified.js");
    let bundle = read_utf8(&bundle_path).ok()?;
    let carry = match carry_renames_into_bundle(&bundle, ledger, renames) {
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

/// The gate's JSON report (the TS probe's shape,
/// test/parity/wp54-reconcile-probe.ts).
pub fn reconcile_report_json(report: &ReconcileReport) -> String {
    let r = &report.result;
    let mut stats = JsObject::new();
    stats.insert("considered", JsValue::Number(r.stats.considered as f64));
    stats.insert("changed", JsValue::Number(r.stats.changed as f64));
    stats.insert("corpusGated", JsValue::Number(r.stats.corpus_gated as f64));
    stats.insert("discarded", JsValue::Number(r.stats.discarded as f64));
    stats.insert("incoherent", JsValue::Number(r.stats.incoherent as f64));
    let mut changed: Vec<&str> = r.changed.iter().map(|(f, _)| f.as_str()).collect();
    changed.sort_by(|a, b| cmp_utf16(a, b));
    let renames: Vec<JsValue> = r
        .renames
        .iter()
        .map(|x| {
            let mut o = JsObject::new();
            o.insert("file", JsValue::str(&x.file));
            o.insert("fromName", JsValue::str(&x.from_name));
            o.insert("toName", JsValue::str(&x.to_name));
            o.insert("kind", JsValue::str(x.kind));
            o.insert("votes", JsValue::Number(x.votes as f64));
            o.insert("topLevel", JsValue::Bool(x.top_level));
            if let Some((body, name)) = x.locator {
                let mut l = JsObject::new();
                l.insert("bodyOrdinal", JsValue::Number(body as f64));
                l.insert("nameOrdinal", JsValue::Number(name as f64));
                o.insert("locator", JsValue::Object(l));
            }
            JsValue::Object(o)
        })
        .collect();
    let mut root = JsObject::new();
    root.insert("messages", JsValue::Array(Vec::new()));
    root.insert("stats", JsValue::Object(stats));
    root.insert("changedFiles", JsValue::str_array(&changed));
    root.insert("renames", JsValue::Array(renames));
    let carry = match &report.carry {
        None => JsValue::Null,
        Some(c) => {
            let mut o = JsObject::new();
            o.insert("carried", JsValue::Number(c.carried as f64));
            o.insert("wroteBundle", JsValue::Bool(c.code.is_some()));
            let mut ab = c.abstained.clone();
            ab.sort_by(|a, b| cmp_utf16(&a.0, &b.0));
            let mut a = JsObject::new();
            for (k, n) in ab {
                a.insert(k, JsValue::Number(n as f64));
            }
            o.insert("abstained", JsValue::Object(a));
            JsValue::Object(o)
        }
    };
    root.insert("carry", carry);
    format!("{}\n", stringify_pretty(&JsValue::Object(root), 2))
}
