//! The module step end to end through the naming stage
//! (docs/design/module-naming.md): a helper-first module gets a name for
//! what it does, its wrapper becomes `init<Name>` and the split names its
//! file `<name>.js`; a barrel is left to the waves; a duplicate gets one
//! retry with the taken name listed; a wrapper the prior carried is never
//! asked again.

use std::sync::Mutex;

use humanify_model::llm::{BatchRenameResponse, LlmCall, LlmError, NameProvider, Renames};

use crate::libindex::table::RestorationRow;
use crate::naming::driver::{NamingConfig, NamingInput, NamingOutcome, run_naming};
use crate::naming::waves::processor::DEFAULT_PROMPT_WINDOW;
use crate::place::assign::namer::{MODULE_NAMER_SYSTEM_PROMPT, SplitNamerBudget};

/// Three recorded modules, no entry tail (the markers describe the whole
/// bundle): a helper-first color module (`a1` is a two-line rounding
/// helper), a barrel that only loads it, and a theme module.
const BUNDLE: &str = "var W = (fn, res) => () => (fn && (res = fn(fn = 0)), res);\n\
function a1(x) {\n  return Math.round(x);\n}\n\
var b1;\n\
function c1(from, to, t) {\n  return a1(from + (to - from) * t) + \"spinner-frames\";\n}\n\
var Gq = W(() => {\n  b1 = c1(0, 1, 0.5);\n});\n\
var Hq = W(() => {\n  Gq();\n});\n\
function d1(y) {\n  return b1 + y + \"theme-name\";\n}\n\
var e1;\n\
var Jq = W(() => {\n  Hq();\n  e1 = d1(2);\n});\n";

/// Answers a module batch per entry by `answer(entry)`; mints a fresh
/// `renamedX` name for every identifier of any other ask, except the
/// identifiers in `decline` (no answer — a sweep whose offers all fail).
struct ModuleProvider {
    answer: fn(&str) -> Option<&'static str>,
    decline: &'static [&'static str],
    prompts: Mutex<Vec<String>>,
    module_prompts: Mutex<Vec<String>>,
    minted: Mutex<usize>,
}

impl ModuleProvider {
    fn new(answer: fn(&str) -> Option<&'static str>) -> Self {
        ModuleProvider {
            answer,
            decline: &[],
            prompts: Mutex::new(Vec::new()),
            module_prompts: Mutex::new(Vec::new()),
            minted: Mutex::new(0),
        }
    }

    fn mint(&self) -> String {
        let mut n = self.minted.lock().expect("lock");
        let mut k = *n;
        *n += 1;
        let mut word = String::from("renamed");
        loop {
            word.push(char::from(b'A' + (k % 26) as u8));
            k /= 26;
            if k == 0 {
                return word;
            }
        }
    }

    fn respond(&self, call: &LlmCall) -> Renames {
        if call.system_prompt != MODULE_NAMER_SYSTEM_PROMPT {
            self.prompts
                .lock()
                .expect("lock")
                .push(call.user_prompt.clone());
            return Renames::from_entries(call.request.identifiers.iter().map(|i| {
                let answer = (!self.decline.contains(&i.as_str())).then(|| self.mint());
                (i.clone(), answer)
            }));
        }
        self.module_prompts
            .lock()
            .expect("lock")
            .push(call.user_prompt.clone());
        Renames::from_entries(call.request.identifiers.iter().map(|key| {
            let entry = call
                .user_prompt
                .split(&format!("### {key}\n"))
                .nth(1)
                .and_then(|rest| rest.split("\n### ").next())
                .unwrap_or("");
            (key.clone(), (self.answer)(entry).map(str::to_string))
        }))
    }
}

impl NameProvider for ModuleProvider {
    fn run_wave(&self, calls: Vec<LlmCall>) -> Vec<Result<BatchRenameResponse, LlmError>> {
        calls
            .iter()
            .map(|c| {
                Ok(BatchRenameResponse {
                    renames: self.respond(c),
                    ..BatchRenameResponse::default()
                })
            })
            .collect()
    }
}

fn config() -> NamingConfig {
    NamingConfig {
        layout: crate::toolchain::BundleLayout::SingleWrapperFunction,
        module_wrappers: crate::toolchain::ModuleWrapperGrammar::BunAndEsbuild,
        name_profile: crate::rename::name_profile::NameProfile::Bun,
        never_rename: crate::rename::eligibility::NeverRename::UNIVERSAL,
        module_group_size: 10,
        skip_libraries: true,
        reconcile_prior_diff: true,
        naming_floor: true,
        naming_floor_sweep: true,
        source_map: false,
        emit_rename_ledger: false,
        family_permute_disabled: false,
        params: humanify_model::llm::CacheKeyParams {
            model: "m".into(),
            temperature: Some(0.0),
            max_tokens: None,
            reasoning_effort: None,
        },
        capture_dump: false,
        tunables: Default::default(),
        shingle_probe: false,
        fast: crate::fast::FastTier::Off,
        prompt_window: DEFAULT_PROMPT_WINDOW,
        module_naming: Some(SplitNamerBudget::default()),
    }
}

