//! The module step end to end through the naming stage
//! (docs/design/module-naming.md): a helper-first module gets a name for
//! what it does, its wrapper becomes `init<Name>` and the split names its
//! file `<name>.js`; a barrel is left to the waves; a duplicate gets one
//! retry with the taken name listed; a wrapper the prior carried is never
//! asked again.

use std::sync::Mutex;

use humanify_model::llm::{BatchRenameResponse, LlmCall, LlmError, NameProvider, Renames};

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

use crate::naming::module_names::{ClaimedRegion, LibraryLayout};

/// `BUNDLE`'s modules as the era extracts them: m0 = statements 0..=4
/// (`var W` through `var Gq`), m1 = statement 5 (`var Hq`), m2 = 6..=8.
/// A claimed region for m1 — the whole-claimed one — and one for m2 with
/// unclaimed holes (partial: it must stay).
fn lazy_regions() -> Vec<ClaimedRegion> {
    vec![
        ClaimedRegion {
            statements: vec![5],
            claims: vec![Some("ms".to_string())],
        },
        ClaimedRegion {
            statements: vec![6, 7, 8],
            claims: vec![None, None, Some("ms".to_string())],
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
        },
        &config(),
        provider,
        &mut log,
    )
    .expect("the stage runs")
}

#[test]
fn a_wholly_claimed_module_moves_to_vendor_and_is_never_asked() {
    let provider = ModuleProvider::new(by_purpose);
    let out = run_with_regions(BUNDLE, None, &provider, &lazy_regions());
    let layout: &LibraryLayout = &out.library_layout;
    assert_eq!(
        layout.moved,
        vec![(1usize, "vendor/ms/module-2374bc3b.js".to_string())],
        "only the wholly claimed module moves; the partial one stays: {:?}",
        layout.moved
    );
    // The model names nothing of the moved module: not the wrapper, not
    // the body's function, not the file (the module step skips it).
    assert!(
        !asked_by_waves(&provider, "Hq"),
        "the moved module's wrapper is never asked"
    );
    // Not a module-step entry: the step asked only the OTHER two modules
    // (the prompt keys renumber the asked set), and none of its applied
    // names is the moved wrapper's.
    assert_eq!(
        out.module_names.asked, 2,
        "three modules, one moved: only two are asked"
    );
    assert!(
        out.module_names
            .named
            .iter()
            .chain(&out.module_names.named_on_disclosure)
            .chain(&out.module_names.fallback)
            .all(|(old, _)| old != "Hq"),
        "the moved module's wrapper is never named: {:?}",
        out.module_names.named
    );
    // Its identifiers stay exactly as they were — the wrapper keeps its
    // own minified name (the lazy helper's rename to `__esm` is the
    // plumbing pass's, module-wide, not the moved module's identifier).
    let code = out.code.expect("shipped");
    assert!(code.contains("var Hq = __esm("), "{code}");
    assert!(
        !code.contains("var init") || code.contains("var initColorUtils"),
        "the OTHER modules still name normally: {code}"
    );
}

#[test]
fn a_moved_modules_exclusion_holds_over_a_prior_too() {
    // The prior is the same bundle fully named; with the region claimed,
    // the transfer must not carry the prior's names into the moved
    // module either — its statements are library-placed, names as-is.
    let first = run(BUNDLE, None, &ModuleProvider::new(by_purpose));
    let prior = first.code.expect("shipped");
    let provider = ModuleProvider::new(|_| Some("other-name"));
    let out = run_with_regions(BUNDLE, Some(&prior), &provider, &lazy_regions());
    let layout = &out.library_layout;
    assert_eq!(
        layout.moved,
        vec![(1usize, "vendor/ms/module-2374bc3b.js".to_string())]
    );
    let code = out.code.expect("shipped");
    assert!(
        code.contains("var Hq = __esm("),
        "the moved module keeps its own names over a prior: {code}"
    );
    assert!(
        !asked_by_waves(&provider, "Hq"),
        "and nothing inside it is asked"
    );
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
    }];
    let out = run_with_regions(BUNDLE, None, &provider, &regions);
    assert!(
        out.library_layout.moved.is_empty(),
        "an unjoinable region moves nothing"
    );
}
