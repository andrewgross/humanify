//! The unit lookup: the structural rule, specificity, support and literal
//! agreement, on a tiny in-memory index.

use oxc_span::Span;

use crate::libindex::format::{LiteralRow, UnitRow};
use crate::libindex::lookup::{LibraryIndex, LookupRules, UnitQuery, claim_units};
use crate::libindex::units::{StatementPrints, UnitPrint};

/// A test name's unit hash (16 hex digits, the only spelling an index
/// holds).
fn hx(name: &str) -> String {
    format!("{:016x}", crate::libindex::table::literal_key(name))
}

fn row(hash: &str, owner: &str, nodes: u32, module_bytes: u64) -> UnitRow {
    UnitRow {
        hash: hx(hash),
        owner: owner.into(),
        file: "index.js".into(),
        nodes,
        content: 5,
        module_bytes,
        versions: vec![("1.0.0".into(), "1.0.0".into())],
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

fn print(hash: &str, nodes: u32, at: u32) -> UnitPrint {
    UnitPrint {
        span: Span::new(at, at + nodes),
        hash: hx(hash),
        nodes,
        content: 5,
    }
}

fn whole(hash: &str, nodes: u32) -> StatementPrints {
    StatementPrints {
        whole: print(hash, nodes, 0),
        functions: Vec::new(),
    }
}

#[test]
fn restore_names_answers_the_unit_row_the_hit_names() {
    use crate::libindex::format::NameRow;
    use crate::libindex::names::{NameKind, RestoredName};
    use crate::libindex::table::RestoreAnswer;
    let n = |slot: u32, name: &str| RestoredName {
        slot,
        name: name.into(),
        kind: NameKind::Local,
    };
    let names = vec![
        NameRow {
            hash: hx("a"),
            owner: "dep".into(),
            file: "index.js".into(),
            names: vec![n(0, "pLimit"), n(1, "concurrency")],
            versions: vec![("1.0.0".into(), "1.0.0".into())],
        },
        // the same triple with a DIFFERENT list: ambiguous
        NameRow {
            hash: hx("b"),
            owner: "dep".into(),
            file: "index.js".into(),
            names: vec![n(0, "one")],
            versions: vec![("1.0.0".into(), "1.0.0".into())],
        },
        NameRow {
            hash: hx("b"),
            owner: "dep".into(),
            file: "index.js".into(),
            names: vec![n(0, "two")],
            versions: vec![("2.0.0".into(), "2.0.0".into())],
        },
    ];
    let index = LibraryIndex::from_rows_with_names(
        vec![
            row("a", "dep", 40, 100),
            row("b", "dep", 40, 100),
            row("c", "dep", 40, 100),
        ],
        names,
        Vec::new(),
    );
    // one hit per (hash, owner), each its row
    let hits = |hash: &str| index.unit_hits(&hx(hash));
    assert_eq!(hits("a").len(), 1);
    match index.restore_names(hits("a")[0]) {
        RestoreAnswer::Single(names) => {
            let spelled: Vec<&str> = names.iter().map(|n| n.name.as_str()).collect();
            assert_eq!(spelled, vec!["pLimit", "concurrency"]);
        }
        other => panic!("{other:?}"),
    }
    match index.restore_names(hits("b")[0]) {
        RestoreAnswer::Ambiguous(lists) => assert_eq!(lists.len(), 2, "{lists:?}"),
        other => panic!("{other:?}"),
    }
    // a row the names table holds nothing of: absence, not an error
    assert_eq!(index.restore_names(hits("c")[0]), RestoreAnswer::Absent);
}

#[test]
fn an_index_without_names_tables_answers_restore_with_absence() {
    use crate::libindex::table::RestoreAnswer;
    let index = LibraryIndex::from_rows(vec![row("a", "dep", 40, 100)], Vec::new());
    let hit = index.unit_hits(&hx("a"))[0];
    assert_eq!(index.restore_names(hit), RestoreAnswer::Absent);
}

fn structural() -> LookupRules {
    LookupRules {
        agreement: false,
        ..LookupRules::default()
    }
}

fn family(p: &str) -> String {
    p.split('/').next().unwrap_or(p).to_string()
}

fn same(a: &str, b: &str) -> bool {
    family(a) == family(b)
}

fn claims(
    index: &LibraryIndex,
    prints: &[StatementPrints],
    rules: LookupRules,
) -> Vec<(u32, String)> {
    let queries: Vec<UnitQuery<'_>> = prints
        .iter()
        .enumerate()
        .map(|(i, p)| UnitQuery {
            unit: i as u32,
            prints: p,
            module_bytes: 1000,
            text: "",
            callees: &[],
        })
        .collect();
    claim_units(index, &queries, rules, &same)
        .into_iter()
        .map(|c| (c.unit, c.package))
        .collect()
}

#[test]
fn a_strong_whole_statement_hit_claims_and_a_weak_one_needs_a_neighbour() {
    let index = LibraryIndex::from_rows(
        vec![
            row("a", "zod", 150, 2000),
            row("b", "zod", 40, 2000),
            row("c", "ms", 40, 2000),
        ],
        vec![],
    );
    let q = [
        whole("a", 150),
        whole("b", 40),
        whole("x", 40),
        whole("c", 40),
    ];
    assert_eq!(
        claims(&index, &q, structural()),
        vec![(0, "zod".into()), (1, "zod".into())]
    );
}

#[test]
fn specificity_and_ambiguity_refuse_a_hit() {
    let index = LibraryIndex::from_rows(
        vec![
            row("big", "vite", 150, 900_000),
            row("many", "a", 150, 100),
            row("many", "b", 150, 100),
            row("many", "c", 150, 100),
            row("many", "d", 150, 100),
        ],
        vec![],
    );
    let q = [whole("big", 150), whole("many", 150)];
    assert!(claims(&index, &q, structural()).is_empty());
    let loose = LookupRules {
        specificity: None,
        ..structural()
    };
    assert_eq!(claims(&index, &q, loose), vec![(0, "vite".into())]);
}

#[test]
fn functions_must_cover_half_the_statement() {
    let index = LibraryIndex::from_rows(vec![row("f", "zod", 120, 2000)], vec![]);
    let mut covered = whole("none", 200);
    covered.functions = vec![print("f", 120, 10)];
    let mut thin = whole("none", 300);
    thin.functions = vec![print("f", 120, 10)];
    assert_eq!(
        claims(&index, &[covered, thin], structural()),
        vec![(0, "zod".into())]
    );
}

#[test]
fn agreement_needs_the_literal_rule_to_name_the_same_family() {
    // 12 non-identifier strings (2 bits is the cap only for identifier-like
    // literals); 16 packages, so a literal one package holds weighs 4 bits.
    let strings: Vec<String> = (0..12).map(|i| format!("S:x-amz header {i}")).collect();
    let mut literal_rows = vec![lits(
        "@smithy/core",
        "index.js",
        &strings.iter().map(String::as_str).collect::<Vec<_>>(),
    )];
    for i in 0..15 {
        literal_rows.push(lits(
            &format!("pkg{i}"),
            "index.js",
            &[&format!("S:unrelated {i}")],
        ));
    }
    let index = LibraryIndex::from_rows(
        vec![
            row("a", "@aws-sdk/core", 150, 2000),
            row("b", "zod", 150, 2000),
        ],
        literal_rows,
    );
    let array: Vec<String> = (0..12).map(|i| format!("\"x-amz header {i}\"")).collect();
    let text = format!("var s=[{}];", array.join(","));
    let p = [whole("a", 150), whole("b", 150)];
    let queries = [
        UnitQuery {
            unit: 0,
            prints: &p[0],
            module_bytes: 1000,
            text: &text,
            callees: &[],
        },
        UnitQuery {
            unit: 1,
            prints: &p[1],
            module_bytes: 1000,
            text: &text,
            callees: &[],
        },
    ];
    let family = |p: &str| {
        if p.starts_with("@aws-sdk/") || p.starts_with("@smithy/") {
            "aws".to_string()
        } else {
            p.to_string()
        }
    };
    let same = |a: &str, b: &str| family(a) == family(b);
    let got = claim_units(&index, &queries, LookupRules::default(), &same);
    assert_eq!(got.len(), 1, "zod's structural claim is refused: {got:?}");
    assert_eq!(
        (got[0].unit, got[0].package.as_str()),
        (0, "@smithy/core"),
        "agreed, named by the literal rule's more specific package"
    );
    let (lp, score) = got[0].literal.clone().expect("consulted");
    assert_eq!((lp.as_str(), score), ("@smithy/core", 48.0));
}

#[test]
fn the_loader_reads_the_recipes_slice_and_refuses_another_fingerprint() {
    use crate::libindex::format::{
        INDEX_FORMAT, Manifest, RECIPE_VERSION, SliceRecord, fingerprint_versions,
    };
    use crate::libindex::lookup::LoadError;
    use crate::libindex::recipe::detect_recipe;
    use humanify_model::detection::{BundlerType, MinifierType};
    let dir = std::env::temp_dir().join(format!("humanify-lookup-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let write_manifest = |units: &str| {
        let mut f = fingerprint_versions();
        f.units = units.into();
        let m = Manifest {
            format: INDEX_FORMAT.into(),
            as_of: "2026-10-12".into(),
            fingerprints: f,
            registry: Default::default(),
            slices: vec![SliceRecord {
                slice: "bun-bun-cjs-min".into(),
                recipe: "bun:target=bun,format=cjs,minify".into(),
                recipe_version: RECIPE_VERSION.into(),
                default_for_bundler: true,
                bundler: "bun".into(),
                version: "1.4.0".into(),
                published: None,
                newest_at_as_of: None,
                bundler_after_as_of: false,
            }],
            packages: vec![],
            refused: vec![],
            shards: Default::default(),
            tables: Default::default(),
            data: Default::default(),
        };
        std::fs::write(
            dir.join("manifest.json"),
            serde_json::to_string(&m).unwrap(),
        )
        .unwrap();
    };
    let units = [(
        "bun-bun-cjs-min".to_string(),
        vec![row("a", "zod", 150, 10)],
    )]
    .into_iter()
    .collect();
    for (name, bytes) in
        crate::libindex::table::lookup_files(&units, Some(&Default::default()), &[]).unwrap()
    {
        std::fs::write(dir.join(name), bytes).unwrap();
    }
    let target_bun = detect_recipe(
        "// @bun @bun-cjs\nvar a;",
        BundlerType::Bun,
        MinifierType::Bun,
    );
    let target_node = detect_recipe("var a;", BundlerType::Bun, MinifierType::Bun);
    write_manifest("units/0");
    assert!(matches!(
        LibraryIndex::load(
            &dir,
            &target_bun,
            crate::libindex::recipe::TerserSlices::Off
        ),
        Err(LoadError::Unusable(_))
    ));
    write_manifest(&fingerprint_versions().units);
    let ix = LibraryIndex::load(
        &dir,
        &target_bun,
        crate::libindex::recipe::TerserSlices::Off,
    )
    .expect("loads");
    assert_eq!((ix.id.len(), ix.slice.as_str()), (64, "bun-bun-cjs-min"));
    assert_eq!(
        claims(&ix, &[whole("a", 150)], structural()),
        vec![(0, "zod".into())]
    );
    match LibraryIndex::load(
        &dir,
        &target_node,
        crate::libindex::recipe::TerserSlices::Off,
    ) {
        Err(LoadError::NoSlice { why, .. }) => assert!(why.contains("bun:target=node"), "{why}"),
        other => panic!("a target-node bundle has no slice here: {:?}", other.err()),
    }
    // the lookup tables are the index: one missing is an unusable index
    std::fs::remove_file(dir.join(crate::libindex::table::LITERALS_FILE)).unwrap();
    match LibraryIndex::load(
        &dir,
        &target_bun,
        crate::libindex::recipe::TerserSlices::Off,
    ) {
        Err(LoadError::Unusable(e)) => assert!(e.contains("literals.bin"), "{e}"),
        other => panic!("no literal table: {:?}", other.err()),
    }
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn function_hits_need_no_content_tokens_by_default() {
    // Minified library logic often has no strings, globals or property
    // names (the lab's Step B: content floor on whole statements only).
    let index = LibraryIndex::from_rows(vec![row("f", "zod", 150, 2000)], vec![]);
    let mut q = whole("none", 200);
    let mut f = print("f", 150, 10);
    f.content = 0;
    q.functions = vec![f];
    assert_eq!(claims(&index, &[q], structural()), vec![(0, "zod".into())]);
}

#[test]
fn the_literal_margin_by_family_ignores_a_sibling_holding_the_same_literals() {
    // @smithy/core and @smithy/util both hold the 12 strings: by package
    // the runner-up ties the best (margin 0, refused); by family the
    // runner-up is the best OTHER family's file (none), so it agrees.
    let strings: Vec<String> = (0..12).map(|i| format!("S:x-amz header {i}")).collect();
    let held: Vec<&str> = strings.iter().map(String::as_str).collect();
    let mut literal_rows = vec![
        lits("@smithy/core", "index.js", &held),
        lits("@smithy/util", "index.js", &held),
    ];
    for i in 0..30 {
        literal_rows.push(lits(
            &format!("pkg{i}"),
            "index.js",
            &[&format!("S:unrelated {i}")],
        ));
    }
    let index = LibraryIndex::from_rows(vec![row("a", "@aws-sdk/core", 150, 2000)], literal_rows);
    let array: Vec<String> = (0..12).map(|i| format!("\"x-amz header {i}\"")).collect();
    let text = format!("var s=[{}];", array.join(","));
    let p = [whole("a", 150)];
    let queries = [UnitQuery {
        unit: 0,
        prints: &p[0],
        module_bytes: 1000,
        text: &text,
        callees: &[],
    }];
    let family = |p: &str| {
        if p.starts_with("@aws-sdk/") || p.starts_with("@smithy/") {
            "aws".to_string()
        } else {
            p.to_string()
        }
    };
    let same = |a: &str, b: &str| family(a) == family(b);
    let got = claim_units(&index, &queries, LookupRules::default(), &same);
    assert!(
        got.is_empty(),
        "a tie between siblings is refused by package: {got:?}"
    );
    let by_family = LookupRules {
        literal_margin_by_family: true,
        ..LookupRules::default()
    };
    let got = claim_units(&index, &queries, by_family, &same);
    assert_eq!(got.len(), 1, "{got:?}");
    assert_eq!(got[0].package, "@smithy/core");
}

#[test]
fn owners_counted_by_family_let_a_family_and_one_carrier_through() {
    // exp125: a bigger index adds carriers (a package whose dist inlines the
    // code). Three members of one scope family plus one carrier are FOUR
    // owners but TWO families: by package the print is too common to say
    // anything; by family it still names its family.
    let index = LibraryIndex::from_rows(
        vec![
            row("wasm", "@shiki/core", 150, 100),
            row("wasm", "@shiki/engine", 150, 100),
            row("wasm", "@shiki/main", 150, 100),
            row("wasm", "zcarrier", 150, 100),
            row("many", "a", 150, 100),
            row("many", "b", 150, 100),
            row("many", "c", 150, 100),
            row("many", "d", 150, 100),
        ],
        vec![],
    );
    let q = [whole("wasm", 150), whole("many", 150)];
    assert!(claims(&index, &q, structural()).is_empty());
    let by_family = LookupRules {
        max_packages_by_family: true,
        ..structural()
    };
    assert_eq!(
        claims(&index, &q, by_family),
        vec![(0, "@shiki/core".into())]
    );
}

/// An on-disk index of `slices` (slice name, recipe, rows; each its
/// bundler's default), current stamps, no edge rows.
fn write_index(dir: &std::path::Path, slices: Vec<(&str, &str, Vec<UnitRow>)>) {
    write_index_with_edges(dir, slices, Vec::new());
}

/// [`write_index`] with edge rows per slice (an `/4` index's full table
/// set, exp155's callee-print table included).
fn write_index_with_edges(
    dir: &std::path::Path,
    slices: Vec<(&str, &str, Vec<UnitRow>)>,
    edges: Vec<(&str, Vec<EdgeRow>)>,
) {
    use crate::libindex::format::{
        INDEX_FORMAT, Manifest, RECIPE_VERSION, SliceRecord, fingerprint_versions,
    };
    std::fs::create_dir_all(dir).unwrap();
    let records = slices
        .iter()
        .map(|(slice, recipe, _)| SliceRecord {
            slice: slice.to_string(),
            recipe: recipe.to_string(),
            recipe_version: RECIPE_VERSION.into(),
            default_for_bundler: true,
            bundler: recipe.split(':').next().unwrap().into(),
            version: "1.0.0".into(),
            published: None,
            newest_at_as_of: None,
            bundler_after_as_of: false,
        })
        .collect();
    let m = Manifest {
        format: INDEX_FORMAT.into(),
        as_of: "2026-10-12".into(),
        fingerprints: fingerprint_versions(),
        registry: Default::default(),
        slices: records,
        packages: vec![],
        refused: vec![],
        shards: Default::default(),
        tables: Default::default(),
        data: Default::default(),
    };
    std::fs::write(
        dir.join("manifest.json"),
        serde_json::to_string(&m).unwrap(),
    )
    .unwrap();
    let units = slices
        .into_iter()
        .map(|(slice, _, rows)| (slice.to_string(), rows))
        .collect();
    let edges = edges.into_iter().map(|(s, e)| (s.to_string(), e)).collect();
    for (name, bytes) in crate::libindex::table::lookup_files(&units, Some(&edges), &[]).unwrap() {
        std::fs::write(dir.join(name), bytes).unwrap();
    }
}

/// exp127: `terser_slices` reads the index's terser-built slices beside
/// the chosen one — `union` always (each owner once, at its smallest
/// module), `fallback` only for a print the chosen slice does not hold —
/// and `off` (the default) reads the chosen slice alone.
#[test]
fn the_loader_reads_the_terser_built_slices_beside_the_chosen_one() {
    use crate::libindex::recipe::{TerserSlices, detect_recipe};
    use humanify_model::detection::{BundlerType, MinifierType};
    let dir = std::env::temp_dir().join(format!("humanify-terser-slices-{}", std::process::id()));
    write_index(
        &dir,
        vec![
            (
                "esbuild-node-cjs-min",
                "esbuild:target=node,format=cjs,minify",
                vec![row("a", "zod", 150, 900)],
            ),
            (
                "rollup-node-cjs-min",
                "rollup:target=node,format=cjs,minify",
                vec![
                    row("a", "zod", 150, 700),
                    row("a", "yup", 150, 800),
                    row("b", "lodash", 150, 10),
                ],
            ),
            (
                "webpack-node-cjs-min",
                "webpack:target=node,format=cjs,minify",
                vec![row("c", "chalk", 150, 10), row("a", "zod", 150, 2000)],
            ),
        ],
    );
    let esb = detect_recipe("var a;", BundlerType::Esbuild, MinifierType::Esbuild);
    let hits = |mode| {
        let ix = LibraryIndex::load(&dir, &esb, mode).expect("loads");
        assert_eq!(
            ix.slice, "esbuild-node-cjs-min",
            "the chosen slice is recorded"
        );
        ["a", "b", "c"].map(|h| {
            ix.occurrences(&hx(h))
                .into_iter()
                .map(|(o, m)| (o.to_string(), m))
                .collect::<Vec<_>>()
        })
    };
    let own = |o: &str, m: u64| (o.to_string(), m);
    assert_eq!(
        hits(TerserSlices::Off),
        [vec![own("zod", 900)], vec![], vec![]],
        "off reads the chosen slice alone"
    );
    assert_eq!(
        hits(TerserSlices::Union),
        [
            vec![own("zod", 700), own("yup", 800)],
            vec![own("lodash", 10)],
            vec![own("chalk", 10)]
        ],
        "union: each owner once, at its smallest module over the slices read"
    );
    assert_eq!(
        hits(TerserSlices::Fallback),
        [
            vec![own("zod", 900)],
            vec![own("lodash", 10)],
            vec![own("chalk", 10)]
        ],
        "fallback: the terser slices only where the chosen slice holds nothing"
    );
    let ix = LibraryIndex::load(&dir, &esb, TerserSlices::Union).unwrap();
    assert!(
        ix.slice_reason().contains("rollup-node-cjs-min")
            && ix.slice_reason().contains("webpack-node-cjs-min"),
        "the reason names the slices read beside it: {}",
        ix.slice_reason()
    );
    std::fs::remove_dir_all(&dir).ok();
}

/// exp127: `terser_slices = "auto"` reads the terser-built slices only for
/// a bundle whose whole-unit prints the terser slices alone hold a share
/// of (nodes, over the prints either side holds) at least `terser_share`:
/// a terser-minified bundle, not a bundle the extra rows only add
/// look-alikes to. Before the bundle's verdict, and below the share, it
/// reads the chosen slice alone.
#[test]
fn auto_reads_the_terser_slices_only_for_a_bundle_they_hold_a_share_of() {
    use crate::libindex::recipe::{TerserSlices, detect_recipe};
    use humanify_model::detection::{BundlerType, MinifierType};
    let dir = std::env::temp_dir().join(format!("humanify-terser-auto-{}", std::process::id()));
    write_index(
        &dir,
        vec![
            (
                "esbuild-node-cjs-min",
                "esbuild:target=node,format=cjs,minify",
                vec![row("a", "zod", 150, 900)],
            ),
            (
                "rollup-node-cjs-min",
                "rollup:target=node,format=cjs,minify",
                vec![row("b", "lodash", 150, 10), row("c", "chalk", 150, 10)],
            ),
        ],
    );
    let esb = detect_recipe("var a;", BundlerType::Esbuild, MinifierType::Esbuild);
    let rules = structural();
    let bundle = |hashes: &[(&str, u32)]| -> Vec<Option<StatementPrints>> {
        hashes.iter().map(|&(h, n)| Some(whole(h, n))).collect()
    };
    let ix = LibraryIndex::load(&dir, &esb, TerserSlices::Auto { share: 0.5 }).unwrap();
    assert!(
        ix.occurrences(&hx("b")).is_empty(),
        "no verdict yet: the chosen slice alone"
    );
    // a held 150 nodes on the chosen slice, b + c 300 on the terser slice alone: 2/3
    let share = ix.decide_terser_slices(
        &bundle(&[("a", 150), ("b", 150), ("c", 150), ("z", 150)]),
        &rules,
    );
    assert!((share - 2.0 / 3.0).abs() < 1e-9, "{share}");
    assert_eq!(
        ix.occurrences(&hx("b")),
        vec![("lodash", 10)],
        "over the share: union"
    );
    assert!(ix.slice_reason().contains("union"), "{}", ix.slice_reason());
    let ix = LibraryIndex::load(&dir, &esb, TerserSlices::Auto { share: 0.5 }).unwrap();
    let share = ix.decide_terser_slices(&bundle(&[("a", 300), ("b", 150)]), &rules);
    assert!((share - 1.0 / 3.0).abs() < 1e-9, "{share}");
    assert!(
        ix.occurrences(&hx("b")).is_empty(),
        "under the share: the chosen slice alone"
    );
    assert!(ix.slice_reason().contains("off"), "{}", ix.slice_reason());
    std::fs::remove_dir_all(&dir).ok();
}

/// exp129: an index's helper table (`helpers.jsonl`, the merge's record of
/// the prints the helper SOURCES' own builds hold) is read on load; an
/// index without one has no helper prints.
#[test]
fn the_loader_reads_the_helper_table_when_the_index_has_one() {
    use crate::libindex::format::HelperRow;
    use crate::libindex::recipe::{TerserSlices, detect_recipe};
    use humanify_model::detection::{BundlerType, MinifierType};
    let dir = std::env::temp_dir().join(format!("humanify-helpers-{}", std::process::id()));
    let slices = || {
        vec![(
            "esbuild-node-cjs-min",
            "esbuild:target=node,format=cjs,minify",
            vec![row("a", "zod", 150, 900)],
        )]
    };
    write_index(&dir, slices());
    let esb = detect_recipe("var a;", BundlerType::Esbuild, MinifierType::Esbuild);
    let ix = LibraryIndex::load(&dir, &esb, TerserSlices::Off).expect("loads");
    assert!(ix.helper_owners(&hx("h")).is_empty(), "no helper table");
    let line = serde_json::to_string(&HelperRow {
        hash: hx("h"),
        owners: vec!["tslib".into()],
    })
    .unwrap();
    std::fs::write(dir.join(crate::libindex::table::HELPERS_FILE), line + "\n").unwrap();
    let ix = LibraryIndex::load(&dir, &esb, TerserSlices::Off).expect("loads");
    assert!(
        ix.helper_owners(&hx("h")).is_empty(),
        "a table the manifest does not list is not the index's"
    );
    let mut m: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("manifest.json")).unwrap()).unwrap();
    m["tables"][crate::libindex::table::HELPERS_FILE] = serde_json::json!("sha");
    std::fs::write(dir.join("manifest.json"), m.to_string()).unwrap();
    let ix = LibraryIndex::load(&dir, &esb, TerserSlices::Off).expect("loads");
    assert_eq!(ix.helper_owners(&hx("h")), ["tslib".to_string()]);
    assert!(ix.helper_owners(&hx("a")).is_empty());
    assert!(
        ix.occurrences(&hx("h")).is_empty(),
        "a helper print is no unit row"
    );
    let rows = LibraryIndex::from_rows(vec![], vec![]).with_helpers(vec![HelperRow {
        hash: hx("h"),
        owners: vec!["@babel/runtime".into()],
    }]);
    assert_eq!(rows.helper_owners(&hx("h")), ["@babel/runtime".to_string()]);
    std::fs::remove_dir_all(&dir).ok();
}

// ---------------------------------------------------------------------------
// The call-context rule (exp153)
// ---------------------------------------------------------------------------

use crate::libindex::callctx::Callee;
use crate::libindex::format::EdgeRow;
use crate::libindex::lookup::{CallContextRules, CallSupport};

fn edge(hash: &str, owner: &str, inside: &[&str]) -> EdgeRow {
    EdgeRow {
        hash: hx(hash),
        owner: owner.into(),
        file: "index.js".into(),
        inside: inside.iter().map(|h| hx(h)).collect(),
        out_of: Vec::new(),
    }
}

fn callee(name: &str) -> Callee {
    Callee {
        key: crate::libindex::table::unit_key(&hx(name)).unwrap(),
        nodes: 40,
    }
}

/// [`claims`] with each query's callees.
fn claims_called(
    index: &LibraryIndex,
    prints: &[(StatementPrints, Vec<Callee>)],
    rules: LookupRules,
) -> Vec<(u32, String)> {
    let queries: Vec<UnitQuery<'_>> = prints
        .iter()
        .enumerate()
        .map(|(i, (p, c))| UnitQuery {
            unit: i as u32,
            prints: p,
            module_bytes: 1000,
            text: "",
            callees: c,
        })
        .collect();
    claim_units(index, &queries, rules, &same)
        .into_iter()
        .map(|c| (c.unit, c.package))
        .collect()
}

fn with_call_context(call_context: CallContextRules) -> LookupRules {
    LookupRules {
        call_context,
        ..structural()
    }
}

#[test]
fn call_context_can_stand_in_for_the_position_window() {
    let index = LibraryIndex::from_rows_with_edges(
        vec![row("w", "zod", 40, 2000), row("n", "zod", 40, 2000)],
        vec![edge("w", "zod", &["h"])],
        vec![],
    );
    // a weak claim with no neighbour, its callee recorded
    let lone = [(whole("w", 40), vec![callee("h")])];
    assert!(claims_called(&index, &lone, structural()).is_empty());
    let or = with_call_context(CallContextRules {
        support: CallSupport::Or,
        ..CallContextRules::default()
    });
    assert_eq!(claims_called(&index, &lone, or), vec![(0, "zod".into())]);
    // a weak claim WITH a neighbour but a callee the row does not record:
    // the window keeps it, call context alone does not
    let near = [
        (whole("w", 40), vec![callee("other")]),
        (whole("n", 40), vec![]),
    ];
    assert_eq!(claims_called(&index, &near, structural()).len(), 2);
    let replace = with_call_context(CallContextRules {
        support: CallSupport::Replace,
        ..CallContextRules::default()
    });
    assert!(claims_called(&index, &near, replace).is_empty());
    assert_eq!(claims_called(&index, &near, or).len(), 2);
}

#[test]
fn call_context_relaxes_the_node_floor_and_the_package_cap_for_what_it_confirms() {
    let index = LibraryIndex::from_rows_with_edges(
        vec![
            row("small", "zod", 25, 2000),
            row("many", "a", 150, 2000),
            row("many", "b", 150, 2000),
            row("many", "c", 150, 2000),
            row("many", "d", 150, 2000),
        ],
        vec![edge("small", "zod", &["h"]), edge("many", "c", &["k"])],
        vec![],
    );
    let q = [
        (whole("small", 25), vec![callee("h")]),
        (whole("many", 150), vec![callee("k")]),
    ];
    assert!(claims_called(&index, &q, structural()).is_empty());
    let relax =
        with_call_context(CallContextRules::from_name("relax", Default::default()).unwrap());
    assert_eq!(
        claims_called(&index, &q, relax),
        vec![(0, "zod".into()), (1, "c".into())]
    );
    // unconfirmed: nothing relaxed
    let q = [
        (whole("small", 25), vec![callee("x")]),
        (whole("many", 150), vec![]),
    ];
    assert!(claims_called(&index, &q, relax).is_empty());
}

#[test]
fn call_context_named_settings_round_trip_and_the_full_combo_turns_every_role_on() {
    for name in CallContextRules::NAMES {
        let rules = CallContextRules::from_name(name, Default::default())
            .unwrap_or_else(|| panic!("{name} must parse"));
        assert_eq!(rules.name(), Some(name), "{name} must round-trip");
    }
    let full = CallContextRules::from_name("tie-break+support-or+relax", Default::default())
        .expect("the full combination is a named setting");
    assert!(full.tie_break);
    assert_eq!(full.support, CallSupport::Or);
    assert_eq!(full.veto, CallSupport::Or);
    assert_eq!(full.relax_floor, crate::libindex::units::UNIT_ROW_MIN_NODES);
    assert!(full.relax_common);
    assert_eq!(full.name(), Some("tie-break+support-or+relax"));
}

// ---------------------------------------------------------------------------
// The flat callee-print table (exp155): every callee print any edge row
// records, keyed — including one with no unit row of its own (under the
// row floor, or dropped with its row), so a scoped fit can tell "unknown
// to the index" from "recorded, by another family" instead of refuting
// both alike (the exp153 census's refutation gap).
// ---------------------------------------------------------------------------

#[test]
fn a_rowless_callee_print_resolves_to_its_recorders_in_memory_and_on_disk() {
    let key = |h: &str| crate::libindex::table::unit_key(&hx(h)).unwrap();
    let index = LibraryIndex::from_rows_with_edges(
        vec![row("w", "zod", 40, 2000), row("y", "yup", 40, 2000)],
        vec![edge("w", "zod", &["h"]), edge("y", "yup", &["w"])],
        vec![],
    );
    assert!(index.has_callee_prints());
    // "h" holds NO unit row anywhere — yet the flat table answers for it
    assert!(
        index.unit_hits(&hx("h")).is_empty(),
        "h is under the row floor"
    );
    // owner ids are sorted package names: yup = 0, zod = 1
    assert_eq!(
        index.callee_recorders(key("h")),
        vec![1],
        "zod records h INSIDE"
    );
    // "w" has a row of its own and is ALSO recorded by yup's row
    assert_eq!(
        index.callee_recorders(key("w")),
        vec![0],
        "yup records w INSIDE"
    );
    assert!(
        index.callee_recorders(key("nope")).is_empty(),
        "an unrecorded print"
    );
    // a /3 index records no edges and no callee prints: every query is
    // silence, never evidence against a claim
    let plain = LibraryIndex::from_rows(vec![row("w", "zod", 40, 2000)], vec![]);
    assert!(!plain.has_callee_prints());
    assert!(plain.callee_recorders(key("h")).is_empty());

    // the same answer from an index on disk, read on demand
    use crate::libindex::recipe::{TerserSlices, detect_recipe};
    use humanify_model::detection::{BundlerType, MinifierType};
    let dir = std::env::temp_dir().join(format!("humanify-callee-prints-{}", std::process::id()));
    write_index_with_edges(
        &dir,
        vec![(
            "bun-node-cjs-min",
            "bun:target=node,format=cjs,minify",
            vec![row("w", "zod", 40, 2000)],
        )],
        vec![("bun-node-cjs-min", vec![edge("w", "zod", &["h"])])],
    );
    let bun = detect_recipe("var a;", BundlerType::Bun, MinifierType::Bun);
    let loaded = LibraryIndex::load(&dir, &bun, TerserSlices::Off).expect("loads");
    assert!(loaded.has_callee_prints());
    assert!(
        loaded.unit_hits(&hx("h")).is_empty(),
        "the row floor holds on disk too"
    );
    assert_eq!(
        loaded.callee_recorders(key("h")),
        vec![0],
        "the on-disk flat table answers a rowless print"
    );
    std::fs::remove_dir_all(&dir).ok();
}

// ---------------------------------------------------------------------------
// The down-scoped tie-break (exp155): what the flat callee-print table
// buys exp153's tie resolution — an unheld callee the index knows NOWHERE
// is skipped instead of counted against (the DOWN-only deviation, declared
// in docs/responsibility.md), so the one tied family whose rows hold every
// callee the index has ever recorded wins a tie the plain fit cannot
// touch, and an index without the table answers nothing.
// ---------------------------------------------------------------------------

#[test]
fn the_down_scoped_tie_break_resolves_a_tie_the_plain_fit_cannot() {
    let index = LibraryIndex::from_rows_with_edges(
        vec![row("w", "alpha", 40, 2000), row("w", "beta", 40, 2000)],
        vec![edge("w", "alpha", &[]), edge("w", "beta", &["held"])],
        vec![],
    );
    let prints = [(whole("w", 40), vec![callee("held"), callee("unknown")])];
    let queries: Vec<UnitQuery<'_>> = prints
        .iter()
        .enumerate()
        .map(|(i, (p, c))| UnitQuery {
            unit: i as u32,
            prints: p,
            module_bytes: 1000,
            text: "",
            callees: c,
        })
        .collect();
    let pick = |index: &LibraryIndex, rules: LookupRules| {
        crate::libindex::lookup::picks(index, &queries, &rules, &same)[0]
            .map(|p| index.package_name(p.package).to_string())
    };
    // both tied packages refute under the plain fit — alpha's rows hold
    // nothing, beta's lack the unknown callee — so exp153's tie-break
    // confirms neither and the package-order fallback wins
    let plain = with_call_context(
        CallContextRules::from_name("tie-break", Default::default()).expect("a named setting"),
    );
    assert_eq!(pick(&index, plain).as_deref(), Some("alpha"));
    // the DOWN-scoped fit skips the callee the index knows nowhere and
    // confirms the one tied family that holds every recorded callee
    let down = with_call_context(
        CallContextRules::from_name("tie-break-down", Default::default()).expect("a named setting"),
    );
    assert_eq!(pick(&index, down).as_deref(), Some("beta"));
    // a scoped fit on an index without the flat table is SILENT — never
    // evidence against a tied family — so the fallback pick stands
    let no_table = LibraryIndex::from_rows(
        vec![row("w", "alpha", 40, 2000), row("w", "beta", 40, 2000)],
        vec![],
    );
    assert!(!no_table.has_callee_prints());
    assert_eq!(pick(&no_table, down).as_deref(), Some("alpha"));
}

// -- restoration capture (exp160) ------------------------------------------

fn query<'q>(i: u32, p: &'q StatementPrints) -> UnitQuery<'q> {
    UnitQuery {
        unit: i,
        prints: p,
        module_bytes: 1000,
        text: "",
        callees: &[],
    }
}

