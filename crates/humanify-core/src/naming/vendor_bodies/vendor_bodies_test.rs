use std::collections::BTreeSet;
use std::sync::Mutex;

use humanify_model::llm::{BatchRenameResponse, LlmCall, LlmError, NameProvider, Renames};

use super::{VendorBody, join_bodies, name_vendor_bodies, split_bodies};

/// Names every asked identifier `<prefix><Id>` and records what it was asked.
struct Namer {
    prefix: &'static str,
    asked: Mutex<Vec<String>>,
}

impl Namer {
    fn new(prefix: &'static str) -> Self {
        Namer {
            prefix,
            asked: Mutex::new(Vec::new()),
        }
    }
    fn asked(&self) -> Vec<String> {
        self.asked.lock().expect("lock").clone()
    }
}

impl NameProvider for Namer {
    fn run_wave(&self, calls: Vec<LlmCall>) -> Vec<Result<BatchRenameResponse, LlmError>> {
        calls
            .into_iter()
            .map(|c| {
                self.asked
                    .lock()
                    .expect("lock")
                    .extend(c.request.identifiers.iter().cloned());
                Ok(BatchRenameResponse {
                    renames: Renames::from_entries(c.request.identifiers.iter().map(|i| {
                        let mut cap = i.trim_start_matches('$').to_string();
                        cap[..1].make_ascii_uppercase();
                        (i.clone(), Some(format!("{}{cap}", self.prefix)))
                    })),
                    finish_reason: None,
                    usage: None,
                })
            })
            .collect()
    }
}

fn config() -> crate::naming::driver::NamingConfig {
    crate::naming::driver::NamingConfig {
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
        prompt_window: crate::naming::waves::processor::DEFAULT_PROMPT_WINDOW,
        module_naming: None,
    }
}

fn log() -> crate::artifact_dump::DispatchLog {
    crate::artifact_dump::DispatchLog::off(config().params)
}

fn body(path: &str, text: &str, restored: &[&str]) -> VendorBody {
    VendorBody {
        path: path.into(),
        text: text.into(),
        restored: restored
            .iter()
            .map(|s| s.to_string())
            .collect::<BTreeSet<_>>(),
    }
}

const SEMVER_LIKE: &str = "function(e,t){var r=require(\"./x.js\"),n=(i,o)=>{if(i instanceof r.f)return i;return new r.f(i,o)};t.exports=n}";

#[test]
fn the_joined_text_splits_back_into_its_bodies() {
    let bodies = ["function(e,t){t.exports=1}", "(e,t)=>{t.exports=e}"];
    let joined = join_bodies(bodies);
    assert_eq!(split_bodies(&joined, 2).expect("split"), bodies);
    assert!(
        split_bodies(&joined, 3).is_err(),
        "a count mismatch is refused"
    );
}

#[test]
fn an_unrestored_factory_bodys_identifiers_are_asked_of_the_model() {
    let namer = Namer::new("asked");
    let out = name_vendor_bodies(
        &[body("vendor/semver/functions/coerce.js", SEMVER_LIKE, &[])],
        None,
        &config(),
        &namer,
        &mut log(),
    )
    .expect("runs")
    .expect("named");
    let asked = namer.asked();
    for id in ["r", "n", "i", "o"] {
        assert!(asked.iter().any(|a| a == id), "{id} asked: {asked:?}");
    }
    let text = &out.texts[0];
    assert!(text.contains("askedN"), "{text}");
    assert!(
        text.contains("require(\"./x.js\")"),
        "free identifiers stay: {text}"
    );
    assert!(text.trim_start().starts_with("function"), "{text}");
}

#[test]
fn a_restored_name_is_held_not_asked() {
    let namer = Namer::new("asked");
    let text = "function(e,t){var parseVersion=function(i){return i.split(\".\")},r=parseVersion(e);t.exports=r}";
    let out = name_vendor_bodies(
        &[body("vendor/a/b.js", text, &["parseVersion"])],
        None,
        &config(),
        &namer,
        &mut log(),
    )
    .expect("runs")
    .expect("named");
    let asked = namer.asked();
    assert!(!asked.iter().any(|a| a == "parseVersion"), "{asked:?}");
    assert!(asked.iter().any(|a| a == "r"), "{asked:?}");
    assert!(out.texts[0].contains("parseVersion"), "{}", out.texts[0]);
}

