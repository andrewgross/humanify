use super::*;
use crate::library::truth::Src;

/// An app install on disk: `files` are (path under the app, text).
fn install(tag: &str, files: &[(&str, &str)]) -> String {
    let d = std::env::temp_dir().join(format!("dev-audit15-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    for (p, text) in files {
        let f = d.join(p);
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(f, text).unwrap();
    }
    std::fs::create_dir_all(&d).unwrap();
    d.to_string_lossy().into_owned()
}

const NF_TEXT: &str = "module.exports = function fetch(url) { return url.trim(); };";
const TABLE_JSON: &str = r#"[["a140","table-row-zero"],["a180","table-row-one"],["a1c0","table-row-two"],["a240","table-row-three"],["a280","table-row-four"]]"#;

const TABLE: &str = r#"x.exports=[["a140","table-row-zero"],["a180","table-row-one"],["a1c0","table-row-two"],["a240","table-row-three"],["a280","table-row-four"]];"#;

fn unit(i: usize, s: i64, e: i64) -> Unit {
    Unit {
        i,
        s,
        e,
        b: e - s,
        k: "Statement".into(),
        rt: false,
        mb: None,
        bindings: 0,
        m: None,
        w: None,
    }
}

fn truth_over(n: i64) -> Truth {
    Truth {
        method: "sourcemap".into(),
        segments: vec![(0, 0), (n, 1)],
        sources: vec![
            Src {
                label: "node-fetch".into(),
                version: Some("1.7.3".into()),
                file: Some("index.js".into()),
            },
            Src {
                label: "<app>".into(),
                version: None,
                file: None,
            },
        ],
        app_package: None,
        rows: Vec::new(),
        strata: Default::default(),
    }
}

fn label(pkg: &str) -> Label {
    Label {
        label: pkg.into(),
        pkg: Some(pkg.into()),
        version: Some("1.7.3".into()),
        file: Some("index.js".into()),
        ..Label::default()
    }
}

fn run(source: &str) -> (EntryAudit, HashMap<usize, Label>) {
    let mut bundle = TABLE.as_bytes().to_vec();
    bundle.extend_from_slice(b"\nvar app=1;");
    let base = install(
        &format!("run-{}", source.len()),
        &[("node_modules/node-fetch/index.js", source)],
    );
    let texts = EntryTexts::new(
        bundle,
        &base,
        vec![
            "node_modules/node-fetch/index.js".into(),
            "src/app.js".into(),
            "node_modules/iconv-lite/encodings/tables/cp950.json".into(),
        ],
        vec![
            Some(source.into()),
            Some("var app = 1;".into()),
            Some(r#"[["a140","table-row-zero"],["a180","table-row-one"],["a1c0","table-row-two"],["a240","table-row-three"],["a280","table-row-four"]]"#.into()),
        ],
    );
    let mut audit = EntryAudit::new(texts);
    let n = TABLE.len() as i64;
    let truth = truth_over(n);
    let u = unit(0, 0, n);
    let mut labels: HashMap<usize, Label> = [(0, label("node-fetch"))].into_iter().collect();
    let claimed: HashMap<usize, &str> = [(0, "iconv-lite")].into_iter().collect();
    let registry = Registry::new(std::path::Path::new("/nonexistent-registry"));
    let ctx = Ctx {
        lits: None,
        registry: &registry,
    };
    audit.defects(&ctx, &truth, (&[&u], &mut labels), &claimed);
    (audit, labels)
}

#[test]
fn a_defect_leaves_the_library_and_is_listed_with_its_claim_and_holder() {
    let (audit, labels) = run(r#"module.exports = function fetch(url) { return url.trim(); };"#);
    assert_eq!(labels[&0].label, DEFECT);
    assert!(!is_library(&labels[&0].label));
    assert_eq!(audit.defects.len(), 1);
    let d = &audit.defects[0];
    assert_eq!(d.truth, "node-fetch");
    assert_eq!(d.claimed.as_deref(), Some("iconv-lite"));
    assert_eq!(d.ev.held, 0);
    let (holder, share) = d.holder.clone().expect("the cp950 source holds it");
    assert!(
        holder.ends_with("cp950.json") && share == 1.0,
        "{holder} {share}"
    );
    assert_eq!(audit.defect_bytes(), TABLE.len() as i64);
}

#[test]
fn a_label_whose_file_holds_the_unit_is_kept() {
    let (audit, labels) = run(
        r#"module.exports = [["a140","table-row-zero"],["a180","table-row-one"],["a1c0","table-row-two"],["a240","table-row-three"],["a280","table-row-four"]];"#,
    );
    assert_eq!(labels[&0].label, "node-fetch");
    assert!(audit.defects.is_empty());
}

#[test]
fn a_misattributed_or_unreadable_label_is_never_judged() {
    // the label's file has no text: no verdict
    let (audit, labels) = run("");
    assert_eq!(labels[&0].label, "node-fetch");
    assert!(audit.defects.is_empty());
}

/// A three-source map: the unit's span is filed under node-fetch's
/// index.js (source 0); source 2 is `second`'s file, labelled `pkg2`.
fn run_with(
    second_label: &str,
    lab: Label,
    lits: Option<&LiteralIndex>,
) -> (EntryAudit, HashMap<usize, Label>) {
    run_installed(
        second_label,
        lab,
        lits,
        &[("node_modules/node-fetch/index.js", NF_TEXT)],
    )
}

fn run_installed(
    second_label: &str,
    lab: Label,
    lits: Option<&LiteralIndex>,
    installed: &[(&str, &str)],
) -> (EntryAudit, HashMap<usize, Label>) {
    let n = TABLE.len() as i64;
    let tag = format!("with-{second_label}-{}-{}", lits.is_some(), installed.len());
    let base = install(&tag, installed);
    let texts = EntryTexts::new(
        TABLE.as_bytes().to_vec(),
        &base,
        vec![
            "node_modules/node-fetch/index.js".into(),
            "src/app.js".into(),
            "node_modules/node-fetch/lib/tables.json".into(),
        ],
        vec![
            Some("module.exports = function fetch(url) { return url.trim(); };".into()),
            Some("var app = 1;".into()),
            Some(r#"[["a140","table-row-zero"],["a180","table-row-one"],["a1c0","table-row-two"],["a240","table-row-three"],["a280","table-row-four"]]"#.into()),
        ],
    );
    let mut truth = truth_over(n);
    truth.sources.push(Src {
        label: second_label.into(),
        version: None,
        file: Some("lib/tables.json".into()),
    });
    let mut audit = EntryAudit::new(texts);
    let u = unit(0, 0, n);
    let mut labels: HashMap<usize, Label> = [(0, lab)].into_iter().collect();
    let claimed: HashMap<usize, &str> = HashMap::new();
    let registry = Registry::new(std::path::Path::new("/nonexistent-registry"));
    let ctx = Ctx {
        lits,
        registry: &registry,
    };
    audit.defects(&ctx, &truth, (&[&u], &mut labels), &claimed);
    (audit, labels)
}

#[test]
fn a_unit_under_the_wrong_file_of_the_right_package_is_no_defect() {
    // the table is node-fetch's own (lib/tables.json): the package holds it
    let (audit, labels) = run_with("node-fetch", label("node-fetch"), None);
    assert_eq!(labels[&0].label, "node-fetch");
    assert!(audit.defects.is_empty());
    // the same text under ANOTHER package's file: node-fetch lacks it
    let (audit, labels) = run_with("iconv-lite", label("node-fetch"), None);
    assert_eq!(labels[&0].label, DEFECT);
    assert_eq!(audit.defects.len(), 1);
}

#[test]
fn a_unit_the_label_package_publishes_is_no_defect() {
    // the map holds the table nowhere under node-fetch, but node-fetch's
    // published files (the index literal table) do
    let lits = LiteralIndex::from_rows(&[humanify_ag_core::libindex::format::LiteralRow {
        owner: "node-fetch".into(),
        file: "lib/tables.json".into(),
        lits: [
            "S:a140",
            "S:a180",
            "S:a1c0",
            "S:a240",
            "S:a280",
            "S:table-row-zero",
            "S:table-row-one",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect(),
        versions: vec![("1.7.3".into(), "1.7.3".into())],
    }]);
    let (audit, labels) = run_with("iconv-lite", label("node-fetch"), Some(&lits));
    assert_eq!(labels[&0].label, "node-fetch");
    assert!(audit.defects.is_empty());
}

#[test]
fn a_scorer_14_map_defect_relabel_is_left_alone() {
    let mut l = label("node-fetch");
    l.mapdefect = true;
    let (audit, labels) = run_with("iconv-lite", l, None);
    assert_eq!(labels[&0].label, "node-fetch");
    assert!(audit.defects.is_empty());
}

#[test]
fn the_installed_label_package_holding_the_unit_keeps_the_label() {
    // node-fetch's map file lacks the table, but the app's installed
    // node-fetch has it in a JSON file the map never names
    let (audit, labels) = run_installed(
        "iconv-lite",
        label("node-fetch"),
        None,
        &[
            ("node_modules/node-fetch/index.js", NF_TEXT),
            ("node_modules/node-fetch/data/table.json", TABLE_JSON),
        ],
    );
    assert_eq!(labels[&0].label, "node-fetch");
    assert!(audit.defects.is_empty());
}

#[test]
fn a_label_package_not_installed_on_this_host_cannot_be_a_defect() {
    let (audit, labels) = run_installed("iconv-lite", label("node-fetch"), None, &[]);
    assert_eq!(labels[&0].label, "node-fetch");
    assert!(audit.defects.is_empty());
}