fn run(fresh: &str, prior: Option<&str>, provider: &ModuleProvider) -> NamingOutcome {
    let mut log = crate::artifact_dump::DispatchLog::retain_for_tests(config().params);
    run_naming(
        &NamingInput {
            fresh,
            prior,
            library: None,
            claimed_regions: &[],
            era_bridge: None,
            restorations: crate::naming::driver::no_restorations(),
        },
        &config(),
        provider,
        &mut log,
    )
    .expect("the stage runs")
}

/// Does any wave prompt ask for `id`?
fn asked_by_waves(provider: &ModuleProvider, id: &str) -> bool {
    let module = format!("Identifier: {id}\n");
    provider.prompts.lock().expect("lock").iter().any(|p| {
        p.contains(&module)
            || p.lines().any(|l| {
                [
                    "Identifiers to rename: ",
                    "Identifiers still needing names: ",
                ]
                .iter()
                .any(|h| {
                    l.strip_prefix(h)
                        .is_some_and(|ids| ids.split(", ").any(|i| i == id))
                })
            })
    })
}

fn by_purpose(entry: &str) -> Option<&'static str> {
    if entry.contains("\"spinner-frames\"") {
        Some("color-utils")
    } else if entry.contains("\"theme-name\"") {
        Some("theme-picker")
    } else {
        None
    }
}

#[test]
fn a_helper_first_module_is_named_for_its_purpose_and_its_wrapper_follows() {
    let provider = ModuleProvider::new(by_purpose);
    let out = run(BUNDLE, None, &provider);
    let code = out.code.expect("shipped");
    assert!(code.contains("var initColorUtils = __esm("), "{code}");
    assert!(code.contains("var initThemePicker = __esm("), "{code}");
    assert!(
        !asked_by_waves(&provider, "Gq"),
        "the step's wrappers are never asked by a wave"
    );
    assert!(!asked_by_waves(&provider, "Jq"));
    let modules = provider.module_prompts.lock().expect("lock");
    assert_eq!(modules.len(), 1, "two modules, one call");
    let prompt = &modules[0];
    assert!(
        prompt.contains("var MODULE_INIT = __esm(() => {"),
        "{prompt}"
    );
    assert!(prompt.contains("Strings: \"spinner-frames\""), "{prompt}");
    assert!(prompt.contains("Its setup code assigns: "), "{prompt}");
    assert!(
        !prompt.contains("var __esm"),
        "the helper is plumbing: {prompt}"
    );
    assert!(
        !prompt.contains("Gq") && !prompt.contains("Jq") && !prompt.contains("Hq"),
        "{prompt}"
    );
    assert_eq!(out.module_names.asked, 2);
    assert_eq!(
        out.module_names.named,
        vec![
            ("Gq".to_string(), "initColorUtils".to_string()),
            ("Jq".to_string(), "initThemePicker".to_string()),
        ]
    );
}

/// Four modules a theme feature might hold, and two barrels: one loads
/// the three theme modules, one loads a theme module and a clock.
const BARREL_BUNDLE: &str = "var W = (fn, res) => () => (fn && (res = fn(fn = 0)), res);\n\
function a1(x) {\n  return x + \"modal-open\";\n}\n\
var v1;\n\
var Ga = W(() => {\n  v1 = a1(1);\n});\n\
function b1(x) {\n  return x + \"picker-open\";\n}\n\
var v2;\n\
var Gb = W(() => {\n  v2 = b1(2);\n});\n\
function c1(x) {\n  return x + \"store-open\";\n}\n\
var v3;\n\
var Gc = W(() => {\n  v3 = c1(3);\n});\n\
function d1(x) {\n  return x + \"clock-tick\";\n}\n\
var v4;\n\
var Gd = W(() => {\n  v4 = d1(4);\n});\n\
var Hb = W(() => {\n  Ga();\n  Gb();\n  Gc();\n});\n\
var Hc = W(() => {\n  Ga();\n  Gd();\n});\n";

fn theme_parts(entry: &str) -> Option<&'static str> {
    if entry.contains("It is an index file") {
        Some("dashboard-widgets")
    } else if entry.contains("\"modal-open\"") {
        Some("theme-modal")
    } else if entry.contains("\"picker-open\"") {
        Some("theme-picker")
    } else if entry.contains("\"store-open\"") {
        Some("theme-store")
    } else if entry.contains("\"clock-tick\"") {
        Some("clock-face")
    } else {
        None
    }
}