fn named(slot: u32, name: &str) -> crate::libindex::names::RestoredName {
    crate::libindex::names::RestoredName {
        slot,
        name: name.into(),
        kind: crate::libindex::names::NameKind::Local,
    }
}

#[test]
fn restoration_answers_answers_a_claimed_units_whole_print() {
    use crate::libindex::table::RestoreAnswer;
    let index = LibraryIndex::from_rows_with_names(
        vec![row("a", "dep", 40, 100)],
        vec![crate::libindex::format::NameRow {
            hash: hx("a"),
            owner: "dep".into(),
            file: "index.js".into(),
            names: vec![named(0, "pLimit"), named(1, "concurrency")],
            versions: vec![("1.0.0".into(), "1.0.0".into())],
        }],
        Vec::new(),
    );
    let p = whole("a", 40);
    let q = query(0, &p);
    let answers = index.restoration_answers(&q, &structural(), &same, "dep");
    assert_eq!(answers.len(), 1);
    assert_eq!(answers[0].0, hx("a"));
    match &answers[0].1 {
        RestoreAnswer::Single(names) => {
            assert_eq!(names[0].name, "pLimit");
            assert_eq!(names[1].slot, 1);
        }
        other => panic!("{other:?}"),
    }
    // a package the index does not know carries no restoration
    assert!(
        index
            .restoration_answers(&q, &structural(), &same, "other")
            .is_empty()
    );
}

