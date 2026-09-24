//! The Bun unpack adapter (`src/unpack/adapters/bun.ts` `BunUnpackAdapter`):
//! every `var X = HELPER((exports, module) => {...})` CJS factory body is
//! set aside as a vendored file under `vendor/`, every reference to a
//! factory var is rewritten to a content-derived identifier, what is left
//! becomes `runtime.js`, and `vendor/_bun-modules.json` records the naming.
//!
//! Classification is `crate::modules` (WP1.5); naming — the deterministic
//! cascade, the file names, the LLM fallback pass, the manifest shape and
//! order — is `crate::modules::vendor_names`. This file is the adapter: the
//! extraction ranges, the stable-identifier rewrite plan, the text edits,
//! and the tree write.
//!
//! Offsets: the TS works in UTF-16 code units and this port in UTF-8 bytes;
//! every range here is a node span or a match position over the SAME text,
//! so the sliced strings are identical.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use oxc_allocator::Allocator;
use oxc_ast::AstKind;
use oxc_span::GetSpan;
use oxc_syntax::reference::ReferenceFlags;

use crate::babel_view::unparen;
use crate::detect::js_text::{is_js_space, is_word_boundary, skip_js_space};
use crate::hash::serialize::SymbolTables;
use crate::ingest::Ingest;
use crate::modules::vendor_names::{
    BunModulesManifest, FileNameChooser, ManifestEntry, NameLookup, PriorManifestEntry,
    VendorNamer, annotate_hash_ordinals, load_prior_manifest_factories, load_prior_vendor_names,
    name_fallback_factories_with_llm, order_by_prior_manifest, stable_stem,
};
use crate::modules::wrapper::find_wrapper_function;
use crate::modules::{
    BunModuleClassification, FactoryNameCounts, FactoryRecord, classify_bun_modules,
    identify_bun_cjs_factory, name_cjs_factories,
};

use super::{UnpackResult, UnpackedFile, write_passthrough};

/// The vendor folder (`split/layout.ts VENDOR_DIR`).
pub const VENDOR_DIR: &str = "vendor";

/// The sidecar manifest's file name, inside `vendor/` (`BUN_MODULES_MANIFEST`).
pub const BUN_MODULES_MANIFEST: &str = "_bun-modules.json";

/// The runtime file's name (the leftover code outside every factory).
pub const RUNTIME_FILE: &str = "runtime.js";

/// The manifest's path within an output tree (`bunManifestPath`).
pub fn bun_manifest_path(output_dir: &Path) -> PathBuf {
    output_dir.join(VENDOR_DIR).join(BUN_MODULES_MANIFEST)
}

/// The prior release's TREE ROOT — the directory holding `vendor/` —
/// resolved from whatever `--prior-version` points at
/// (`findPriorTreeRoot`): normally a prior tree's `.humanify/humanified.js`,
/// so try that file's own directory first, then its parent.
pub fn find_prior_tree_root(prior_file: &Path) -> Option<PathBuf> {
    let dir = prior_file.parent().unwrap_or(Path::new(""));
    let parent = dir.parent().unwrap_or(Path::new(""));
    [dir.to_path_buf(), parent.to_path_buf()]
        .into_iter()
        .find(|root| bun_manifest_path(root).exists())
}

/// The prior tree's manifest text, when there is a prior tree.
fn prior_manifest_text(prior_file: &Path) -> Option<String> {
    let root = find_prior_tree_root(prior_file)?;
    fs::read_to_string(bun_manifest_path(&root)).ok()
}

/// `loadPriorVendorNames(priorFile)`: structuralHash → the names its
/// factories carried, in bundle order. None without a prior tree or a
/// parseable manifest.
pub fn load_prior_vendor_names_from(prior_file: &Path) -> Option<HashMap<String, Vec<String>>> {
    load_prior_vendor_names(&prior_manifest_text(prior_file)?)
}

