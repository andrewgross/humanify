//! The lookup tables (`humanify-library-index/4`): what the encoder writes
//! the reader answers, from disk and from memory alike, without loading a
//! table whole.

use std::collections::BTreeMap;

use crate::libindex::format::{LiteralRow, UnitRow};
use crate::libindex::table::{
    BLOCK, KeyColumnBytes, LITERALS_FILE, LiteralTable, PACKAGES_FILE, Source, UnitTable,
    lookup_files, package_table, unit_key, units_file,
};

fn unit(hash: u64, owner: &str, module_bytes: u64) -> UnitRow {
    UnitRow {
        hash: format!("{hash:016x}"),
        owner: owner.into(),
        file: "index.js".into(),
        nodes: 40,
        content: 5,
        module_bytes,
        versions: vec![("1.0.0".into(), "1.0.0".into())],
        vendored: None,
    }
}

fn lits(owner: &str, file: &str, l: &[&str]) -> LiteralRow {
    LiteralRow {
        owner: owner.into(),
        file: file.into(),
        lits: l.iter().map(|s| s.to_string()).collect(),
        versions: vec![("1.0.0".into(), "1.0.0".into())],
    }
}

#[test]
fn unit_keys_are_the_sixteen_lowercase_hex_digits_and_nothing_else() {
    assert_eq!(unit_key("00000422066b168c"), Some(0x0000_0422_066b_168c));
    assert_eq!(unit_key("ffffffffffffffff"), Some(u64::MAX));
    // any other spelling can hold no indexed row: no key, never a near one
    for bad in [
        "",
        "abc",
        "00000422066B168C",
        "00000422066b168c0",
        "+0000422066b168c",
    ] {
        assert_eq!(unit_key(bad), None, "{bad}");
    }
}

#[test]
fn an_equal_range_is_exact_across_block_boundaries_and_duplicate_runs() {
    // duplicates straddle the fences on purpose: 3 x BLOCK keys, a run of
    // equal keys across the first fence and a run at the very end
    let mut keys: Vec<u64> = (0..(3 * BLOCK) as u64).map(|k| k * 10).collect();
    for k in &mut keys[BLOCK - 3..BLOCK + 4] {
        *k = 635;
    }
    let last = keys.len();
    for k in &mut keys[last - 5..] {
        *k = 9999;
    }
    keys.sort_unstable();
    let col = KeyColumnBytes::encode(&keys);
    let src = Source::Memory(col.bytes.clone());
    let reader = col.open(&src, 0).expect("opens");
    let want = |key: u64| {
        let lo = keys.partition_point(|&k| k < key) as u64;
        let hi = keys.partition_point(|&k| k <= key) as u64;
        lo..hi
    };
    for key in [
        0,
        5,
        10,
        620,
        630,
        635,
        640,
        1270,
        1280,
        9999,
        10_000,
        u64::MAX,
    ] {
        assert_eq!(reader.equal_range(&src, key), want(key), "key {key}");
    }
    for &key in &keys {
        assert_eq!(reader.equal_range(&src, key), want(key), "key {key}");
    }
}

#[test]
fn a_unit_table_answers_every_hash_with_its_rows_in_row_order() {
    // more keys than one block, a key held by two owners (two rows, row
    // order kept), and the owner ids are the package table's
    let mut rows: Vec<UnitRow> = (0..200u64).map(|h| unit(h * 7, "zod", h)).collect();
    rows.insert(11, unit(70, "ajv", 9));
    let packages = package_table(&[&rows], &[]);
    assert_eq!(packages, vec!["ajv".to_string(), "zod".to_string()]);
    let bytes = UnitTable::encode(&rows, &packages).expect("encodes");
    let table = UnitTable::open(Source::Memory(bytes)).expect("opens");
    let hits = table.hits(&format!("{:016x}", 70));
    let got: Vec<(u32, u64)> = hits.iter().map(|h| (h.owner, h.module_bytes)).collect();
    assert_eq!(got, vec![(1, 10), (0, 9)]);
    assert_eq!(table.hits(&format!("{:016x}", 7 * 199)).len(), 1);
    assert!(table.hits(&format!("{:016x}", 8)).is_empty());
    assert!(table.hits("not-a-hash").is_empty());
}

#[test]
fn a_unit_table_carries_each_row_s_vendored_mark() {
    // exp115 (/4): own code, a copy of an indexed package (its owner id),
    // a copy of a package the index does not hold
    use crate::libindex::table::{NOT_VENDORED, VENDORED_UNINDEXED};
    let mut own = unit(5, "crossws", 100);
    own.file = "dist/index.mjs".into();
    let mut copy = unit(5, "crossws", 200);
    copy.file = "dist/_chunks/libs/ws.mjs".into();
    copy.vendored = Some("ws".into());
    let mut unknown = unit(5, "@rspack/core", 300);
    unknown.file = "compiled/tinypool/index.js".into();
    unknown.vendored = Some("tinypool".into());
    let origin = unit(5, "ws", 50);
    let rows = vec![own, copy, unknown, origin];
    let packages = package_table(&[&rows], &[]);
    let ws = packages.iter().position(|p| p == "ws").unwrap() as u32;
    let table = UnitTable::open(Source::Memory(
        UnitTable::encode(&rows, &packages).expect("encodes"),
    ))
    .expect("opens");
    let got: Vec<(u64, u32, bool)> = table
        .hits(&format!("{:016x}", 5))
        .iter()
        .map(|h| (h.module_bytes, h.vendored, h.is_vendored()))
        .collect();
    assert_eq!(
        got,
        vec![
            (100, NOT_VENDORED, false),
            (200, ws, true),
            (300, VENDORED_UNINDEXED, true),
            (50, NOT_VENDORED, false),
        ]
    );
}

