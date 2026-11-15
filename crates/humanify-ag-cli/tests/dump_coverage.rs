//! `--dump-artifacts` coverage: a prior-version run of a committed e2e
//! fixture with the dump on writes every kept section, non-empty, writes
//! none of the dropped ones, and leaves the output tree byte-identical to
//! the same run without the dump (the dump only observes).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use humanify_ag_testkit::ScratchDir;
use humanify_ag_testkit::answers::renamed;
use humanify_ag_testkit::stub_server::StubServer;

/// The fixture: a minified Bun CJS app with a vendored library, two
/// releases (so the prior-version stages run) and a split tree.
const FIXTURE: &str = "../../test/e2e/fixtures/bun-cjs-app/build";

/// Every section a prior-version split run writes.
const KEPT: [&str; 12] = [
    "functions.json",
    "modules.json",
    "matches.json",
    "matches-close.json",
    "transfers.json",
    "transfers-mechanical.json",
    "votes.json",
    "names.json",
    "placement.json",
    "emit.json",
    "partitions.json",
    "prompts.jsonl",
];

/// What the dump no longer writes.
const DROPPED: [&str; 4] = [
    "meta.json",
    "tree-manifest.json",
    "cache-keys.jsonl",
    "text",
];

fn bundle(version: &str) -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(FIXTURE)
        .join(version)
        .join("build/index.js");
    p.display().to_string()
}

/// One run of `input` into `out`; `settings` are extra `--config` lines.
fn run(dir: &Path, endpoint: &str, input: &str, out: &Path, extra: &[String]) {
    let config = out.with_extension("toml");
    let toml: String = [
        "llm.retries = 0".to_string(),
        "library.matching = \"off\"".to_string(),
        "split.enabled = true".to_string(),
    ]
    .into_iter()
    .chain(extra.iter().cloned())
    .map(|l| format!("{l}\n"))
    .collect();
    std::fs::write(&config, toml).unwrap();
    let o = Command::new(env!("CARGO_BIN_EXE_humanify-ag"))
        .current_dir(dir)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HUMANIFY_API_KEY", "k")
        .args([input, "--model", "stub-model", "--endpoint", endpoint, "-o"])
        .arg(out)
        .arg("--config")
        .arg(&config)
        .output()
        .unwrap();
    assert_eq!(
        o.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
}

/// Every file under `root`, by relative path.
fn tree(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                let rel = path.strip_prefix(root).unwrap().to_path_buf();
                out.insert(rel, std::fs::read(&path).unwrap());
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out);
    out
}

/// A section holds records: a non-empty array, an object with a member
/// that holds records, or any scalar but null.
fn has_records(v: &serde_json::Value) -> bool {
    match v {
        serde_json::Value::Null => false,
        serde_json::Value::Array(items) => !items.is_empty(),
        serde_json::Value::Object(members) => members.values().any(has_records),
        _ => true,
    }
}

#[test]
fn the_dump_writes_every_kept_section_and_never_changes_the_tree() {
    let s = ScratchDir::new("dump-coverage");
    let url = StubServer::answering(renamed).base_url;
    let prior = s.path().join("v1");
    run(s.path(), &url, &bundle("v1.0.0"), &prior, &[]);
    let with_prior = |dump: Option<&Path>| -> Vec<String> {
        let mut extra = vec![format!(
            "matching.prior_version = {:?}",
            prior.display().to_string()
        )];
        if let Some(d) = dump {
            extra.push(format!(
                "diagnostics.dump_artifacts = {:?}",
                d.display().to_string()
            ));
        }
        extra
    };
    let plain = s.path().join("plain");
    run(s.path(), &url, &bundle("v1.1.0"), &plain, &with_prior(None));
    let dump = s.path().join("dump");
    let dumped = s.path().join("dumped");
    run(
        s.path(),
        &url,
        &bundle("v1.1.0"),
        &dumped,
        &with_prior(Some(&dump)),
    );

    for name in KEPT {
        let path = dump.join(name);
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{name}: {e}"));
        let records: Vec<serde_json::Value> = if name.ends_with(".jsonl") {
            text.lines()
                .filter(|l| !l.trim().is_empty())
                .map(|l| serde_json::from_str(l).expect("a JSONL row"))
                .collect()
        } else {
            vec![serde_json::from_str(&text).expect("a JSON section")]
        };
        assert!(records.iter().any(has_records), "{name} holds no records");
    }
    for name in DROPPED {
        assert!(!dump.join(name).exists(), "{name} is no longer written");
    }
    let a = tree(&plain);
    let b = tree(&dumped);
    assert_eq!(
        a.keys().collect::<Vec<_>>(),
        b.keys().collect::<Vec<_>>(),
        "the same files"
    );
    for (path, bytes) in &a {
        assert!(
            b[path] == *bytes,
            "{} differs with the dump on",
            path.display()
        );
    }
}