/// `loadPriorManifestFactories(priorFile)`: the prior manifest's entries in
/// the order that release emitted them.
pub fn load_prior_manifest_factories_from(prior_file: &Path) -> Option<Vec<PriorManifestEntry>> {
    load_prior_manifest_factories(&prior_manifest_text(prior_file)?)
}

/// A hook over the classification between classifying and naming — the
/// migration seam the WPB.2 gate uses to substitute the TS's structural
/// hash BYTES (00-control §3: the bytes differ by design, the classes are
/// gated). Production passes None.
pub type ClassificationHook<'h> = &'h dyn Fn(&mut BunModuleClassification) -> Result<(), String>;

/// The unpack options (`UnpackOptions`).
#[derive(Default)]
pub struct BunUnpackOptions<'n> {
    /// The LLM namer for hash-named factories — None skips the pass, as the
    /// TS does when no `vendorNamer` is wired.
    pub namer: Option<&'n mut dyn VendorNamer>,
    /// The prior release's vendor names (carry-over, ahead of the LLM).
    pub prior_vendor_names: Option<HashMap<String, Vec<String>>>,
    /// The prior release's manifest entries, in its emitted order.
    pub prior_manifest_factories: Option<Vec<PriorManifestEntry>>,
    /// See `ClassificationHook`.
    pub classification_hook: Option<ClassificationHook<'n>>,
}

/// What the Bun adapter did, beyond the files.
pub struct BunUnpackOutcome {
    pub result: UnpackResult,
    /// The written manifest — None when the adapter fell back to a single
    /// `index.js` (no factory helper, or no extractable factory).
    pub manifest: Option<BunModulesManifest>,
    /// The deterministic cascade's per-source counts (the verbose "Vendor
    /// name sources" line) — None when the AST classifier did not run.
    pub name_counts: Option<FactoryNameCounts>,
    /// How many factories the LLM pass renamed.
    pub llm_renamed: usize,
    /// Every extracted module in BUNDLE order (the manifest is written in
    /// the prior release's order and drops the factory var).
    pub bundle_order: Vec<BundleOrderRow>,
}

/// One extracted module, in bundle order.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct BundleOrderRow {
    #[serde(rename = "factoryVar")]
    pub factory_var: String,
    #[serde(rename = "fileName")]
    pub file_name: String,
    #[serde(rename = "runtimeIdentifier")]
    pub runtime_identifier: Option<String>,
}

/// One factory's extraction ranges (`ExtractedModule`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtractedModule {
    /// The factory var.
    pub name: String,
    /// The factory BODY range (the helper call's first argument).
    pub body_start: usize,
    pub body_end: usize,
    /// The whole declaration's range (spliced out of the runtime), past
    /// one trailing `;`.
    pub decl_start: usize,
    pub decl_end: usize,
}

/// A byte-precise source substitution ("" = splice the range out).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextEdit {
    pub start: usize,
    pub end: usize,
    pub replacement: String,
}

/// Pass 1's per-module plan (`ModulePlan`).
pub struct ModulePlan {
    pub naming: NameLookup,
    pub identifier: Option<String>,
}

/// Pass 1 of unpack (`planModules`): file names + the stable-identifier
/// rewrite, per extracted module, and the edits both passes apply.
pub struct UnpackPlan {
    pub plans: Vec<ModulePlan>,
    pub decl_edits: Vec<TextEdit>,
    pub ref_edits: Vec<TextEdit>,
}