#[test]
fn restoration_answers_covers_the_function_route_per_contributing_print() {
    use crate::libindex::table::RestoreAnswer;
    let index = LibraryIndex::from_rows_with_names(
        vec![row("f1", "dep", 40, 100), row("f2", "dep", 30, 100)],
        vec![
            crate::libindex::format::NameRow {
                hash: hx("f1"),
                owner: "dep".into(),
                file: "index.js".into(),
                names: vec![named(0, "alpha")],
                versions: vec![("1.0.0".into(), "1.0.0".into())],
            },
            crate::libindex::format::NameRow {
                hash: hx("f2"),
                owner: "dep".into(),
                file: "index.js".into(),
                names: vec![named(0, "beta")],
                versions: vec![("1.0.0".into(), "1.0.0".into())],
            },
        ],
        Vec::new(),
    );
    // the whole print matches nothing; two functions do (largest first)
    let p = StatementPrints {
        whole: print("w", 60, 0),
        functions: vec![print("f2", 30, 50), print("f1", 40, 0)],
    };
    let q = query(0, &p);
    let answers = index.restoration_answers(&q, &structural(), &same, "dep");
    // the largest function contributes first
    assert_eq!(
        answers.iter().map(|(h, _)| h.as_str()).collect::<Vec<_>>(),
        vec![hx("f1"), hx("f2")]
    );
    assert!(
        answers
            .iter()
            .all(|(_, a)| matches!(a, RestoreAnswer::Single(_)))
    );
}