#[test]
fn a_barrel_is_named_from_the_words_its_loaded_modules_share() {
    let provider = ModuleProvider::new(theme_parts);
    let out = run(BARREL_BUNDLE, None, &provider);
    let code = out.code.expect("shipped");
    assert!(code.contains("var initTheme = __esm("), "{code}");
    assert!(
        !asked_by_waves(&provider, "Hb"),
        "a barrel's wrapper is the module step's"
    );
    let prompts = provider.module_prompts.lock().expect("lock");
    assert!(
        prompts
            .iter()
            .all(|p| !p.contains("theme-modal, theme-picker, theme-store")),
        "the rule named it; the model was not asked: {prompts:?}"
    );
    assert_eq!(out.module_names.barrels, 2);
    assert_eq!(
        out.module_names.barrel_rule,
        vec![("Hb".to_string(), "initTheme".to_string())]
    );
}

#[test]
fn a_barrel_whose_modules_share_no_word_is_asked_with_their_names() {
    let provider = ModuleProvider::new(theme_parts);
    let out = run(BARREL_BUNDLE, None, &provider);
    let code = out.code.expect("shipped");
    assert!(code.contains("var initDashboardWidgets = __esm("), "{code}");
    assert!(!asked_by_waves(&provider, "Hc"));
    let prompts = provider.module_prompts.lock().expect("lock");
    let barrel = prompts
        .iter()
        .find(|p| p.contains("It is an index file"))
        .expect("the barrel was asked");
    assert!(
        barrel.contains(
            "It is an index file: its setup code only loads other modules, named: theme-modal, clock-face"
        ),
        "{barrel}"
    );
    assert!(
        barrel.contains(
            "Already taken by other files (pick a DIFFERENT name): theme-modal, clock-face"
        ),
        "{barrel}"
    );
    assert_eq!(out.module_names.barrels_asked, 1);
}

#[test]
fn a_single_module_barrel_is_its_modules_index() {
    let provider = ModuleProvider::new(by_purpose);
    let out = run(BUNDLE, None, &provider);
    let code = out.code.expect("shipped");
    assert!(code.contains("var initColorUtilsIndex = __esm("), "{code}");
    assert!(!asked_by_waves(&provider, "Hq"));
    let modules = provider.module_prompts.lock().expect("lock");
    assert_eq!(
        modules[0].matches("### m").count(),
        2,
        "the barrel is not an entry"
    );
}

fn always_color(entry: &str) -> Option<&'static str> {
    if entry.contains("Already taken by other files") {
        Some("theme-picker")
    } else {
        Some("color-utils")
    }
}

#[test]
fn a_duplicate_name_gets_one_retry_with_the_taken_name_listed() {
    let provider = ModuleProvider::new(always_color);
    let out = run(BUNDLE, None, &provider);
    let code = out.code.expect("shipped");
    assert!(code.contains("var initColorUtils = __esm("), "{code}");
    assert!(code.contains("var initThemePicker = __esm("), "{code}");
    assert_eq!(out.module_names.retried, 1);
    let modules = provider.module_prompts.lock().expect("lock");
    assert_eq!(modules.len(), 2);
    assert!(
        modules[1].contains("Already taken by other files (pick a DIFFERENT name): color-utils"),
        "{}",
        modules[1]
    );
}

#[test]
fn a_module_whose_answers_all_collide_gets_its_first_function_name_never_a_minified_one() {
    // Every module answer is `color-utils`, and the sweep declines the
    // wrapper (its offers all fail): on main `Jq` stays minified.
    let mut provider = ModuleProvider::new(|_| Some("color-utils"));
    provider.decline = &["Jq"];
    let out = run(BUNDLE, None, &provider);
    assert_eq!(out.module_names.retried, 1);
    assert_eq!(out.module_names.disclosed, 1);
    assert_eq!(out.module_names.named.len(), 1);
    let code = out.code.expect("shipped");
    assert!(!code.contains("Jq"), "never left minified: {code}");
    let (old, new) = out.module_names.fallback[0].clone();
    assert_eq!(old, "Jq");
    assert!(
        new.starts_with("initRenamed"),
        "init + the first function's name (`d1`, renamed by the waves): {new}"
    );
    assert!(code.contains(&format!("var {new} = __esm(")), "{code}");
    let modules = provider.module_prompts.lock().expect("lock");
    let disclosed = modules.last().expect("the disclosed retry");
    assert!(
        disclosed.contains(
            "Your earlier answers for this file were refused: \"color-utils\" (another file already has that name)."
        ),
        "{disclosed}"
    );
    assert!(
        disclosed.contains("Already taken by other files (pick a DIFFERENT name): color-utils"),
        "{disclosed}"
    );
}

#[test]
fn a_wrapper_the_prior_carried_is_never_asked_again() {
    let first = run(BUNDLE, None, &ModuleProvider::new(by_purpose));
    let prior = first.code.expect("shipped");
    let provider = ModuleProvider::new(|_| Some("other-name"));
    let out = run(BUNDLE, Some(&prior), &provider);
    assert_eq!(out.module_names.asked, 0);
    assert_eq!(out.module_names.carried, 3, "the barrel's too");
    assert!(provider.module_prompts.lock().expect("lock").is_empty());
    let code = out.code.expect("shipped");
    assert!(code.contains("var initColorUtils = __esm("), "{code}");
}

