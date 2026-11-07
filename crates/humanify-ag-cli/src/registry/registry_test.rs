//! The registry machinery on a toy table (the pipeline's own table is held
//! by config_file_test.rs, surface_test.rs and the help goldens): what the
//! harness needs beyond the pipeline — repeated list flags, a registry's
//! own retired flags, "did you mean" for a flag, typed `key=value` text.

use clap::ArgMatches;
use std::sync::LazyLock;

use super::flags::{FlagSpec, add_flags, flag_values, specs};
use super::{Key, Kind, Registry, Source, Val, resolve};

fn none() -> Option<Val> {
    None
}
fn modes() -> Vec<String> {
    vec!["scratch".into(), "seeded".into()]
}

static TOY_KEYS: &[Key] = &[
    Key {
        section: "run",
        name: "workdir",
        kind: Kind::Text,
        flag: Some("--workdir"),
        default: || Some(Val::Text("/work".into())),
        unset: "",
        doc: "Where runs write.",
    },
    Key {
        section: "run",
        name: "pipeline_set",
        kind: Kind::List(None),
        flag: Some("--pipeline-set"),
        default: || Some(Val::List(Vec::new())),
        unset: "",
        doc: "One pipeline setting; repeatable.",
    },
    Key {
        section: "eval",
        name: "base_mode",
        kind: Kind::Choice(modes),
        flag: Some("--base-mode"),
        default: || Some(Val::Text("scratch".into())),
        unset: "",
        doc: "How v-1 is built.",
    },
    Key {
        section: "eval",
        name: "self_hop",
        kind: Kind::Bool,
        flag: Some("--self-hop"),
        default: || Some(Val::Bool(true)),
        unset: "",
        doc: "Run the self-hop.",
    },
    Key {
        section: "libscore",
        name: "jobs",
        kind: Kind::Int { min: 1, max: 64 },
        flag: Some("--jobs"),
        default: none,
        unset: "one per CPU",
        doc: "Parallel jobs.",
    },
];

static TOY: Registry = Registry {
    keys: TOY_KEYS,
    retired_keys: &[],
    retired_flags: &[("--seeded-base", "renamed: use --base-mode seeded")],
    short_flags: &[],
    counted_flag: None,
    repeatable: &["run.pipeline_set"],
    section_titles: &[],
    short_help_hides: |_| false,
    value_names: &[],
};

static TOY_FLAGS: LazyLock<Vec<FlagSpec>> = LazyLock::new(|| specs(&TOY));

fn matches(args: &[&str]) -> Result<ArgMatches, clap::Error> {
    let cmd = add_flags(clap::Command::default().name("toy"), TOY_FLAGS.as_slice());
    cmd.try_get_matches_from(std::iter::once("toy").chain(args.iter().copied()))
}

#[test]
fn a_repeatable_list_flag_keeps_every_use_in_order() {
    let m = matches(&[
        "--pipeline-set",
        "switches.disable=a,b",
        "--pipeline-set",
        "library.matching=off",
    ])
    .expect("parses");
    let flags = flag_values(TOY_FLAGS.as_slice(), &m).expect("typed");
    assert_eq!(
        flags,
        vec![(
            "run.pipeline_set",
            Val::List(vec![
                "switches.disable=a,b".into(),
                "library.matching=off".into()
            ])
        )],
        "one item per use, order kept, a comma inside a value is not a separator"
    );
}

#[test]
fn a_non_repeatable_flag_given_twice_is_refused() {
    assert!(matches(&["--workdir", "a", "--workdir", "b"]).is_err());
}

#[test]
fn a_registry_refuses_its_own_retired_flags_naming_the_replacement() {
    let refusal = TOY
        .retired_flag(&["x".into(), "--seeded-base".into()])
        .expect("refused");
    assert!(refusal.contains("--seeded-base"), "{refusal}");
    assert!(refusal.contains("--base-mode seeded"), "{refusal}");
    assert_eq!(
        TOY.retired_flag(&["--".into(), "--seeded-base".into()]),
        None,
        "nothing after -- is a flag"
    );
}

#[test]
fn an_unknown_flag_gets_the_nearest_spelling() {
    assert_eq!(TOY.nearest_flag("--workdr").as_deref(), Some("--workdir"));
    assert_eq!(
        TOY.nearest_flag("--no-self-hp").as_deref(),
        Some("--no-self-hop")
    );
    assert_eq!(TOY.nearest_flag("--entirely-different"), None);
}

#[test]
fn key_value_text_is_typed_by_the_key_and_range_checked_apart() {
    let jobs = TOY.key("libscore.jobs");
    assert_eq!(Registry::typed_text(jobs, "jobs", "8"), Ok(Val::Int(8)));
    assert!(Registry::typed_text(jobs, "jobs", "8x").is_err());
    let hop = TOY.key("eval.self_hop");
    assert_eq!(
        Registry::typed_text(hop, "x", "false"),
        Ok(Val::Bool(false))
    );
    assert!(Registry::typed_text(hop, "x", "no").is_err());
    let out_of_range = super::check_range(jobs, "--jobs", &Val::Int(65));
    assert!(out_of_range.is_some_and(|e| e.contains("--jobs")));
}

#[test]
fn resolution_over_a_toy_registry_records_sources_and_values() {
    let flags = vec![("eval.base_mode", Val::Text("seeded".into()))];
    let (r, errors) = resolve(&TOY, &flags, None);
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(r.text("eval.base_mode").as_deref(), Some("seeded"));
    assert_eq!(r.source("eval.base_mode"), Source::Flag);
    assert_eq!(r.source("run.workdir"), Source::Default);
    assert_eq!(r.int("libscore.jobs"), None, "unset stays unset");
    let keys: Vec<&str> = r.values().map(|(k, _, _)| k).collect();
    assert_eq!(
        keys,
        vec![
            "eval.base_mode",
            "eval.self_hop",
            "run.pipeline_set",
            "run.workdir"
        ]
    );
    let (_, errors) = resolve(&TOY, &[("eval.base_mode", Val::Text("x".into()))], None);
    assert_eq!(errors.len(), 1, "{errors:?}");
}
