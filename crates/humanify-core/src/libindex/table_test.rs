//! The lookup tables (`humanify-library-index/3`): what the encoder writes
//! the reader answers, from disk and from memory alike, without loading a
//! table whole.

use std::collections::BTreeMap;

use crate::libindex::format::{LiteralRow, NameRow, UnitRow};
use crate::libindex::names::{NameKind, RestoredName};
use crate::libindex::table::{
    BLOCK, KeyColumnBytes, LITERALS_FILE, LiteralTable, NAMES_STRINGS_FILE, NameTable,
    PACKAGES_FILE, Source, UnitTable, lookup_files, names_file, names_lookup_files, package_table,
    unit_key, units_file,
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
    }
}

fn name(hash: &str, owner: &str, names: &[(&str, &str, &str)], v: &str) -> NameRow {
    NameRow {
        hash: hash.into(),
        owner: owner.into(),
        file: "index.js".into(),
        names: names
            .iter()
            .map(|&(slot, name, kind)| RestoredName {
                slot: slot.parse().unwrap(),
                name: name.into(),
                kind: serde_json::from_str::<NameKind>(&format!("\"{kind}\"")).unwrap(),
            })
            .collect(),
        versions: vec![(v.into(), v.into())],
    }
}

#[test]
fn the_names_table_answers_each_unit_row_with_its_lists() {
    let units = BTreeMap::from([(
        "bun".to_string(),
        vec![unit(0x11, "dep", 100), unit(0x22, "dep", 100)],
    )]);
    let names = BTreeMap::from([(
        "bun".to_string(),
        vec![
            name(
                &format!("{:016x}", 0x11),
                "dep",
                &[("0", "pLimit", "function")],
                "1.0.0",
            ),
            // the same print under two versions whose sources disagree:
            // both lists are kept, the lookup answers Ambiguous
            name(
                &format!("{:016x}", 0x22),
                "dep",
                &[("0", "a", "local")],
                "1.0.0",
            ),
            name(
                &format!("{:016x}", 0x22),
                "dep",
                &[("0", "b", "param")],
                "2.0.0",
            ),
        ],
    )]);
    let files = names_lookup_files(&units, &names).expect("encode");
    assert_eq!(
        files.keys().cloned().collect::<Vec<_>>(),
        vec![names_file("bun").as_str(), NAMES_STRINGS_FILE],
        "{:?}",
        files.keys()
    );
    let strings: Vec<String> =
        serde_json::from_slice(&files[NAMES_STRINGS_FILE]).expect("names.json");
    assert_eq!(strings, vec!["a", "b", "pLimit"]);
    let table = NameTable::open(Source::Memory(files[names_file("bun").as_str()].clone()))
        .expect("the encoded table opens");
    let single = table.lists(0, &strings).expect("decode");
    assert_eq!(single.len(), 1);
    assert_eq!(single[0][0].slot, 0);
    assert_eq!(single[0][0].name, "pLimit");
    assert_eq!(single[0][0].kind, NameKind::Function);
    let both = table.lists(1, &strings).expect("decode");
    assert_eq!(both.len(), 2);
    assert_eq!(both[0][0].name, "a");
    assert_eq!(both[1][0].name, "b");
    // a row with no names answers nothing (not an error, not a near row)
    assert!(table.lists(2, &strings).expect("decode").is_empty());
}

#[test]
fn a_names_row_whose_unit_key_is_absent_is_refused_at_encoding() {
    let units = BTreeMap::from([("bun".to_string(), vec![unit(0x11, "dep", 100)])]);
    let names = BTreeMap::from([(
        "bun".to_string(),
        vec![name(
            &format!("{:016x}", 0x99),
            "dep",
            &[("0", "x", "local")],
            "1.0.0",
        )],
    )]);
    let err = names_lookup_files(&units, &names).expect_err("orphan names row");
    assert!(err.contains("no unit row"), "{err}");
}