#[test]
fn the_split_names_each_file_by_its_module_name() {
    use crate::place::assign::fossil::{FossilOptions, assign_fossil};
    let out = run(BUNDLE, None, &ModuleProvider::new(by_purpose));
    let code = out.code.expect("shipped");
    let (inv, values) = crate::twins::statement_inventory_with_values(
        &code,
        "shipped",
        None,
        crate::toolchain::BundleLayout::SingleWrapperFunction,
    )
    .expect("inventory");
    let spans: Vec<(u32, u32)> = inv
        .statements
        .iter()
        .map(|s| (s.span.start, s.span.end))
        .collect();
    let hashes: Vec<String> = inv.statements.iter().map(|s| s.hash.clone()).collect();
    let placed =
        assign_fossil(&values, &spans, &hashes, None, FossilOptions::default()).expect("assign");
    let file_of = |needle: &str| {
        let i = inv
            .statements
            .iter()
            .position(|s| code[s.span.start as usize..s.span.end as usize].contains(needle))
            .unwrap_or_else(|| panic!("{needle} in {code}"));
        placed.assignment[i].clone()
    };
    assert!(
        file_of("var initColorUtils").ends_with("/color-utils.js"),
        "{}",
        file_of("var initColorUtils")
    );
    assert!(file_of("var initThemePicker").ends_with("/theme-picker.js"));
}

// ---- names mode: library placement (exp156) ----------------------------------

use crate::naming::module_names::{ClaimedRegion, EraBridge, LibraryLayout};

/// `BUNDLE`'s modules as the era extracts them: m0 = statements 0..=4
/// (`var W` through `var Gq`), m1 = statement 5 (`var Hq`), m2 = 6..=8.
/// A claimed region for m1 — the whole-claimed one — and one for m2 with
/// unclaimed holes (partial: it must stay).
fn lazy_regions() -> Vec<ClaimedRegion> {
    vec![
        ClaimedRegion {
            statements: vec![5],
            claims: vec![Some("ms".to_string())],
            raw_spans: Vec::new(),
        },
        ClaimedRegion {
            statements: vec![6, 7, 8],
            claims: vec![None, None, Some("ms".to_string())],
            raw_spans: Vec::new(),
        },
    ]
}

fn run_with_regions(
    fresh: &str,
    prior: Option<&str>,
    provider: &ModuleProvider,
    regions: &[ClaimedRegion],
) -> NamingOutcome {
    let mut log = crate::artifact_dump::DispatchLog::retain_for_tests(config().params);
    run_naming(
        &NamingInput {
            fresh,
            prior,
            library: None,
            claimed_regions: regions,
            era_bridge: None,
            restorations: crate::naming::driver::no_restorations(),
        },
        &config(),
        provider,
        &mut log,
    )
    .expect("the stage runs")
}

/// `run_with_regions` with captured restoration answers (exp160).
fn run_with_restorations(
    fresh: &str,
    prior: Option<&str>,
    provider: &ModuleProvider,
    regions: &[ClaimedRegion],
    restorations: &std::collections::BTreeMap<String, RestorationRow>,
) -> NamingOutcome {
    let mut log = crate::artifact_dump::DispatchLog::retain_for_tests(config().params);
    run_naming(
        &NamingInput {
            fresh,
            prior,
            library: None,
            claimed_regions: regions,
            era_bridge: None,
            restorations,
        },
        &config(),
        provider,
        &mut log,
    )
    .expect("the stage runs")
}

/// The era's canonical facts for the program body's `statement`: its
/// whole-print hash and the slot of the mapping entry spelled `declares`
/// — the same walk the era's restoration runs, so the key the test
/// plants is the key the era looks up.
fn era_print_facts(fresh: &str, statement: usize, declares: &str) -> (String, u32) {
    let allocator = oxc_allocator::Allocator::default();
    let ingest = crate::ingest::Ingest::parse(&allocator, fresh, "input.js");
    assert!(ingest.errors.is_empty(), "fixture must parse");
    let tables = crate::hash::serialize::SymbolTables::build(ingest.semantic());
    let json = crate::ingest::program_estree_json(ingest.program);
    let body = crate::cut::container_statement_values(&json, None).expect("program body");
    let prints = crate::libindex::units::statement_named_prints(&body[statement], &tables);
    let slot = prints
        .whole
        .mapping
        .iter()
        .position(|(_, _, name)| name == declares)
        .unwrap_or_else(|| panic!("{declares} is not a slot of statement {statement}"))
        as u32;
    (prints.whole.print.hash, slot)
}