/// The Bun adapter (`BunUnpackAdapter.unpack`).
pub fn unpack_bun(
    code: &str,
    out_dir: &Path,
    mut options: BunUnpackOptions<'_>,
) -> Result<BunUnpackOutcome, String> {
    fs::create_dir_all(out_dir).map_err(|e| format!("mkdir {}: {e}", out_dir.display()))?;
    let floor = |code: &str| -> Result<BunUnpackOutcome, String> {
        Ok(BunUnpackOutcome {
            result: write_passthrough(code, out_dir)?,
            manifest: None,
            name_counts: None,
            llm_renamed: 0,
            bundle_order: Vec::new(),
        })
    };

    let Some(factory) = identify_bun_cjs_factory(code) else {
        return floor(code);
    };
    let require_var = identify_bun_require(code);

    let allocator = Allocator::default();
    let ingest = Ingest::parse_unambiguous(&allocator, code);
    // The TS classifier runs on Babel's error-recovering parse and falls
    // back to the regex extractor when it throws. A syntax error here takes
    // the regex floor: on broken input the two parsers recover differently,
    // and the regex path is the one both sides define the same way.
    let mut classification = if ingest.errors.is_empty() {
        classify(code, &ingest)
    } else {
        None
    };
    let mut name_counts = None;
    let mut llm_renamed = 0;
    if let Some(c) = classification.as_mut() {
        if let Some(hook) = options.classification_hook {
            hook(c)?;
        }
        name_counts = Some(name_cjs_factories(
            c,
            code,
            options.prior_vendor_names.as_ref(),
        ));
        // Post-cascade LLM pass: only hash-named (fallback) factories are
        // re-named, so banner/URL/carry-over names always win.
        if let Some(namer) = options.namer.as_deref_mut() {
            llm_renamed = name_fallback_factories_with_llm(&mut c.factories, code, namer);
        }
    }
    let helper_name = classification
        .as_ref()
        .map_or(factory.name.as_str(), |c| c.helper_var.as_str());

    // AST extraction is the source of truth; the regex floor only when the
    // classifier did not run.
    let modules = match &classification {
        Some(c) => extract_factory_bodies_from_ast(c, code, &ingest),
        None => extract_factory_bodies(code, helper_name),
    };
    if modules.is_empty() {
        return floor(code);
    }

    let factories: &[FactoryRecord] = classification
        .as_ref()
        .map_or(&[], |c| c.factories.as_slice());
    let planner = classification
        .as_ref()
        .map(|_| IdentifierPlanner::build(&ingest));
    let plan = plan_modules(&modules, factories, code, planner.as_ref(), &ingest);
    let by_factory_var = naming_lookup(factories);

    // Pass 2: each factory body with cross-factory references rewritten,
    // then the runtime the same way.
    fs::create_dir_all(out_dir.join(VENDOR_DIR)).map_err(|e| format!("mkdir vendor: {e}"))?;
    let mut files = Vec::new();
    let mut entries = Vec::new();
    let mut bundle_order = Vec::new();
    for (module, module_plan) in modules.iter().zip(&plan.plans) {
        let mut body = slice_with_edits(code, &plan.ref_edits, module.body_start, module.body_end);
        if let Some(req) = &require_var {
            body = rewrite_require_calls(&body, req);
        }
        let record = by_factory_var
            .get(module.name.as_str())
            .map(|&i| &factories[i]);
        let rel_path = format!("{VENDOR_DIR}/{}.js", module_plan.naming.file_name);
        files.push(UnpackedFile {
            path: write_vendor_file(out_dir, &rel_path, &body)?,
            metadata: None,
        });
        bundle_order.push(BundleOrderRow {
            factory_var: module.name.clone(),
            file_name: rel_path.clone(),
            runtime_identifier: module_plan.identifier.clone(),
        });
        entries.push(ManifestEntry {
            file_name: rel_path,
            name: module_plan.naming.name.clone(),
            name_source: module_plan.naming.name_source.as_str(),
            structural_hash: module_plan.naming.structural_hash.clone(),
            runtime_identifier: module_plan.identifier.clone(),
            banner_package: record.and_then(|r| r.banner_package.clone()),
            banner_version: record.and_then(|r| r.banner_version.clone()),
            hash_ordinal: None,
        });
    }

    let mut all_edits = plan.decl_edits.clone();
    all_edits.extend(plan.ref_edits.iter().cloned());
    let runtime = slice_with_edits(code, &all_edits, 0, code.len());
    let mut runtime_file = None;
    if !runtime.trim_matches(is_js_space).is_empty() {
        let path = out_dir.join(RUNTIME_FILE);
        fs::write(&path, &runtime).map_err(|e| format!("write runtime: {e}"))?;
        files.push(UnpackedFile {
            path,
            metadata: None,
        });
        runtime_file = Some(RUNTIME_FILE.to_string());
    }

    // Entries are in BUNDLE order here, the order the naming tie-break is
    // defined against — ordinals are stamped BEFORE the reorder.
    let manifest = BunModulesManifest {
        adapter: "bun",
        runtime_file,
        factories: order_by_prior_manifest(
            annotate_hash_ordinals(entries),
            options.prior_manifest_factories.as_deref(),
        ),
    };
    fs::write(bun_manifest_path(out_dir), manifest.to_written_json())
        .map_err(|e| format!("write manifest: {e}"))?;

    Ok(BunUnpackOutcome {
        result: UnpackResult { files },
        manifest: Some(manifest),
        name_counts,
        llm_renamed,
        bundle_order,
    })
}

