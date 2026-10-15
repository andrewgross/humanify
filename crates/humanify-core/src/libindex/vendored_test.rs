//! Vendored-copy rows (exp115): a unit row whose FILE is a copy of another
//! package, read off the file's path inside the owner's package.

use std::collections::BTreeSet;

use crate::libindex::format::UnitRow;
use crate::libindex::vendored::{PathKind, mark_vendored, named_package};

fn known() -> BTreeSet<String> {
    [
        "ws",
        "typescript",
        "events",
        "katex",
        "chevrotain",
        "neo-async",
        "d3-color",
        "react",
        "index",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

fn never(_: &str, _: &str) -> bool {
    false
}

fn named(owner: &str, file: &str) -> Option<(String, PathKind)> {
    let k = known();
    named_package(owner, file, &|n| k.contains(n), &never)
}

#[test]
fn a_vendor_directory_names_its_package_directly() {
    // a strong vendor directory: the next segment is the copied package,
    // known to the index or not
    assert_eq!(
        named("@rspack/core", "compiled/neo-async/index.js"),
        Some(("neo-async".into(), PathKind::Direct))
    );
    assert_eq!(
        named("@antv/vendor", "lib-vendor/d3-color/src/color.js"),
        Some(("d3-color".into(), PathKind::Direct))
    );
    assert_eq!(
        named("x", "dist/vendors/left-pad/index.js"),
        Some(("left-pad".into(), PathKind::Direct))
    );
    assert_eq!(
        named("x", "compiled/@babel/core/index.js"),
        Some(("@babel/core".into(), PathKind::Direct))
    );
    assert_eq!(
        named("x", "lib/node_modules/@scope/pkg/a.js"),
        Some(("@scope/pkg".into(), PathKind::Direct))
    );
    // a FILE directly under a vendor directory names a package only when
    // the index knows the name
    assert_eq!(
        named("x", "compiled/ws.js"),
        Some(("ws".into(), PathKind::Direct))
    );
    assert_eq!(named("x", "compiled/unheard-of.js"), None);
}

#[test]
fn a_weak_directory_or_a_file_stem_names_a_known_package_to_be_confirmed() {
    assert_eq!(
        named("crossws", "dist/_chunks/libs/ws.mjs"),
        Some(("ws".into(), PathKind::Confirm))
    );
    // a bundler chunk hash is not part of the name
    assert_eq!(
        named("npm-check-updates", "build/chunks/typescript-DKNvXqx9.cjs"),
        Some(("typescript".into(), PathKind::Confirm))
    );
    assert_eq!(
        named("mermaid", "dist/katex-b82f950a.js"),
        Some(("katex".into(), PathKind::Confirm))
    );
    assert_eq!(
        named("@react-three/fiber", "dist/events-21d8cd92.cjs.dev.js"),
        Some(("events".into(), PathKind::Confirm))
    );
    // a weak directory's unknown name is nothing
    assert_eq!(named("x", "dist/libs/unheard-of.js"), None);
    // an all-lowercase suffix is part of a name, not a hash
    assert_eq!(named("x", "dist/react-dom.js"), None);
}

#[test]
fn the_owners_own_names_generic_names_and_its_family_never_count() {
    assert_eq!(named("crossws", "dist/shared/crossws.2ed26345.cjs"), None);
    assert_eq!(named("@scope/ws", "dist/ws.js"), None);
    assert_eq!(named("x", "dist/index.js"), None);
    assert_eq!(named("x", "compiled/index.js"), None);
    assert_eq!(named("x", "vendor/lib/a.js"), None);
    let k = known();
    let fam =
        |a: &str, b: &str| (a == "x-ts" && b == "typescript") || (a == "typescript" && b == "x-ts");
    assert_eq!(
        named_package("x-ts", "dist/typescript.js", &|n| k.contains(n), &fam),
        None
    );
}

fn row(hash: u64, owner: &str, file: &str) -> UnitRow {
    UnitRow {
        hash: format!("{hash:016x}"),
        owner: owner.into(),
        file: file.into(),
        nodes: 40,
        content: 5,
        module_bytes: 1000,
        versions: vec![("1.0.0".into(), "1.0.0".into())],
        vendored: None,
    }
}

#[test]
fn a_confirmed_file_must_share_its_prints_with_the_named_package() {
    let mut rows = Vec::new();
    // ncu's typescript chunk: 4 of its 10 prints are typescript's too
    for h in 0..10u64 {
        rows.push(row(
            h,
            "npm-check-updates",
            "build/chunks/typescript-DKNvXqx9.cjs",
        ));
        if h < 4 {
            rows.push(row(h, "typescript", "lib/typescript.js"));
        }
    }
    // fiber's own events chunk: 1 of 20 prints is held by events (chance)
    for h in 100..120u64 {
        rows.push(row(
            h,
            "@react-three/fiber",
            "dist/events-21d8cd92.cjs.dev.js",
        ));
    }
    rows.push(row(100, "events", "events.js"));
    // a direct vendor directory needs no confirmation
    rows.push(row(200, "@rspack/core", "compiled/neo-async/index.js"));
    rows.sort_by(|a, b| (&a.hash, &a.owner, &a.file).cmp(&(&b.hash, &b.owner, &b.file)));
    let k = known();
    let stats = mark_vendored(&mut rows, &|n| k.contains(n), &never);
    let marked = |owner: &str| {
        rows.iter()
            .filter(|r| r.owner == owner)
            .map(|r| r.vendored.clone())
            .collect::<BTreeSet<_>>()
    };
    assert_eq!(
        marked("npm-check-updates"),
        BTreeSet::from([Some("typescript".to_string())])
    );
    assert_eq!(marked("@react-three/fiber"), BTreeSet::from([None]));
    assert_eq!(
        marked("@rspack/core"),
        BTreeSet::from([Some("neo-async".to_string())])
    );
    // the origin's own rows are never marked
    assert_eq!(marked("typescript"), BTreeSet::from([None]));
    assert_eq!(marked("events"), BTreeSet::from([None]));
    assert_eq!(stats.rows, 11);
    assert_eq!(stats.files, 2);
}