/// exp160: a claimed statement whose index answer is a single name list is
/// named by the INDEX, before the model is ever asked — the restored
/// identifier never appears in a wave prompt, its name ships, and a slot
/// the answer does NOT cover falls back to the era's own naming (the
/// model mints it, exactly as before).
#[test]
fn restoration_names_a_claimed_statement_before_the_model_is_ever_asked() {
    use crate::libindex::names::{NameKind, RestoredName};
    use crate::libindex::table::{RestorationRow, RestoreAnswer};
    // Statement 1 is `function a1(x) { return Math.round(x); }` — m0's
    // second statement, claimed here but NOT moved (lazy_regions leaves
    // m0's statements to the waves).
    let (hash, a1_slot) = era_print_facts(BUNDLE, 1, "a1");
    let _ = era_print_facts(BUNDLE, 1, "x");
    let n = |slot: u32, name: &str| RestoredName {
        slot,
        name: name.into(),
        kind: NameKind::Local,
    };
    let mut restorations = std::collections::BTreeMap::new();
    restorations.insert(
        hash,
        RestorationRow {
            package: "dep".into(),
            // The answer covers a1's slot only: x's slot is missing —
            // the model still names it.
            answer: RestoreAnswer::Single(vec![n(a1_slot, "roundTo")]),
        },
    );
    let regions = [ClaimedRegion {
        statements: vec![1],
        claims: vec![None],
        raw_spans: Vec::new(),
    }];
    let provider = ModuleProvider::new(by_purpose);
    let out = run_with_restorations(BUNDLE, None, &provider, &regions, &restorations);
    let code = out.code.expect("shipped");
    assert!(
        code.contains("function roundTo("),
        "the index's name ships: {code}"
    );
    assert!(!code.contains("a1"), "no crawl name left: {code}");
    assert!(
        !asked_by_waves(&provider, "a1"),
        "the restored identifier is never asked"
    );
    assert!(
        asked_by_waves(&provider, "x"),
        "the missing slot falls back to the model"
    );
}

// ---- exp166: "restore or ask, never freeze" — names mode keeps the model
// off ONLY what the index restored; every other identifier of a moved
// module takes the normal flow (the prior's carry, then the model). And
// the claimed regions reach the era through the exact raw-to-era bridge,
// never through ordinal arithmetic the formatter's splits break. -------------

#[test]
fn a_moved_modules_unrestored_identifiers_are_asked_not_frozen() {
    let provider = ModuleProvider::new(by_purpose);
    let out = run_with_regions(BUNDLE, None, &provider, &lazy_regions());
    assert_eq!(
        out.library_layout.moved,
        vec![(1usize, "vendor/ms/module-55da5bd8.js".to_string())],
        "only the wholly claimed module moves; the partial one stays: {:?}",
        out.library_layout.moved
    );
    // Nothing of it was restored, so nothing of it is frozen: its wrapper
    // takes the module step like every other module (the file path stays
    // the placement's).
    assert_eq!(
        out.module_names.barrels, 1,
        "the moved barrel takes the module step's barrel rule too"
    );
    assert!(
        named_by_module_step(&out, "Hq"),
        "the moved module's wrapper is named: {:?}",
        out.module_names.barrel_rule
    );
    let code = out.code.expect("shipped");
    assert!(
        !code.contains("var Hq = "),
        "an unrestored slot never ships its minified name: {code}"
    );
}

#[test]
fn a_moved_modules_unrestored_identifiers_carry_from_the_prior() {
    // The prior is the same bundle fully named: the moved module's
    // unrestored wrapper takes the prior's name (the default pipeline's
    // carry for unchanged code), not its minified one.
    let first = run(BUNDLE, None, &ModuleProvider::new(by_purpose));
    let prior = first.code.expect("shipped");
    let provider = ModuleProvider::new(|_| Some("other-name"));
    let out = run_with_regions(BUNDLE, Some(&prior), &provider, &lazy_regions());
    assert_eq!(
        out.library_layout.moved,
        vec![(1usize, "vendor/ms/module-55da5bd8.js".to_string())]
    );
    let code = out.code.expect("shipped");
    assert!(!code.contains("var Hq = "), "{code}");
    let wrapper_of = |text: &str| -> String {
        let at = text
            .find("__esm(() => {\n  initColorUtils();")
            .expect("the barrel");
        text[..at].rsplit('\n').next().unwrap().to_string()
    };
    assert_eq!(
        wrapper_of(&code),
        wrapper_of(&prior),
        "the moved module's wrapper carries the prior's name"
    );
}

/// `raw` as the bridge sees it with no unpack edits (runtime = raw): the
/// formatter's output with its statement provenance, every raw program
/// statement as one container statement, and Site B's restored names
/// given as (needle in `raw`, name inside the needle).
fn bridged(raw: &str, restored: &[(&str, &str)]) -> (String, EraBridge, Vec<(u32, u32)>) {
    let formatted =
        crate::format::format_file_with_origins(raw, &crate::format::FormatOptions::default(), &[])
            .expect("formats");
    let spans = raw_statements(raw);
    let restored = restored
        .iter()
        .map(|(needle, name)| {
            let at = raw.find(needle).expect("needle") + needle.find(name).expect("name");
            (at as u32, name.to_string())
        })
        .collect();
    let bridge = EraBridge {
        container: spans.iter().map(|&s| (s, s)).collect(),
        factories: Vec::new(),
        runtime_file: "runtime.js".into(),
        restored,
    };
    (formatted.text, bridge, formatted.statement_origins)
}