/// The AST classification on the parsed input (`classifyWithAst` minus the
/// naming, which the caller sequences around the hook).
fn classify(code: &str, ingest: &Ingest<'_>) -> Option<BunModuleClassification> {
    let wrapper = find_wrapper_function(ingest.program, &ingest.semantic);
    let tables = SymbolTables::build(&ingest.semantic);
    classify_bun_modules(
        code,
        ingest.program,
        &ingest.semantic,
        wrapper.as_ref().map(|w| w.body_span),
        &tables,
    )
}

/// factoryVar → record index; a duplicated var keeps the LAST record (the
/// TS `Map.set` in `buildNamingLookup`).
fn naming_lookup(factories: &[FactoryRecord]) -> HashMap<&str, usize> {
    let mut map = HashMap::new();
    for (i, f) in factories.iter().enumerate() {
        map.insert(f.factory_var.as_str(), i);
    }
    map
}

/// Write one vendored body, creating a nested package folder
/// (vendor/@scope/name/…) when the grouped name has one.
fn write_vendor_file(out_dir: &Path, rel_path: &str, body: &str) -> Result<PathBuf, String> {
    let path = out_dir.join(rel_path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
    }
    fs::write(&path, body).map_err(|e| format!("write {}: {e}", path.display()))?;
    Ok(path)
}

/// Pass 1 (`planModules`): the file name and the stable-identifier plan for
/// every module, in module order. The identifier is DECOUPLED from the
/// display path — derived from the module's stable structural stem, so a
/// package folder (a display change) never churns runtime.js references.
pub fn plan_modules(
    modules: &[ExtractedModule],
    factories: &[FactoryRecord],
    code: &str,
    planner: Option<&IdentifierPlanner>,
    ingest: &Ingest<'_>,
) -> UnpackPlan {
    let by_factory_var = naming_lookup(factories);
    let record_of =
        |m: &ExtractedModule| by_factory_var.get(m.name.as_str()).map(|&i| &factories[i]);
    let mut chooser = FileNameChooser::new(modules.iter().filter_map(record_of));
    let mut used_identifiers: HashSet<String> = HashSet::new();
    let mut plans = Vec::with_capacity(modules.len());
    let mut decl_edits = Vec::new();
    let mut ref_edits = Vec::new();
    for module in modules {
        decl_edits.push(TextEdit {
            start: module.decl_start,
            end: module.decl_end,
            replacement: String::new(),
        });
        let record = record_of(module);
        let naming = chooser.choose(
            &module.name,
            record,
            &code[module.body_start..module.body_end],
        );
        let mut identifier = None;
        if let (Some(record), Some(planner)) = (record, planner)
            && let Some((chosen, spans)) = planner.plan(ingest, record, &used_identifiers)
        {
            used_identifiers.insert(chosen.clone());
            ref_edits.extend(spans.into_iter().map(|(start, end)| TextEdit {
                start,
                end,
                replacement: chosen.clone(),
            }));
            identifier = Some(chosen);
        }
        plans.push(ModulePlan { naming, identifier });
    }
    UnpackPlan {
        plans,
        decl_edits,
        ref_edits,
    }
}