#[test]
fn a_unit_row_whose_hash_is_not_a_unit_key_is_refused_at_encoding() {
    let mut bad = unit(1, "zod", 1);
    bad.hash = "zz".into();
    let err = UnitTable::encode(&[bad], &["zod".to_string()]).unwrap_err();
    assert!(err.contains("zz"), "{err}");
}

#[test]
fn a_literal_table_resolves_strings_exactly_and_serves_postings_and_files() {
    let rows = vec![
        lits("ajv", "a.js", &["S:x", "P:shared", "N:42"]),
        lits("zod", "z.js", &["P:shared", "S:only-zod"]),
        lits("zod", "y.js", &["P:shared", "S:x"]),
    ];
    let packages = package_table(&[], &rows);
    let table =
        LiteralTable::open(Source::Memory(LiteralTable::encode(&rows, &packages))).expect("opens");
    assert_eq!(table.literal_packages(), 2);
    assert_eq!(table.resolve("S:absent"), None);
    let shared = table.resolve("P:shared").expect("indexed");
    assert_eq!(table.package_df(shared), 2);
    assert_eq!(table.postings(shared), vec![0, 1, 2]);
    let only = table.resolve("S:only-zod").expect("indexed");
    assert_eq!(table.package_df(only), 1);
    assert_eq!(table.postings(only), vec![1]);
    let x = table.resolve("S:x").expect("indexed");
    assert_eq!((table.postings(x), table.package_df(x)), (vec![0, 2], 2));
    assert_eq!(
        (
            table.file_owner(0),
            table.file_owner(1),
            table.file_owner(2)
        ),
        (0, 1, 1)
    );
    // a file's literal ids, sorted, each resolving back to its string
    let held = table.file_literals(2);
    assert!(held.windows(2).all(|w| w[0] < w[1]), "{held:?}");
    let mut want = vec![shared, x];
    want.sort_unstable();
    assert_eq!(held, want);
}

#[test]
fn literals_sharing_one_key_are_told_apart_by_their_strings() {
    // the reader confirms a key hit against the stored string, so a key
    // collision can never resolve one literal to another: here EVERY
    // literal has the same key
    let rows = vec![
        lits("p", "f.js", &["S:a", "S:b"]),
        lits("q", "g.js", &["S:b", "S:c"]),
    ];
    let packages = package_table(&[], &rows);
    let same = |_: &str| 7u64;
    let table = LiteralTable::open_keyed(
        Source::Memory(LiteralTable::encode_keyed(&rows, &packages, same)),
        same,
    )
    .expect("opens");
    let id = |l: &str| table.resolve(l).expect(l);
    let (a, b, c) = (id("S:a"), id("S:b"), id("S:c"));
    assert!(a != b && b != c && a != c);
    assert_eq!(table.resolve("S:d"), None);
    assert_eq!(
        (table.postings(a), table.postings(b), table.postings(c)),
        (vec![0], vec![0, 1], vec![1])
    );
}

#[test]
fn the_files_on_disk_read_the_same_as_memory_and_a_truncated_table_is_refused() {
    let dir = std::env::temp_dir().join(format!("humanify-table-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let units: BTreeMap<String, Vec<UnitRow>> =
        [("bun-bun-cjs-min".to_string(), vec![unit(5, "zod", 3)])]
            .into_iter()
            .collect();
    let literals = vec![lits("zod", "z.js", &["S:x"])];
    let files = lookup_files(&units, &literals).expect("encodes");
    let names: Vec<&str> = files.keys().map(String::as_str).collect();
    assert_eq!(
        names,
        vec![LITERALS_FILE, PACKAGES_FILE, &units_file("bun-bun-cjs-min")]
    );
    for (name, bytes) in &files {
        std::fs::write(dir.join(name), bytes).unwrap();
    }
    let path = dir.join(units_file("bun-bun-cjs-min"));
    let table = UnitTable::open(Source::open(&path).expect("opens")).expect("reads");
    assert_eq!(table.hits(&format!("{:016x}", 5)).len(), 1);
    let bytes = &files[&units_file("bun-bun-cjs-min")];
    std::fs::write(&path, &bytes[..bytes.len() - 1]).unwrap();
    let err = UnitTable::open(Source::open(&path).expect("opens"))
        .err()
        .expect("a short table is refused");
    assert!(err.contains("bytes"), "{err}");
    std::fs::remove_dir_all(&dir).ok();
}
