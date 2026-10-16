//! Sourcemap decoding and module attribution: which npm file (and so
//! which package) a bundled module's span came from.

use super::sourcemap::{SourceMap, decode_mappings, owner_of_source, utf16_to_byte_offsets};

#[test]
fn vlq_mappings_decode_to_generated_and_source_positions() {
    // "AAAA,EAAC;AACA": line 0 col 0 → src 0 line 0; col 2 → src 0
    // line 0; line 1 col 0 → src 0, ORIGINAL line 1 (a source line is
    // relative across the whole mapping, like the source index).
    let segs = decode_mappings("AAAA,EAAC;AACA").expect("vlq");
    assert_eq!(segs, vec![(0, 0, 0, 0), (0, 2, 0, 0), (1, 0, 0, 1)]);
    // "AAAA,CCAA": second segment moves to source 1.
    let segs = decode_mappings("AAAA,CCAA").expect("vlq");
    assert_eq!(segs, vec![(0, 0, 0, 0), (0, 1, 1, 0)]);
    // "AAEA;AAAA": original line 2, and the next line stays on it.
    let segs = decode_mappings("AAEA;AAAA").expect("vlq");
    assert_eq!(segs, vec![(0, 0, 0, 2), (1, 0, 0, 2)]);
    assert!(decode_mappings("A!").is_err());
}

#[test]
fn utf16_columns_become_byte_offsets() {
    // "é" is 2 bytes and 1 UTF-16 unit; "😀" is 4 bytes and 2 units.
    let text = "aé😀b\nxy";
    let got = utf16_to_byte_offsets(text, &[(0, 0), (0, 1), (0, 2), (0, 4), (1, 1)]);
    assert_eq!(got, vec![Some(0), Some(1), Some(3), Some(7), Some(10)]);
    assert_eq!(utf16_to_byte_offsets(text, &[(5, 0)]), vec![None]);
}

#[test]
fn byte_offsets_are_linear_in_a_minified_one_line_bundle() {
    // A minified bundle is ONE line with a mapping segment every few
    // characters. Scanning from the line start for every segment is
    // quadratic: a 24 MB app bundle never finished (exp095 selection lab).
    let line: String = "aé".repeat(200_000); // 400k UTF-16 units, 600k bytes
    let text = format!("{line}\nxy");
    // out of order, every 4th unit, plus a past-the-end column and line 1
    let mut positions: Vec<(u32, u32)> = (0..100_000u32).rev().map(|i| (0, i * 4)).collect();
    positions.push((0, 400_001));
    positions.push((1, 1));
    let started = std::time::Instant::now();
    let got = utf16_to_byte_offsets(&text, &positions);
    assert!(
        started.elapsed() < std::time::Duration::from_secs(2),
        "took {:?}",
        started.elapsed()
    );
    // unit 4k is byte 6k ("aé" = 2 units, 3 bytes)
    assert_eq!(got[0], Some((99_999 * 4 / 2) * 3));
    assert_eq!(got[99_999], Some(0));
    assert_eq!(got[100_000], None);
    assert_eq!(got[100_001], Some(line.len() + 2));
}

#[test]
fn a_span_belongs_to_the_source_owning_nearly_all_of_its_segments() {
    let map = SourceMap {
        sources: vec!["a.js".into(), "b.js".into()],
        // one line; columns 0..9 from a.js, 10..11 from b.js
        segments: (0..10)
            .map(|c| (0, c, 0, 0))
            .chain([(0, 10, 1, 0), (0, 11, 1, 0)])
            .collect(),
    };
    let code = "x".repeat(20);
    let pure = map.attribute(&code, &[(0, 10), (10, 12), (0, 12)], 0.9);
    assert_eq!(pure[0], Some(0));
    assert_eq!(pure[1], Some(1));
    // 10 of 12 = 83% < 90%: a mixed span is nobody's
    assert_eq!(pure[2], None);
}

#[test]
fn the_owner_is_the_nearest_package_json_under_the_last_node_modules() {
    // node_modules is gitignored, so the install tree is made here.
    let root = std::env::temp_dir().join(format!("humanify-index-owner-{}", std::process::id()));
    let write = |rel: &str, text: &str| {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().expect("dir")).expect("mkdir");
        std::fs::write(p, text).expect("write");
    };
    write(
        "node_modules/dep/package.json",
        r#"{"name":"dep","version":"1.0.0"}"#,
    );
    write("node_modules/dep/index.js", "module.exports=1;");
    write(
        "node_modules/dep/node_modules/@sc/inner/package.json",
        r#"{"name":"@sc/inner","version":"0.2.0"}"#,
    );
    write(
        "node_modules/dep/node_modules/@sc/inner/lib/x.js",
        "module.exports=2;",
    );
    let out = root.join("out");
    let owner = owner_of_source(&out, "../node_modules/dep/node_modules/@sc/inner/lib/x.js")
        .expect("owned");
    assert_eq!(
        (
            owner.package.as_str(),
            owner.version.as_str(),
            owner.file.as_str()
        ),
        ("@sc/inner", "0.2.0", "lib/x.js")
    );
    let owner = owner_of_source(&out, "../node_modules/dep/index.js").expect("owned");
    assert_eq!(
        (owner.package.as_str(), owner.version.as_str()),
        ("dep", "1.0.0")
    );
    // the app entry is not library code
    assert!(owner_of_source(&out, "../entries/req.cjs").is_none());
    std::fs::remove_dir_all(&root).expect("cleanup");
}

#[test]
fn a_build_moved_to_another_work_dir_still_finds_its_sources() {
    // exp124: a finished build is reused from another work dir (seed-work:
    // hard-linked or copied). A bundler that wrote its sources relative to
    // the REAL path of the install (`../../../../../../selABC/work/x@1/
    // node_modules/...`) points nowhere from the new place, so every unit
    // was unowned. The install dir's name re-anchors it.
    use super::sourcemap::lexical;
    let root = std::env::temp_dir().join(format!("humanify-index-moved-{}", std::process::id()));
    let install = root.join("new/work/x@1");
    std::fs::create_dir_all(install.join("node_modules/dep")).expect("mkdir");
    std::fs::write(
        install.join("node_modules/dep/package.json"),
        r#"{"name":"dep","version":"2.0.0"}"#,
    )
    .expect("write");
    std::fs::write(install.join("node_modules/dep/lib.js"), "x").expect("write");
    let out = install.join("build/bun-node-cjs-min/req");
    std::fs::create_dir_all(&out).expect("mkdir");
    let mut map = SourceMap {
        sources: vec![
            "../../../../../../old/work/x@1/node_modules/dep/lib.js".into(),
            "../../../node_modules/dep/other.js".into(),
            "../../../../../../old/work/y@1/node_modules/dep/lib.js".into(),
        ],
        segments: Vec::new(),
    };
    assert!(owner_of_source(&out, &map.sources[0]).is_none(), "moved");
    map.relocate(&out);
    let owner = owner_of_source(&out, &map.sources[0]).expect("re-anchored");
    assert_eq!(
        (owner.package.as_str(), owner.file.as_str()),
        ("dep", "lib.js")
    );
    assert_eq!(
        lexical(&out.join(&map.sources[1])),
        install.join("node_modules/dep/other.js"),
        "a portable source is left alone"
    );
    assert_eq!(
        map.sources[2], "../../../../../../old/work/y@1/node_modules/dep/lib.js",
        "another install's path is not this one's"
    );
    std::fs::remove_dir_all(&root).ok();
}