fn raw_statements(raw: &str) -> Vec<(u32, u32)> {
    let allocator = oxc_allocator::Allocator::default();
    let ingest = crate::ingest::Ingest::parse(&allocator, raw, "raw.js");
    ingest
        .program
        .body
        .iter()
        .map(|s| {
            let span = oxc_span::GetSpan::span(s);
            (span.start, span.end)
        })
        .collect()
}

fn run_bridged(
    fresh: &str,
    provider: &ModuleProvider,
    regions: &[ClaimedRegion],
    bridge: &EraBridge,
    origins: &[(u32, u32)],
) -> NamingOutcome {
    let mut log = crate::artifact_dump::DispatchLog::retain_for_tests(config().params);
    run_naming(
        &NamingInput {
            fresh,
            prior: None,
            library: None,
            claimed_regions: regions,
            era_bridge: Some(crate::naming::driver::EraBridgeInput { bridge, origins }),
            restorations: crate::naming::driver::no_restorations(),
        },
        &config(),
        provider,
        &mut log,
    )
    .expect("the stage runs")
}

#[test]
fn a_restored_slot_is_never_asked() {
    // Site B restored the moved module's wrapper (the bridge carries the
    // name it wrote at its raw offset): that one binding is held — never
    // asked by a wave or the module step, its name ships — while the
    // module's other identifiers stay in the normal flow.
    let (fresh, bridge, origins) = bridged(BUNDLE, &[("var Hq = W", "Hq")]);
    let raw = raw_statements(BUNDLE);
    let regions = [ClaimedRegion {
        statements: Vec::new(),
        claims: vec![Some("ms".to_string())],
        raw_spans: vec![raw[5]],
    }];
    let provider = ModuleProvider::new(by_purpose);
    let out = run_bridged(&fresh, &provider, &regions, &bridge, &origins);
    assert_eq!(
        out.library_layout.moved.len(),
        1,
        "{:?}",
        out.library_layout.moved
    );
    assert!(
        !asked_by_waves(&provider, "Hq"),
        "the restored slot is never asked"
    );
    assert!(
        !named_by_module_step(&out, "Hq"),
        "the restored wrapper is settled, never renamed by the module step"
    );
    assert_eq!(out.module_names.carried, 1, "it counts as carried");
    let code = out.code.expect("shipped");
    assert!(
        code.contains("var Hq = __esm("),
        "the restored name ships: {code}"
    );
}

/// The cc 2.1.215 shape (exp166's trace): a raw statement the formatter
/// splits (`var p = 1, q = 2, r = 3;` → three statements) sits BEFORE an
/// app module and a claimed library module. Ordinal arithmetic put the
/// library's claims on the app module two statements earlier (the plugin
/// installer filed under `vendor/jose/`); the bridge puts them on the
/// library's own statements.
const DRIFT_BUNDLE: &str = "var W = (fn, res) => () => (fn && (res = fn(fn = 0)), res);\n\
var p = 1, q = 2, r = 3;\n\
var A = W(() => {\n  p = q + r;\n});\n\
function app1(y) {\n  return y + \"plugin-installer\";\n}\n\
var P = W(() => {\n  app1(2);\n});\n\
function lib1(x) {\n  return x + \"jose-1\";\n}\n\
var J = W(() => {\n  lib1(1);\n});\n";

#[test]
fn a_library_region_after_a_split_statement_moves_its_own_module_not_the_app_one() {
    let (fresh, bridge, origins) = bridged(DRIFT_BUNDLE, &[]);
    let raw = raw_statements(DRIFT_BUNDLE);
    // The region as the unpack reports it: raw statements 5 and 6 (lib1,
    // J) claimed jose — and the ordinals the old arithmetic gave them.
    let regions = [ClaimedRegion {
        statements: vec![5, 6],
        claims: vec![Some("jose".to_string()), Some("jose".to_string())],
        raw_spans: vec![raw[5], raw[6]],
    }];
    let provider = ModuleProvider::new(by_purpose);
    let out = run_bridged(&fresh, &provider, &regions, &bridge, &origins);
    let moved = &out.library_layout.moved;
    assert_eq!(moved.len(), 1, "{moved:?}");
    assert!(moved[0].1.starts_with("vendor/jose/"), "{moved:?}");
    let code = out.code.expect("shipped");
    assert!(
        !code.contains("function app1(") && !code.contains("var P = "),
        "the app module is named like app code: {code}"
    );
    let conversion = out.library_layout.conversion.expect("converted");
    assert_eq!((conversion.converted, conversion.dropped), (1, 0));
    // The moved module is the library's: its statements are the raw
    // statements the region claimed, no other.
    let moved_raw: Vec<(u32, u32)> = out
        .library_layout
        .moved_spans
        .iter()
        .map(|span| {
            let i = out
                .library_layout
                .era_raw
                .iter()
                .zip(era_spans(&fresh))
                .position(|(_, s)| s == *span)
                .expect("an era statement");
            out.library_layout.era_raw[i].expect("bridged")
        })
        .collect();
    assert_eq!(moved_raw, vec![raw[5], raw[6]]);
    // The diagnostics' baseline: the retired ordinal arithmetic, run over
    // the same groups, moves the APP module (the trace's bug, reproduced).
    let ordinal = &out.library_layout.ordinal_moved_files;
    assert_eq!(ordinal.len(), 1, "{ordinal:?}");
    let app_spans: Vec<(u32, u32)> = era_spans(&fresh)
        .into_iter()
        .zip(&out.library_layout.era_raw)
        .filter(|(_, r)| **r == Some(raw[3]) || **r == Some(raw[4]))
        .map(|(s, _)| s)
        .collect();
    assert!(ordinal[0].0.starts_with("vendor/jose/"), "{ordinal:?}");
    assert_eq!(
        ordinal[0].1, app_spans,
        "the ordinal plan moved the app module"
    );
}

