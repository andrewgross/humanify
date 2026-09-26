//! The run-artifact writers' red tests (the byte-gated formats live in
//! vectors_test.rs).

use humanify_core::rename::validated::ledger::{
    LedgerEdit, LedgerStage, Occurrence, RenameLedger, RenameLedgerBundle, RenameLedgerEntry,
    apply_rename_ledger, sha256_hex,
};

use crate::writers::{RENAME_LEDGER_APPLIER, write_rename_ledger};

fn temp_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("hf-ledger-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// `--rename-ledger <dir>`: `rename-ledger.json` (compact, no trailing
/// newline), the source snapshot, the applier.
#[test]
fn the_ledger_dir_holds_the_ledger_its_source_and_the_applier() {
    let dir = temp_dir("dir");
    let source = "var a = 1;\n";
    let bundle = RenameLedgerBundle {
        ledger: RenameLedger {
            version: 2,
            source_sha256: sha256_hex(source),
            entries: Vec::new(),
            edits: Vec::new(),
            post: None,
            output_sha256: None,
        },
        source: source.to_string(),
        stage_sources: Vec::new(),
    };
    write_rename_ledger(&dir.join("nested"), &bundle).expect("writes");
    let read = |f: &str| std::fs::read_to_string(dir.join("nested").join(f)).expect(f);
    assert_eq!(read("rename-ledger.json"), bundle.to_json());
    assert_eq!(read("source.js"), source);
    assert_eq!(read("apply.mjs"), RENAME_LEDGER_APPLIER);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Run the written `apply.mjs`; its output file's text.
fn run_applier(dir: &std::path::Path) -> Result<String, String> {
    let out = dir.join("out.js");
    let status = std::process::Command::new("node")
        .arg(dir.join("apply.mjs"))
        .arg(&out)
        .output()
        .map_err(|e| format!("node: {e}"))?;
    if !status.status.success() {
        return Err(String::from_utf8_lossy(&status.stderr).into_owned());
    }
    std::fs::read_to_string(&out).map_err(|e| e.to_string())
}

/// Findings #48/#49 for the standalone applier: it replays every recorded
/// form (a printed occurrence, a stage edit, a post stage, non-ASCII text
/// in JS string units) to exactly the Rust replay, checks the pinned output
/// hash, and a bundle-sized ledger (12 MB, 2M occurrences) finishes in
/// seconds — the old right-to-left `slice + name + slice` re-copied the
/// text per edit and never finished on a real bundle.
#[test]
fn the_applier_replays_exactly_and_in_linear_time() {
    // Exactness: `é` shifts JS units against bytes.
    let source = "var é = 1, a = 2;\nuse({ a });\nexport const b = a;\n";
    let a_decl = source.find("a =").expect("a") as u32;
    let a_short = source.find("{ a }").expect("{a}") as u32 + 2;
    let a_ref = source.rfind("= a;").expect("= a") as u32 + 2;
    let export_at = source.find("export ").expect("export") as u32;
    let stage0 = "var é = 1, count = 2;\nuse({ a: count });\nconst b = count;\n";
    let post_at = stage0.find("b =").expect("b") as u32;
    let output = "var é = 1, count = 2;\nuse({ a: count });\nconst total = count;\n";
    let bundle = RenameLedgerBundle {
        ledger: RenameLedger {
            version: 2,
            source_sha256: sha256_hex(source),
            entries: vec![RenameLedgerEntry {
                original_name: "a".into(),
                final_name: "count".into(),
                occurrences: vec![
                    [a_decl, a_decl + 1].into(),
                    Occurrence {
                        start: a_short,
                        end: a_short + 1,
                        text: Some("a: count".into()),
                    },
                    [a_ref, a_ref + 1].into(),
                ],
            }],
            edits: vec![LedgerEdit(export_at, export_at + 7, String::new())],
            post: Some(vec![LedgerStage {
                source_sha256: sha256_hex(stage0),
                entries: vec![RenameLedgerEntry {
                    original_name: "b".into(),
                    final_name: "total".into(),
                    occurrences: vec![[post_at, post_at + 1].into()],
                }],
                edits: Vec::new(),
            }]),
            output_sha256: Some(sha256_hex(output)),
        },
        source: source.to_string(),
        stage_sources: vec![stage0.to_string()],
    };
    assert_eq!(
        apply_rename_ledger(source, &bundle.ledger).as_deref(),
        Ok(output)
    );
    let dir = temp_dir("exact");
    write_rename_ledger(&dir, &bundle).expect("writes");
    assert_eq!(run_applier(&dir).as_deref(), Ok(output));
    // A wrong pinned output is refused, not written.
    let mut wrong = bundle.clone();
    wrong.ledger.output_sha256 = Some(sha256_hex("something else"));
    write_rename_ledger(&dir, &wrong).expect("writes");
    assert!(
        run_applier(&dir).is_err(),
        "a wrong outputSha256 must throw"
    );
    let _ = std::fs::remove_dir_all(&dir);

    // Linear time.
    let unit = "a = b + a;\n";
    let big = unit.repeat(12_000_000 / unit.len());
    let n = (big.len() / unit.len()) as u32;
    let bundle = RenameLedgerBundle {
        ledger: RenameLedger {
            version: 2,
            source_sha256: sha256_hex(&big),
            entries: vec![RenameLedgerEntry {
                original_name: "a".into(),
                final_name: "alpha".into(),
                occurrences: (0..n)
                    .map(|i| i * unit.len() as u32)
                    .flat_map(|s| [[s, s + 1], [s + 8, s + 9]])
                    .map(Into::into)
                    .collect(),
            }],
            edits: Vec::new(),
            post: None,
            output_sha256: None,
        },
        source: big,
        stage_sources: Vec::new(),
    };
    let dir = temp_dir("big");
    write_rename_ledger(&dir, &bundle).expect("writes");
    let t = std::time::Instant::now();
    let out = run_applier(&dir).expect("the applier runs");
    let took = t.elapsed();
    assert_eq!(out.len(), n as usize * "alpha = b + alpha;\n".len());
    assert!(took.as_secs_f64() < 30.0, "apply.mjs took {took:?}");
    let _ = std::fs::remove_dir_all(&dir);
}