#[test]
fn an_unchanged_body_carries_its_names_from_the_prior() {
    let first = name_vendor_bodies(
        &[body("vendor/semver/functions/coerce.js", SEMVER_LIKE, &[])],
        None,
        &config(),
        &Namer::new("prior"),
        &mut log(),
    )
    .expect("runs")
    .expect("named");
    // The next release: the same body under different minifier letters.
    let next = SEMVER_LIKE
        .replace("(e,t)", "(a,b)")
        .replace("t.exports", "b.exports")
        .replace("var r=", "var s=")
        .replace("r.f", "s.f")
        .replace(",n=", ",u=")
        .replace("=n}", "=u}");
    let namer = Namer::new("fresh");
    let out = name_vendor_bodies(
        &[body("vendor/semver/functions/coerce.js", &next, &[])],
        Some(&first.joined),
        &config(),
        &namer,
        &mut log(),
    )
    .expect("runs")
    .expect("named");
    assert!(out.prior_used);
    assert!(namer.asked().is_empty(), "asked {:?}", namer.asked());
    assert_eq!(out.texts[0], first.texts[0]);
}

#[test]
fn bodies_are_joined_in_path_order_and_returned_in_input_order() {
    let out = name_vendor_bodies(
        &[
            body("vendor/z/z.js", "function(e,t){var r=1;t.exports=r}", &[]),
            body("vendor/a/a.js", "function(e,t){var q=2;t.exports=q}", &[]),
        ],
        None,
        &config(),
        &Namer::new("asked"),
        &mut log(),
    )
    .expect("runs")
    .expect("named");
    assert!(out.texts[0].contains("askedR"), "{}", out.texts[0]);
    assert!(out.texts[1].contains("askedQ"), "{}", out.texts[1]);
    let a = out.joined.find("askedQ").expect("a");
    let z = out.joined.find("askedR").expect("z");
    assert!(a < z, "path order: {}", out.joined);
}

#[test]
fn a_body_with_a_lone_surrogate_ships_as_extracted_and_the_rest_still_carry() {
    let surrogate = "function(e,t){var r=\"\\uD800-\\uDFFF\";t.exports=r}";
    let first = name_vendor_bodies(
        &[
            body("vendor/semver/functions/coerce.js", SEMVER_LIKE, &[]),
            body("vendor/xml/chars.js", surrogate, &[]),
        ],
        None,
        &config(),
        &Namer::new("prior"),
        &mut log(),
    )
    .expect("runs")
    .expect("named");
    assert_eq!(first.texts[1], surrogate, "shipped as extracted");
    assert_eq!(
        first.skipped,
        vec![(
            "vendor/xml/chars.js".to_string(),
            super::Skip::LoneSurrogate
        )]
    );
    let namer = Namer::new("fresh");
    let out = name_vendor_bodies(
        &[
            body("vendor/semver/functions/coerce.js", SEMVER_LIKE, &[]),
            body("vendor/xml/chars.js", surrogate, &[]),
        ],
        Some(&first.joined),
        &config(),
        &namer,
        &mut log(),
    )
    .expect("runs")
    .expect("named");
    assert!(out.prior_used, "the prior is read");
    assert!(namer.asked().is_empty(), "asked {:?}", namer.asked());
    assert_eq!(out.texts[0], first.texts[0]);
}

#[test]
fn a_body_with_a_line_too_long_for_a_prompt_ships_as_extracted() {
    let long = format!(
        "function(e,t){{var r=\"{}\";t.exports=r}}",
        "x".repeat(super::MAX_LINE_CHARS + 1)
    );
    let namer = Namer::new("asked");
    let out = name_vendor_bodies(
        &[
            body("vendor/hl/lang.js", &long, &[]),
            body("vendor/a/a.js", "function(e,t){var q=2;t.exports=q}", &[]),
        ],
        None,
        &config(),
        &namer,
        &mut log(),
    )
    .expect("runs")
    .expect("named");
    assert_eq!(out.texts[0], long);
    assert!(out.texts[1].contains("askedQ"), "{}", out.texts[1]);
    assert!(
        !namer.asked().iter().any(|a| a == "r"),
        "{:?}",
        namer.asked()
    );
    assert_eq!(
        out.skipped,
        vec![("vendor/hl/lang.js".to_string(), super::Skip::LongLine)]
    );
}
