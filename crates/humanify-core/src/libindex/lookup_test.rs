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

fn structural() -> LookupRules {
    LookupRules {
        agreement: false,
        ..LookupRules::default()
    }
}

fn family(p: &str) -> String {
    p.split('/').next().unwrap_or(p).to_string()
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
        })
        .collect();
    claim_units(index, &queries, rules, &family)
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
        },
        UnitQuery {
            unit: 1,
            prints: &p[1],
            module_bytes: 1000,
            text: &text,
        },
    ];
    let family = |p: &str| {
        if p.starts_with("@aws-sdk/") || p.starts_with("@smithy/") {
            "aws".to_string()
        } else {
            p.to_string()
        }
    };
    let got = claim_units(&index, &queries, LookupRules::default(), &family);
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
    for (name, bytes) in crate::libindex::table::lookup_files(&units, &[]).unwrap() {
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
        LibraryIndex::load(&dir, &target_bun),
        Err(LoadError::Unusable(_))
    ));
    write_manifest(&fingerprint_versions().units);
    let ix = LibraryIndex::load(&dir, &target_bun).expect("loads");
    assert_eq!((ix.id.len(), ix.slice.as_str()), (64, "bun-bun-cjs-min"));
    assert_eq!(
        claims(&ix, &[whole("a", 150)], structural()),
        vec![(0, "zod".into())]
    );
    match LibraryIndex::load(&dir, &target_node) {
        Err(LoadError::NoSlice { why, .. }) => assert!(why.contains("bun:target=node"), "{why}"),
        other => panic!("a target-node bundle has no slice here: {:?}", other.err()),
    }
    // the lookup tables are the index: one missing is an unusable index
    std::fs::remove_file(dir.join(crate::libindex::table::LITERALS_FILE)).unwrap();
    match LibraryIndex::load(&dir, &target_bun) {
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