/// `sliceWithEdits`: `[slice_start, slice_end)` of `code` with every edit
/// inside the range applied; an edit inside an already-consumed range (a
/// reference inside a spliced-out declaration) is dropped.
pub fn slice_with_edits(
    code: &str,
    edits: &[TextEdit],
    slice_start: usize,
    slice_end: usize,
) -> String {
    let mut in_range: Vec<&TextEdit> = edits
        .iter()
        .filter(|e| e.start >= slice_start && e.end <= slice_end)
        .collect();
    // Stable, like Array.prototype.sort: start ascending, then end
    // DEscending.
    in_range.sort_by(|a, b| a.start.cmp(&b.start).then(b.end.cmp(&a.end)));
    let mut out = String::with_capacity(slice_end - slice_start);
    let mut cursor = slice_start;
    for edit in in_range {
        if edit.start < cursor {
            continue;
        }
        out.push_str(&code[cursor..edit.start]);
        out.push_str(&edit.replacement);
        cursor = edit.end;
    }
    out.push_str(&code[cursor..slice_end]);
    out
}

/// `extractFactoryBodiesFromAst`: one module per classified factory whose
/// declarator is `HELPER(arrowOrFunction, …)` (`factoryToModule` — the call
/// needs at least one argument and the first must be a function; the
/// callee is not re-checked). The declaration range covers the `var`
/// keyword — the parent VariableDeclaration — plus one trailing `;`.
fn extract_factory_bodies_from_ast(
    classification: &BunModuleClassification,
    code: &str,
    ingest: &Ingest<'_>,
) -> Vec<ExtractedModule> {
    let nodes = ingest.semantic.nodes();
    // Declarator span → (body range, parent declaration span).
    let mut shapes: HashMap<(u32, u32), [(usize, usize); 2]> = HashMap::new();
    for node in nodes.iter() {
        let AstKind::VariableDeclarator(decl) = node.kind() else {
            continue;
        };
        let Some(oxc_ast::ast::Expression::CallExpression(call)) = decl.init.as_ref() else {
            continue;
        };
        let Some(arg0) = call.arguments.first().and_then(|a| a.as_expression()) else {
            continue;
        };
        let arg0 = unparen(arg0);
        if !matches!(
            arg0,
            oxc_ast::ast::Expression::ArrowFunctionExpression(_)
                | oxc_ast::ast::Expression::FunctionExpression(_)
        ) {
            continue;
        }
        let parent = nodes.parent_node(node.id());
        let AstKind::VariableDeclaration(declaration) = parent.kind() else {
            continue;
        };
        let body = arg0.span();
        let d = declaration.span();
        shapes.insert(
            (decl.span.start, decl.span.end),
            [
                (body.start as usize, body.end as usize),
                (d.start as usize, d.end as usize),
            ],
        );
    }
    classification
        .factories
        .iter()
        .filter_map(|f| {
            let &[(body_start, body_end), (decl_start, mut decl_end)] =
                shapes.get(&(f.span.start, f.span.end))?;
            if code.as_bytes().get(decl_end) == Some(&b';') {
                decl_end += 1;
            }
            Some(ExtractedModule {
                name: f.factory_var.clone(),
                body_start,
                body_end,
                decl_start,
                decl_end,
            })
        })
        .collect()
}

/// A non-unicode `[$\w]` byte.
fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

/// A non-unicode `\w` byte.
fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// `(?:var|let|const)\s+` at `at` → the offset after the whitespace run.
fn keyword_then_space(code: &str, at: usize) -> Option<usize> {
    let rest = &code[at..];
    let after = ["var", "let", "const"]
        .iter()
        .find(|kw| rest.starts_with(**kw))
        .map(|kw| at + kw.len())?;
    let end = skip_js_space(code, after);
    (end > after).then_some(end)
}

/// A greedy run of bytes satisfying `pred` from `at` → its end.
fn run_end(code: &str, at: usize, pred: fn(u8) -> bool) -> usize {
    let bytes = code.as_bytes();
    let mut j = at;
    while j < bytes.len() && pred(bytes[j]) {
        j += 1;
    }
    j
}