fn named_by_module_step(out: &NamingOutcome, wrapper: &str) -> bool {
    let m = &out.module_names;
    m.named
        .iter()
        .chain(&m.named_on_disclosure)
        .chain(&m.barrel_rule)
        .chain(&m.fallback)
        .any(|(old, _)| old == wrapper)
}

fn era_spans(fresh: &str) -> Vec<(u32, u32)> {
    raw_statements(fresh)
}

#[test]
fn an_unjoinable_region_moves_nothing() {
    // A region whose statements are no module's segment (here: the
    // barrel module's head alone, missing its pendings) is ignored, not
    // force-fitted: conservative is the policy.
    let provider = ModuleProvider::new(by_purpose);
    let regions = vec![ClaimedRegion {
        statements: vec![5, 7],
        claims: vec![Some("ms".to_string()), Some("ms".to_string())],
        raw_spans: Vec::new(),
    }];
    let out = run_with_regions(BUNDLE, None, &provider, &regions);
    assert!(
        out.library_layout.moved.is_empty(),
        "an unjoinable region moves nothing"
    );
}

// ---- exp158: the synthetic name is the module's own statement-set hash,
// the join stays strict (the drift-tolerant relaxation was measured and
// refuted — see plan_library_modules), and a module two packages claim
// never moves -------------------------------------------------------------------

use crate::naming::module_names::{ModuleSource, plan_library_modules};

/// The naming era, purely: parse `source`, take the program body (no
/// wrapper), and plan the claimed regions against the era's extraction.
fn era_layout(source: &str, regions: &[ClaimedRegion]) -> LibraryLayout {
    let allocator = oxc_allocator::Allocator::default();
    let ingest = crate::ingest::Ingest::parse(&allocator, source, "test.js");
    let json = crate::ingest::program_estree_json(ingest.program);
    let body = crate::cut::container_statement_values(&json, None)
        .expect("program body")
        .clone();
    plan_library_modules(
        &ModuleSource {
            body: &body,
            text: source,
            grammar: crate::toolchain::ModuleWrapperGrammar::BunAndEsbuild,
        },
        regions,
    )
}

fn claimed_all(statements: &[usize], package: &str) -> ClaimedRegion {
    ClaimedRegion {
        statements: statements.to_vec(),
        claims: statements
            .iter()
            .map(|_| Some(package.to_string()))
            .collect(),
        raw_spans: Vec::new(),
    }
}

/// Three modules whose wrapper statements are the same blurred shape
/// (`var mX = W(() => { gY(); })` — the identifiers the hash masks), each
/// preceded by its own helper whose literal differs: m0 = 0..=2 (with the
/// W helper), m1 = 3..=4, m2 = 5..=6. The wrappers' head hashes TIE; the
/// modules' contents differ.
const TIE_BUNDLE: &str = "var W = (fn, res) => () => (fn && (res = fn(fn = 0)), res);\n\
function hc(x) {\n  return x + \"c-one\";\n}\n\
var mc = W(() => {\n  gA();\n});\n\
function ha(x) {\n  return x + \"a-two\";\n}\n\
var ma = W(() => {\n  gB();\n});\n\
function hb(x) {\n  return x + \"b-three\";\n}\n\
var mb = W(() => {\n  gC();\n});\n";

// exp158, the brief's composition fixture: the same library module
// discovered with a sibling added elsewhere in the package must land the
// SAME vendor path. Under the head-hash scheme the tied wrappers fell to
// arrival order (`-2`, `-3`), so the sibling — sorting first — renamed
// every survivor.
#[test]
fn a_composition_change_elsewhere_in_the_package_leaves_the_shared_names_alone() {
    let two = era_layout(
        TIE_BUNDLE,
        &[claimed_all(&[3, 4], "ms"), claimed_all(&[5, 6], "ms")],
    );
    // The next version discovers m0's region too.
    let three = era_layout(
        TIE_BUNDLE,
        &[
            claimed_all(&[0, 1, 2], "ms"),
            claimed_all(&[3, 4], "ms"),
            claimed_all(&[5, 6], "ms"),
        ],
    );
    assert_eq!(two.moved.len(), 2, "{:?}", two.moved);
    assert_eq!(three.moved.len(), 3, "{:?}", three.moved);
    assert_eq!(
        two.moved[0], three.moved[1],
        "the shared module keeps its name whatever else joins the package"
    );
    assert_eq!(
        two.moved[1], three.moved[2],
        "the shared module keeps its name whatever else joins the package"
    );
}