#[test]
fn a_names_table_refuses_a_kind_ordinal_it_does_not_know() {
    // corruption must be loud, never a silently wrong kind
    let units = BTreeMap::from([("bun".to_string(), vec![unit(0x11, "dep", 100)])]);
    let names = BTreeMap::from([(
        "bun".to_string(),
        vec![name(
            &format!("{:016x}", 0x11),
            "dep",
            &[("0", "x", "local")],
            "1.0.0",
        )],
    )]);
    let files = names_lookup_files(&units, &names).expect("encode");
    let mut bytes = files[names_file("bun").as_str()].clone();
    let last = bytes.len() - 4; // the last entry's kind ordinal
    bytes[last] = u32::MAX.to_le_bytes()[0];
    let table = NameTable::open(Source::Memory(bytes)).expect("the table opens");
    let strings = ["x".to_string()];
    assert!(
        table.lists(0, &strings).is_err(),
        "an unknown kind is refused"
    );
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
    let packages = package_table(&[&rows], &[], &[]);
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
    let packages = package_table(&[], &[], &rows);
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
    let packages = package_table(&[], &[], &rows);
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
    let files = lookup_files(&units, None, &literals).expect("encodes");
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

#[test]
fn the_edge_table_answers_each_unit_row_and_stores_a_shared_list_once() {
    use crate::libindex::format::{Callee, EdgeRow};
    use crate::libindex::table::{EdgeCallee, EdgeTable, unit_row_ids};
    let file = |r: UnitRow, f: &str| UnitRow {
        file: f.into(),
        ..r
    };
    // rows out of key order; key 7 is held by two files of one owner
    let units = vec![
        file(unit(9, "zod", 10), "b.js"),
        file(unit(7, "zod", 10), "a.js"),
        file(unit(7, "zod", 10), "c.js"),
        unit(3, "yup", 10),
    ];
    let edge = |hash: u64, f: &str, inside: &[u64], out_of: &[(&str, u64)]| EdgeRow {
        hash: format!("{hash:016x}"),
        owner: "zod".into(),
        file: f.into(),
        inside: inside.iter().map(|h| format!("{h:016x}")).collect(),
        out_of: out_of
            .iter()
            .map(|(o, h)| Callee {
                owner: (*o).into(),
                hash: format!("{h:016x}"),
            })
            .collect(),
    };
    let edges = vec![
        edge(7, "a.js", &[1, 2], &[]),
        edge(7, "c.js", &[5], &[("yup", 3)]),
        edge(9, "b.js", &[1, 2], &[]),
    ];
    let packages = package_table(&[&units], &[&edges], &[]);
    let bytes = EdgeTable::encode(&units, &edges, &packages).expect("encodes");
    let table = EdgeTable::open(Source::Memory(bytes.clone())).expect("opens");
    let ids = unit_row_ids(&units).expect("ids");
    let zod = |key| EdgeCallee { owner: 1, key };
    assert_eq!(table.callees(ids[1]), vec![zod(1), zod(2)]);
    assert_eq!(
        table.callees(ids[2]),
        vec![EdgeCallee { owner: 0, key: 3 }, zod(5)]
    );
    assert_eq!(table.callees(ids[0]), vec![zod(1), zod(2)]);
    assert_eq!(table.callees(ids[3]), vec![], "a row with no edge row");
    // the hits name the same rows the edge table is keyed by
    let unit_table = UnitTable::open(Source::Memory(
        UnitTable::encode(&units, &packages).unwrap(),
    ))
    .unwrap();
    let rows: Vec<u32> = unit_table
        .hits(&format!("{:016x}", 7))
        .iter()
        .map(|h| h.row)
        .collect();
    assert_eq!(rows, vec![ids[1], ids[2]]);
    // three keyed rows, two lists: header, keys + fence, list ids, offsets, callees
    let want = 8 + 3 * 8 + (1 + 3) * 8 + 3 * 4 + (2 + 1) * 8 + (2 + 2) * 12;
    assert_eq!(bytes.len(), want, "the shared list is stored once");
    let unknown = vec![edge(4, "a.js", &[1], &[])];
    assert!(EdgeTable::encode(&units, &unknown, &packages).is_err());
}

#[test]
fn a_callee_print_exists_in_the_edge_data_without_owning_a_unit_row() {
    // exp155: the flat callee-print table indexes every callee print any
    // row records — keyed, with its recorders — so one under the row floor
    // (or dropped with its row) is queryable. INSIDE entries credit the
    // row's owner; OUT-OF entries the callee's own recorded owner.
    use crate::libindex::format::{Callee, EdgeRow};
    use crate::libindex::table::CalleePrintTable;
    let file = |r: UnitRow, f: &str| UnitRow {
        file: f.into(),
        ..r
    };
    let units = vec![
        file(unit(7, "zod", 10), "a.js"),
        file(unit(7, "zod", 10), "b.js"),
        unit(3, "yup", 10),
    ];
    let edge = |hash: u64, f: &str, inside: &[u64], out_of: &[(&str, u64)]| EdgeRow {
        hash: format!("{hash:016x}"),
        owner: "zod".into(),
        file: f.into(),
        inside: inside.iter().map(|h| format!("{h:016x}")).collect(),
        out_of: out_of
            .iter()
            .map(|(o, h)| Callee {
                owner: (*o).into(),
                hash: format!("{h:016x}"),
            })
            .collect(),
    };
    // 11 and 12 are rowless (no unit row holds them); 3 has a row under
    // yup but is recorded here by zod as OUT-OF
    let edges = vec![
        edge(7, "a.js", &[11, 12], &[]),
        edge(7, "b.js", &[12], &[("yup", 3)]),
    ];
    let packages = package_table(&[&units], &[&edges], &[]);
    let bytes = CalleePrintTable::encode(&edges, &packages).expect("encodes");
    let table = CalleePrintTable::open(Source::Memory(bytes)).expect("opens");
    let key = |k: u64| k;
    // zod (id 1) is the row owner of both rows; yup (id 0) records 3
    assert_eq!(table.recorders(key(11)), vec![1], "a rowless INSIDE callee");
    assert_eq!(table.recorders(key(12)), vec![1]);
    assert_eq!(
        table.recorders(key(3)),
        vec![0],
        "an OUT-OF entry credits the callee's own owner"
    );
    assert!(table.recorders(key(99)).is_empty(), "an unrecorded print");
    let mut files = crate::libindex::table::lookup_files(
        &std::iter::once(("bun-node-cjs-min".to_string(), units)).collect(),
        Some(&std::iter::once(("bun-node-cjs-min".to_string(), edges)).collect()),
        &[],
    )
    .expect("lookup files");
    assert!(
        files
            .remove("callee-prints.bun-node-cjs-min.bin")
            .is_some_and(|b| CalleePrintTable::open(Source::Memory(b)).is_ok()),
        "lookup files write one callee-print table per edge slice"
    );
}