#[test]
fn restoration_answers_answers_the_function_prints_beside_a_whole_hit() {
    // exp164: the capture's depth gap. A whole-print hit short-circuits
    // the structural route before the nested functions are consulted, so
    // a wrapper's nested locals were never captured — the mint premium's
    // other half. The capture needs BOTH: the whole's answer names the
    // wrapper's own slots, the functions' name theirs.
    use crate::libindex::table::RestoreAnswer;
    let index = LibraryIndex::from_rows_with_names(
        vec![row("w", "dep", 40, 100), row("f", "dep", 30, 100)],
        vec![
            crate::libindex::format::NameRow {
                hash: hx("w"),
                owner: "dep".into(),
                file: "index.js".into(),
                names: vec![named(0, "outer")],
                versions: vec![("1.0.0".into(), "1.0.0".into())],
            },
            crate::libindex::format::NameRow {
                hash: hx("f"),
                owner: "dep".into(),
                file: "index.js".into(),
                names: vec![named(0, "inner")],
                versions: vec![("1.0.0".into(), "1.0.0".into())],
            },
        ],
        Vec::new(),
    );
    // The whole print MATCHES (a captured answer exists for it) and a
    // nested function matches beside it.
    let p = StatementPrints {
        whole: print("w", 40, 0),
        functions: vec![print("f", 30, 10)],
    };
    let q = query(0, &p);
    let answers = index.restoration_answers(&q, &structural(), &same, "dep");
    assert_eq!(
        answers.iter().map(|(h, _)| h.as_str()).collect::<Vec<_>>(),
        vec![hx("w"), hx("f")],
        "the whole's answer first, the function's beside it"
    );
    assert!(
        answers
            .iter()
            .all(|(_, a)| matches!(a, RestoreAnswer::Single(_)))
    );
}