/// The regex floor (`extractFactoryBodies`), used only when the AST
/// classifier did not run: every
/// `/(?:var|let|const)\s+([$\w]+)\s*=\s*HELPER\s*\(/g` match, the body
/// being the text between the call's outermost parens (paren depth only —
/// it mishandles parens inside string/regex/template literals, the TS's
/// documented weakness).
pub fn extract_factory_bodies(code: &str, helper: &str) -> Vec<ExtractedModule> {
    let mut modules = Vec::new();
    let mut from = 0;
    while from < code.len() {
        let Some((start, paren)) = next_factory_decl(code, from, helper) else {
            break;
        };
        from = paren + 1; // the regex's lastIndex: just past the `(`
        let Some(paren_end) = find_matching_paren(code, paren) else {
            continue;
        };
        let mut decl_end = paren_end + 1;
        if code.as_bytes().get(decl_end) == Some(&b';') {
            decl_end += 1;
        }
        let name_start = keyword_then_space(code, start).expect("matched above");
        modules.push(ExtractedModule {
            name: code[name_start..run_end(code, name_start, is_ident_byte)].to_string(),
            body_start: paren + 1,
            body_end: paren_end,
            decl_start: start,
            decl_end,
        });
    }
    modules
}

/// The leftmost match of the declaration pattern at or after `from` →
/// (match start, offset of the `(`).
fn next_factory_decl(code: &str, from: usize, helper: &str) -> Option<(usize, usize)> {
    for (i, _) in code[from..].char_indices() {
        let at = from + i;
        let Some(name_start) = keyword_then_space(code, at) else {
            continue;
        };
        let name_end = run_end(code, name_start, is_ident_byte);
        if name_end == name_start {
            continue;
        }
        let mut p = skip_js_space(code, name_end);
        if code.as_bytes().get(p) != Some(&b'=') {
            continue;
        }
        p = skip_js_space(code, p + 1);
        if !code[p..].starts_with(helper) {
            continue;
        }
        p = skip_js_space(code, p + helper.len());
        if code.as_bytes().get(p) == Some(&b'(') {
            return Some((at, p));
        }
    }
    None
}

/// `findMatchingParen`: depth-counting scan from the `(` at `open`.
fn find_matching_paren(code: &str, open: usize) -> Option<usize> {
    let mut depth = 0i64;
    for (i, b) in code.as_bytes()[open..].iter().enumerate() {
        match b {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(open + i);
                }
            }
            _ => {}
        }
    }
    None
}

/// `rewriteRequireCalls`: `/\bREQ\(/g` → `require(`. `REQ` is a `\w+`
/// capture, so the boundary is the ASCII one before its first character.
pub fn rewrite_require_calls(body: &str, require_var: &str) -> String {
    let needle = format!("{require_var}(");
    let mut out = String::with_capacity(body.len());
    let mut cursor = 0;
    for (i, _) in body.match_indices(&needle) {
        if i < cursor || !is_word_boundary(body, i) {
            continue;
        }
        out.push_str(&body[cursor..i]);
        out.push_str("require(");
        cursor = i + needle.len();
    }
    out.push_str(&body[cursor..]);
    out
}

/// `identifyBunRequire` (`src/shared/bun-helpers.ts`): the require variable
/// traced through the createRequire import —
/// `import{createRequire as Glq}from"node:module";var m6=Glq(import.meta.url);`
/// → "m6". Two non-unicode regexes, first match each:
///
/// 1. `/import\s*\{[^}]*createRequire\s+as\s+(\w+)[^}]*\}\s*from\s*["']node:module["']/`
///    — the brace group is exactly the text up to the FIRST `}` (`[^}]`
///    cannot cross one), and the greedy `[^}]*` prefix makes the LAST
///    `createRequire as X` inside it the capture.
/// 2. `(?:var|let|const)\s+(\w+)\s*=\s*ALIAS\(import\.meta\.url\)`.
pub fn identify_bun_require(source: &str) -> Option<String> {
    let alias = create_require_alias(source)?;
    let call = format!("{alias}(import.meta.url)");
    for (at, _) in source.char_indices() {
        let Some(name_start) = keyword_then_space(source, at) else {
            continue;
        };
        let name_end = run_end(source, name_start, is_word_byte);
        if name_end == name_start {
            continue;
        }
        let mut p = skip_js_space(source, name_end);
        if source.as_bytes().get(p) != Some(&b'=') {
            continue;
        }
        p = skip_js_space(source, p + 1);
        if source[p..].starts_with(&call) {
            return Some(source[name_start..name_end].to_string());
        }
    }
    None
}

