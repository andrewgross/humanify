//! The finishing stage over a tree on disk, in the TS's order — the driver
//! half of `src/commands/unified.ts` (`finishSplitOutput` →
//! `reconcilePostSplit` → `carryIntoBundle`) plus `relinkBunModules`.
//!
//! The tree, the ledger and `.humanify/humanified.js` are already written
//! (the TS's `committed` point); everything here rewrites files in place.

use std::fs;
use std::path::{Path, PathBuf};

use humanify_model::js::JsValue;

use crate::unpack::bun::{bun_manifest_path, find_prior_tree_root};

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
