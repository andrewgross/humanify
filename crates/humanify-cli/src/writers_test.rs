//! The run-artifact writers' red tests (the byte-gated formats live in
//! vectors_test.rs).

use crate::writers::{RENAME_LEDGER_APPLIER, write_rename_ledger};

/// `writeRenameLedger(dir, bundle)`: `rename-ledger.json` (compact, no
/// trailing newline), the source snapshot, the applier.
#[test]
fn the_ledger_dir_holds_the_ledger_its_source_and_the_applier() {
    use humanify_core::rename::validated::ledger::{RenameLedger, RenameLedgerBundle, sha256_hex};
    let dir = std::env::temp_dir().join(format!("hf-ledger-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let source = "var a = 1;\n";
    let bundle = RenameLedgerBundle {
        ledger: RenameLedger {
            version: 1,
            source_sha256: sha256_hex(source),
            entries: Vec::new(),
            post: None,
        },
        source: source.to_string(),
        stage_sources: Vec::new(),
    };
    write_rename_ledger(&dir.join("nested"), &bundle).expect("writes");
    let read = |f: &str| std::fs::read_to_string(dir.join("nested").join(f)).expect(f);
    assert_eq!(read("rename-ledger.json"), bundle.to_ts_json());
    assert_eq!(read("source.js"), source);
    assert_eq!(read("apply.mjs"), RENAME_LEDGER_APPLIER);
    let _ = std::fs::remove_dir_all(&dir);
}