/// Regex 1 of `identifyBunRequire` → the createRequire alias.
fn create_require_alias(source: &str) -> Option<String> {
    for (at, _) in source.match_indices("import") {
        let brace = skip_js_space(source, at + "import".len());
        if source.as_bytes().get(brace) != Some(&b'{') {
            continue;
        }
        let Some(close) = source[brace + 1..].find('}').map(|i| brace + 1 + i) else {
            continue;
        };
        let group = &source[brace + 1..close];
        let tail = skip_js_space(source, close + 1);
        let Some(after_from) = source[tail..].starts_with("from").then_some(tail + 4) else {
            continue;
        };
        let quote = skip_js_space(source, after_from);
        let rest = &source[quote..];
        let quoted = [
            "\"node:module\"",
            "\"node:module'",
            "'node:module\"",
            "'node:module'",
        ]
        .iter()
        .any(|q| rest.starts_with(q));
        if !quoted {
            continue;
        }
        // The LAST `createRequire\s+as\s+(\w+)` in the group.
        let alias = group
            .match_indices("createRequire")
            .filter_map(|(i, _)| {
                let after = i + "createRequire".len();
                let as_at = skip_js_space(group, after);
                if as_at == after || !group[as_at..].starts_with("as") {
                    return None;
                }
                let name_at = skip_js_space(group, as_at + 2);
                if name_at == as_at + 2 {
                    return None;
                }
                let name_end = run_end(group, name_at, is_word_byte);
                (name_end > name_at).then(|| group[name_at..name_end].to_string())
            })
            .last();
        if alias.is_some() {
            return alias;
        }
    }
    None
}

// ---------------------------------------------------------------------------
// The stable-identifier plan (`planFactoryRename` +
// `chooseCaptureFreeIdentifier`)
// ---------------------------------------------------------------------------

/// The oxc-derived state the identifier plan needs, built once per input.
pub struct IdentifierPlanner {
    /// Symbol per (name, declarator span): the binding the TS resolves via
    /// `declPath.scope.getBinding(record.factoryVar)` + the
    /// `binding.path.node === declPath.node` identity check.
    bindings: HashMap<(String, u32, u32), oxc_semantic::SymbolId>,
    /// Every node that owns a scope → its scope id (for the reference-site
    /// `hasBinding` walk).
    scope_at_node: HashMap<(u32, u32), oxc_semantic::ScopeId>,
    /// `programScope.globals` — the unresolved (global) reference names.
    globals: HashSet<String>,
}

impl IdentifierPlanner {
    pub fn build(ingest: &Ingest<'_>) -> IdentifierPlanner {
        let scoping = ingest.semantic.scoping();
        let nodes = ingest.semantic.nodes();
        let mut bindings = HashMap::new();
        for symbol in scoping.symbol_ids() {
            let decl_node = scoping.symbol_declaration(symbol);
            if let AstKind::VariableDeclarator(decl) = nodes.get_node(decl_node).kind()
                && let oxc_ast::ast::BindingPattern::BindingIdentifier(id) = &decl.id
            {
                bindings.insert(
                    (id.name.to_string(), decl.span().start, decl.span().end),
                    symbol,
                );
            }
        }
        let mut scope_at_node = HashMap::new();
        for sid in 0..scoping.scopes_len() {
            let scope_id = oxc_semantic::ScopeId::new(sid);
            let node_id = scoping.get_node_id(scope_id);
            let span = nodes.get_node(node_id).span();
            scope_at_node.insert((span.start, span.end), scope_id);
        }
        let globals: HashSet<String> = scoping
            .root_unresolved_references()
            .keys()
            .map(|s| s.to_string())
            .collect();
        IdentifierPlanner {
            bindings,
            scope_at_node,
            globals,
        }
    }