#[test]
fn restoration_answers_passes_an_ambiguous_answer_through_for_the_callers_policy() {
    use crate::libindex::table::RestoreAnswer;
    let index = LibraryIndex::from_rows_with_names(
        vec![row("b", "dep", 40, 100)],
        vec![
            crate::libindex::format::NameRow {
                hash: hx("b"),
                owner: "dep".into(),
                file: "index.js".into(),
                names: vec![named(0, "one")],
                versions: vec![("1.0.0".into(), "1.0.0".into())],
            },
            crate::libindex::format::NameRow {
                hash: hx("b"),
                owner: "dep".into(),
                file: "index.js".into(),
                names: vec![named(0, "two")],
                versions: vec![("2.0.0".into(), "2.0.0".into())],
            },
        ],
        Vec::new(),
    );
    let p = whole("b", 40);
    let q = query(0, &p);
    match &index.restoration_answers(&q, &structural(), &same, "dep")[0].1 {
        RestoreAnswer::Ambiguous(lists) => assert_eq!(lists.len(), 2),
        other => panic!("{other:?}"),
    }
}

#[test]
fn look_hits_picks_one_row_per_owner_at_the_smallest_module_bytes() {
    // two rows of the same print under one owner: the pick is the smallest
    // module bytes, then the lowest row
    let index = LibraryIndex::from_rows(
        vec![
            row("a", "dep", 40, 100),
            UnitRow {
                module_bytes: 50,
                ..row("a", "dep", 40, 100)
            },
        ],
        Vec::new(),
    );
    let hits = index
        .look_hits(&hx("a"), 1000, &structural(), &same)
        .expect("owners");
    assert_eq!(hits.len(), 1, "one owner");
    let hit = hits.values().next().expect("one hit");
    assert_eq!(hit.module_bytes, 50, "the smallest module bytes wins");
}