#[test]
fn tied_head_hashes_get_distinct_names_when_the_statement_sets_differ() {
    let out = era_layout(
        TIE_BUNDLE,
        &[
            claimed_all(&[0, 1, 2], "ms"),
            claimed_all(&[3, 4], "ms"),
            claimed_all(&[5, 6], "ms"),
        ],
    );
    assert_eq!(out.moved.len(), 3, "{:?}", out.moved);
    let (_, path_a) = &out.moved[0];
    let (_, path_b) = &out.moved[1];
    let (_, path_c) = &out.moved[2];
    assert_ne!(path_a, path_b, "distinct contents, distinct names");
    assert_ne!(path_b, path_c, "distinct contents, distinct names");
    assert_ne!(path_a, path_c, "distinct contents, distinct names");
    for (i, (_, path)) in out.moved.iter().enumerate() {
        assert!(
            path.starts_with("vendor/ms/module-") && path.ends_with(".js"),
            "module {i} takes the planner's synthetic spelling: {path}"
        );
        assert!(
            !path.contains("-2"),
            "the members are distinguished by content, not arrival: {path}"
        );
    }
}

#[test]
fn a_content_tie_keeps_its_members_distinct_by_arrival_order() {
    // Two modules whose STATEMENT SETS hash identically (same helper, same
    // wrapper shape) cannot share a path: the first by (sort key, arrival)
    // keeps the plain name, the next takes -2 — bundle order decides ONLY
    // inside a content tie.
    let bundle = "var W = (fn, res) => () => (fn && (res = fn(fn = 0)), res);\n\
function ha(x) {\n  return x + \"same\";\n}\n\
var ma = W(() => {\n  g();\n});\n\
function hb(x) {\n  return x + \"same\";\n}\n\
var mb = W(() => {\n  g();\n});\n\
function hcm(x) {\n  return x + \"same\";\n}\n\
var mc2 = W(() => {\n  g();\n});\n";
    let out = era_layout(
        bundle,
        &[claimed_all(&[3, 4], "ms"), claimed_all(&[5, 6], "ms")],
    );
    assert_eq!(out.moved.len(), 2, "{:?}", out.moved);
    assert_eq!(out.moved[0].0, 1);
    assert_eq!(out.moved[1].0, 2);
    let plain = out.moved[0].1.clone();
    let second = out.moved[1].1.clone();
    assert!(plain.starts_with("vendor/ms/module-"), "{plain}");
    assert!(!plain.contains("-2"), "{plain}");
    assert_eq!(
        second,
        format!("{}-2.js", plain.strip_suffix(".js").expect("a .js path")),
        "the exact content tie falls back to arrival order"
    );
}

// exp158, the join stays STRICT (the drift-tolerant relaxation was
// measured and refuted on the 2.1.85→2.1.86 pair — every overlap bar from
// 0.5 to 0.95, in all three forms, INCREASED the era's one-sided placed
// paths instead of collapsing the trulyAdded files it targeted: the
// near-miss relationships do not replicate across versions): a region
// whose statement set has drifted off its module's segment does not join,
// whatever the overlap.
#[test]
fn a_region_drifted_off_its_module_does_not_join() {
    // m2 = statements 6..=8; the region reports 6,7 — two thirds of both
    // sides, and still no join: only an exact statement set joins.
    let out = era_layout(BUNDLE, &[claimed_all(&[6, 7], "ms")]);
    assert!(
        out.moved.is_empty(),
        "a drifted region does not join: {:?}",
        out.moved
    );
}

#[test]
fn a_region_the_module_mostly_does_not_hold_does_not_join() {
    // Statement 7 alone sits in m2 = 6..=8.
    let out = era_layout(BUNDLE, &[claimed_all(&[7], "ms")]);
    assert!(
        out.moved.is_empty(),
        "a one-of-three region does not join: {:?}",
        out.moved
    );
}

// exp158, the package guard: a module two packages claim (here the region
// covers m2 exactly, but statement 8 — its own wrapper — is claimed by
// zod) is never moved, whoever agrees: moving it would merge two
// packages' code into one vendor file.
#[test]
fn a_module_two_packages_claim_is_never_moved() {
    let regions = vec![claimed_all(&[6, 7, 8], "ms"), claimed_all(&[8], "zod")];
    let out = era_layout(BUNDLE, &regions);
    assert!(
        out.moved.is_empty(),
        "a contested module never moves: {:?}",
        out.moved
    );
}