    /// Plan one factory's rewrite (`planFactoryRename`): the chosen
    /// identifier and every reference's span, or None when the rewrite is
    /// unsafe — unresolvable/shadowed binding, a WRITE to the factory var
    /// (a partial rewrite would corrupt scope), or no capture-free
    /// identifier. The caller records the identifier as used.
    pub fn plan(
        &self,
        ingest: &Ingest<'_>,
        record: &FactoryRecord,
        used_identifiers: &HashSet<String>,
    ) -> Option<(String, Vec<(usize, usize)>)> {
        let scoping = ingest.semantic.scoping();
        let nodes = ingest.semantic.nodes();
        let symbol = *self.bindings.get(&(
            record.factory_var.clone(),
            record.span.start,
            record.span.end,
        ))?;
        // Babel's `binding.constantViolations`: a write, or a redeclaration.
        if !scoping.symbol_redeclarations(symbol).is_empty() {
            return None;
        }
        let mut reference_scopes = Vec::new();
        let mut spans = Vec::new();
        for &reference_id in scoping.get_resolved_reference_ids(symbol) {
            let reference = scoping.get_reference(reference_id);
            if reference.flags().contains(ReferenceFlags::Write) {
                return None;
            }
            let node_id = reference.node_id();
            let span = nodes.get_node(node_id).span();
            spans.push((span.start as usize, span.end as usize));
            // The reference site's nearest enclosing scope — the
            // `ref.scope` of Babel's hasBinding test.
            let mut current = node_id;
            loop {
                let s = nodes.get_node(current).span();
                if let Some(&scope_id) = self.scope_at_node.get(&(s.start, s.end)) {
                    reference_scopes.push(scope_id);
                    break;
                }
                let parent = nodes.parent_id(current);
                if parent == current {
                    break;
                }
                current = parent;
            }
        }
        let identifier = choose_capture_free_identifier(
            &sanitize_identifier(&stable_stem(record)),
            &reference_scopes,
            &self.globals,
            scoping,
            used_identifiers,
        )?;
        Some((identifier, spans))
    }
}

/// File names allow `@ . -`; identifiers don't (`sanitizeIdentifier`).
fn sanitize_identifier(file_name: &str) -> String {
    let mut out: String = file_name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '$' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if out.starts_with(|c: char| c.is_ascii_digit()) {
        out.insert(0, '_');
    }
    out
}

/// The candidate (or a `_2`, `_3`, … variant) no reference site can
/// capture (`chooseCaptureFreeIdentifier`): not already chosen for another
/// factory, not an existing free name in the bundle, and not bound in any
/// scope visible from a reference.
fn choose_capture_free_identifier(
    base: &str,
    reference_scopes: &[oxc_semantic::ScopeId],
    globals: &HashSet<String>,
    scoping: &oxc_semantic::Scoping,
    used_identifiers: &HashSet<String>,
) -> Option<String> {
    (1..=1000usize).find_map(|i| {
        let candidate = if i == 1 {
            base.to_string()
        } else {
            format!("{base}_{i}")
        };
        if used_identifiers.contains(&candidate) || globals.contains(&candidate) {
            return None;
        }
        let captured = reference_scopes.iter().any(|&scope| {
            let mut current = Some(scope);
            while let Some(s) = current {
                if scoping
                    .iter_bindings_in(s)
                    .any(|sym| scoping.symbol_name(sym) == candidate)
                {
                    return true;
                }
                current = scoping.scope_parent_id(s);
            }
            false
        });
        (!captured).then_some(candidate)
    })
}
