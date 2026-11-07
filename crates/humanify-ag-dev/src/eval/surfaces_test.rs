use super::*;

fn stmt(hash: &str, text: &str, declared: &[&str]) -> Stmt {
    Stmt {
        hash: hash.into(),
        text: text.into(),
        lines: line_count(text),
        declared: declared.iter().map(|s| s.to_string()).collect(),
    }
}

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

#[test]
fn statement_kinds_and_their_split_partition_the_totals() {
    let prior = vec![
        stmt("h1", "a", &[]),
        stmt("h2", "b", &[]),
        stmt("h3", "c\nc", &[]),
    ];
    let fresh = vec![
        stmt("h1", "a", &[]),
        stmt("h2", "B", &[]),
        stmt("h3", "c\nc", &[]),
        stmt("h4", "new\nnew", &[]),
    ];
    let kinds = statement_kinds(&fresh, &prior);
    use StatementKind::*;
    assert_eq!(kinds, [Clean, Noise, Clean, Novel]);
    let fo = s(&["src/a.js", "src/a.js", "vendor/x/m.js", "src/b.js"]);
    let po = s(&["src/a.js", "src/a.js", "src/a.js"]);
    let (split, to_vendor, to_app) =
        statement_split(&fresh, &prior, &kinds, Some(&fo), Some(&po)).unwrap();
    assert_eq!((to_vendor, to_app), (1, 0));
    assert_eq!(split.cross.unchanged_clean, 1);
    assert_eq!(split.app.novel, 1);
    assert_eq!(split.app.real_lines, 2);
    assert_eq!(split.app.total + split.vendor.total + split.cross.total, 4);
    // An order that does not cover the bundle is refused.
    assert!(statement_split(&fresh, &prior, &kinds, Some(&fo[..2]), Some(&po)).is_none());
}

#[test]
fn names_relocs_and_mints() {
    let fresh = Ledger {
        name_to_files: vec![
            ("a".into(), s(&["src/x.js"])),
            ("b".into(), s(&["vendor/l/m.js"])),
            ("c".into(), s(&["src/y.js"])),
        ],
        order: Some(s(&["src/x.js", "vendor/l/m.js", "src/y.js"])),
        hashes: Some(s(&["h1", "h2", "h3"])),
    };
    let prior = Ledger {
        name_to_files: vec![
            ("a".into(), s(&["src/z.js"])),
            ("b".into(), s(&["src/x.js"])),
        ],
        order: Some(s(&["src/z.js", "src/x.js", "src/y.js"])),
        hashes: Some(s(&["h1", "h2", "h2"])),
    };
    let n = name_split(&fresh, &prior);
    assert_eq!(
        n.app,
        NameCounts {
            fresh_names: 2,
            novel_names: 1,
            same_name_moved_file: 1
        }
    );
    assert_eq!(
        n.cross,
        NameCounts {
            fresh_names: 1,
            novel_names: 0,
            same_name_moved_file: 1
        }
    );
    // h2 occurs twice in the prior: not comparable.
    assert_eq!(
        unique_statement_pairs(&fresh, &prior),
        [("src/z.js".to_string(), "src/x.js".to_string())]
    );
    let r = reloc_split(&fresh, &prior);
    assert_eq!(
        r.app,
        RelocCounts {
            statements_compared: 1,
            relocated_statements: 1
        }
    );
    // Mints: two `m` copies and one `m` binding per side force one each;
    // `q` (app only) is app; `zz` is declared nowhere: unattributed.
    let stmts = vec![
        stmt("h", "x", &["m", "q"]),
        stmt("h", "y", &["m"]),
        stmt("h", "z", &[]),
    ];
    let order = s(&["src/a.js", "vendor/l/m.js", "src/b.js"]);
    let m = mint_split(&s(&["m", "m", "q", "zz"]), &stmts, &order);
    assert_eq!(
        m,
        MintCounts {
            app: 2,
            vendor: 1,
            unattributed: 1
        }
    );
}

fn tree(name: &str, files: &[(&str, &str)]) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("dev-surfaces-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("src")).unwrap();
    std::fs::create_dir_all(d.join("vendor")).unwrap();
    for (rel, text) in files {
        let p = d.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
    d
}

#[test]
fn a_statement_that_moved_into_vendor_is_charged_once_as_a_move() {
    let moved = "function helper(a) {\n  return a + 1;\n}\n";
    let p = tree(
        "p",
        &[
            ("src/app.js", &format!("var k = 1;\n{moved}")),
            ("vendor/lib/m.js", "module.exports = 1;\n"),
        ],
    );
    let f = tree(
        "f",
        &[
            ("src/app.js", "var k = 1;\n"),
            ("vendor/lib/m.js", &format!("module.exports = 1;\n{moved}")),
        ],
    );
    let differ = super::super::difftool::Differ;
    let comp = super::super::compose::compose_diff(&p.join("src"), &f.join("src"), false).unwrap();
    let raw =
        super::super::vendor::decompose(&p.join("vendor"), &f.join("vendor"), &differ).unwrap();
    let layout = layout_in(&comp.tally, 0);
    let (app, vendor, cross) = boundary_lines(
        &p.join("src"),
        &f.join("src"),
        &p.join("vendor"),
        &f.join("vendor"),
        &comp.real_by_file,
        &layout,
        &raw,
    )
    .unwrap();
    assert_eq!(comp.tally.real, 3);
    assert_eq!(cross.to_vendor, (1, 6));
    assert_eq!(cross.app_side, 3);
    assert_eq!(app.real, 0);
    // Partition: app + vendor + cross = layout + vendor totals.
    assert_eq!(
        app.churn_lines + vendor.churn_lines + cross.churn_lines as i64,
        layout.churn_lines + raw.total_lines as i64
    );
    assert_eq!(cross.moves[0].from, "src/app.js");
    assert_eq!(cross.moves[0].to, "vendor/lib/m.js");
}
