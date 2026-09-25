//! The WPB.4 flag-surface gate: the Rust pipeline program against the REAL
//! commander program, recorded by test/parity/wpb4-cli-probe.ts into
//! test/parity/wpb4-cli-surface.json.
//!
//! Three comparisons, every one exact:
//! 1. the option TABLE — per command, every option's flags, short, long,
//!    negate/required/optional/variadic, attributeName, default and
//!    description, in declaration order (Rust-only options are named in
//!    `RUST_ONLY_OPTIONS` and printed, never silently extra);
//! 2. the rendered HELP text (what every usage error prints to stderr);
//! 3. the PARSE corpus — every recorded argv's outcome: the action's
//!    command, arguments, option values and value sources; or commander's
//!    exit code, error code and exact stdout/stderr bytes.

use serde_json::Value;

use crate::commander::{ParseOutcome, ValueSource};
use crate::surface::{RUST_ONLY_OPTIONS, program};

fn surface() -> Value {
    let path = format!(
        "{}/../../test/parity/wpb4-cli-surface.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(&path).expect("the committed surface vectors");
    serde_json::from_str(&text).expect("surface JSON")
}

fn command_by_name<'a>(
    root: &'a crate::commander::CliCommand,
    name: &str,
) -> &'a crate::commander::CliCommand {
    if root.name == name {
        return root;
    }
    root.commands
        .iter()
        .find(|c| c.name == name)
        .unwrap_or_else(|| panic!("the Rust program lacks command {name}"))
}

#[test]
fn option_table_matches_commander_exactly() {
    let s = surface();
    let root = program();
    let mut compared = 0usize;
    let mut mismatches: Vec<String> = Vec::new();
    for ts_cmd in s["commands"].as_array().unwrap() {
        let name = ts_cmd["name"].as_str().unwrap();
        let rs_cmd = command_by_name(&root, name);
        assert_eq!(rs_cmd.description, ts_cmd["description"].as_str().unwrap());
        let rs_opts: Vec<_> = rs_cmd
            .options
            .iter()
            .filter(|o| !RUST_ONLY_OPTIONS.contains(&o.flags.as_str()))
            .collect();
        let ts_opts = ts_cmd["options"].as_array().unwrap();
        if rs_opts.len() != ts_opts.len() {
            mismatches.push(format!(
                "{name}: {} Rust options vs {} TS",
                rs_opts.len(),
                ts_opts.len()
            ));
        }
        for (i, t) in ts_opts.iter().enumerate() {
            let Some(r) = rs_opts.get(i) else {
                mismatches.push(format!("{name}: missing option #{i} {}", t["flags"]));
                continue;
            };
            let rs = serde_json::json!({
                "flags": r.flags,
                "short": r.short,
                "long": r.long,
                "negate": r.negate,
                "required": r.required,
                "optional": r.optional,
                "variadic": r.variadic,
                "attributeName": r.attribute_name(),
                "defaultValue": r.default_value.clone().unwrap_or(Value::Null),
                "description": r.description,
            });
            if &rs != t {
                mismatches.push(format!("{name} option #{i}:\n  ts   {t}\n  rust {rs}"));
            }
            compared += 1;
        }
        let ts_args = ts_cmd["arguments"].as_array().unwrap();
        assert_eq!(rs_cmd.arguments.len(), ts_args.len(), "{name} arguments");
        for (r, t) in rs_cmd.arguments.iter().zip(ts_args) {
            assert_eq!(r.name, t["name"].as_str().unwrap());
            assert_eq!(r.required, t["required"].as_bool().unwrap());
            assert_eq!(r.variadic, t["variadic"].as_bool().unwrap());
            assert_eq!(r.description, t["description"].as_str().unwrap());
        }
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
    eprintln!(
        "surface: {compared} options identical; Rust-only (declared): {:?}",
        RUST_ONLY_OPTIONS
    );
    assert_eq!(compared, 41, "39 pipeline options (incl. -V) + 2 env-reads");
}

#[test]
fn help_text_matches_commander_byte_for_byte() {
    let s = surface();
    let root = program();
    for ts_cmd in s["commands"].as_array().unwrap() {
        let name = ts_cmd["name"].as_str().unwrap();
        let rs_cmd = command_by_name(&root, name);
        let ancestors: Vec<&str> = if name == root.name {
            vec![]
        } else {
            vec!["humanify"]
        };
        assert_eq!(
            rs_cmd.help_information(&ancestors),
            ts_cmd["help"].as_str().unwrap(),
            "help for {name}"
        );
    }
}

#[test]
fn version_is_package_json_version() {
    assert_eq!(
        program().version.as_deref(),
        surface()["version"].as_str(),
        "the -V output is package.json's version"
    );
}

fn outcome_json(o: &ParseOutcome) -> Value {
    match o {
        ParseOutcome::Action {
            command,
            args,
            opts,
        } => {
            let mut values = serde_json::Map::new();
            for (k, v, _) in opts.entries() {
                values.insert(k.clone(), v.clone());
            }
            serde_json::json!({
                "kind": "action",
                "command": command,
                "args": args,
                "opts": values,
            })
        }
        ParseOutcome::Exit {
            exit_code,
            code,
            stdout,
            stderr,
        } => serde_json::json!({
            "kind": "exit",
            "exitCode": exit_code,
            "code": code,
            "stdout": stdout,
            "stderr": stderr,
        }),
    }
}

#[test]
fn every_recorded_argv_parses_like_commander() {
    let s = surface();
    let root = program();
    let mut divergences: Vec<String> = Vec::new();
    let cases = s["cases"].as_array().unwrap();
    for case in cases {
        let argv: Vec<String> = case["argv"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a.as_str().unwrap().to_string())
            .collect();
        let ts = &case["outcome"];
        let rs = root.parse(&argv);
        let mut rs_json = outcome_json(&rs);
        let mut ts_cmp = ts.clone();
        // Sources are compared separately (below); drop from the value view.
        ts_cmp.as_object_mut().unwrap().remove("sources");
        if let Some(o) = rs_json.as_object_mut() {
            o.retain(|k, _| ts_cmp.get(k).is_some());
        }
        if rs_json != ts_cmp {
            divergences.push(format!("{argv:?}\n  ts   {ts_cmp}\n  rust {rs_json}"));
            continue;
        }
        if let (ParseOutcome::Action { command, opts, .. }, Some(sources)) =
            (&rs, ts["sources"].as_object())
        {
            let cmd = command_by_name(
                &root,
                if command == "pipeline" {
                    "humanify"
                } else {
                    command
                },
            );
            for o in cmd
                .options
                .iter()
                .filter(|o| !RUST_ONLY_OPTIONS.contains(&o.flags.as_str()))
            {
                let key = o.attribute_name();
                let rs_src = opts.source(&key).map(ValueSource::as_str);
                let ts_src = sources.get(&key).and_then(Value::as_str);
                if rs_src != ts_src {
                    divergences.push(format!(
                        "{argv:?} source of {key}: ts {ts_src:?} rust {rs_src:?}"
                    ));
                }
            }
        }
    }
    assert!(
        divergences.is_empty(),
        "{} of {} cases diverge:\n{}",
        divergences.len(),
        cases.len(),
        divergences.join("\n")
    );
    eprintln!("surface: {} argv cases identical", cases.len());
}
