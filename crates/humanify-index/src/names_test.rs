//! exp159: the names pass — a unit's restored names from the build's own
//! sourcemap and the package's original source. The map anchors each
//! minified declaration to the original line it came from; the original
//! file's identifiers pair with them line by line.

use crate::build::{SpongeRegions, unit_rows};
use crate::bun_node;
use crate::sourcemap::SourceMap;
use humanify_core::libindex::names::NameKind;

fn temp(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("humanify-index-names-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("mkdir");
    d
}

/// The original source of the dep package, its minified build and the map
/// between them: the segments anchor each minified declaration to the
/// original line it came from (ASCII, so byte columns are utf16 columns).
fn fixture(tag: &str) -> (std::path::PathBuf, String, SourceMap) {
    let root = temp(tag);
    let write = |rel: &str, text: &str| {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().expect("dir")).expect("mkdir");
        std::fs::write(p, text).expect("write");
    };
    write(
        "node_modules/dep/package.json",
        r#"{"name":"dep","version":"1.2.3"}"#,
    );
    write(
        "node_modules/dep/index.js",
        "function pLimit(concurrency) {\n  let activeCount = 0;\n  let queue = null;\n  function inner(item) {\n    activeCount = activeCount + item.length + concurrency;\n    queue = item;\n    return queue;\n  }\n  return inner;\n}\n",
    );
    let code = "function h(e){let i=0,l=null;function n(r){i=i+r.length+e;l=r;return l}return n}\n";
    // the minified declarations and the original line each anchors to
    let at = |needle: &str| u32::try_from(code.find(needle).expect("needle")).expect("col");
    let segments: Vec<(u32, u32, u32, u32)> = vec![
        (0, at("h("), 0, 0),
        (0, at("(e)"), 0, 0),
        (0, at("let i"), 0, 1),
        (0, at(",l=null"), 0, 2),
        (0, at("function n"), 0, 3),
        (0, at("(r)"), 0, 3),
        (0, at("return n}"), 0, 8),
    ];
    let map = SourceMap {
        sources: vec!["../node_modules/dep/index.js".into()],
        segments,
    };
    (root, code.to_string(), map)
}

#[test]
fn a_minified_unit_recovers_its_original_names() {
    let (root, code, map) = fixture("restore");
    let (rows, notes) = unit_rows(
        bun_node(),
        &code,
        &map,
        &root.join("out"),
        SpongeRegions::Drop,
        true,
    );
    assert_eq!(notes.mixed_spans, 0);
    // two prints: the whole statement (every declaration inside) and the
    // inner function (its own name and param; the outer locals are
    // references here, declared outside its span)
    assert_eq!(rows.len(), 2, "{rows:?}");
    let whole = rows
        .iter()
        .find(|r| r.names.len() == 6)
        .expect("the whole print");
    // the canonical mapping's slot order (its own deterministic walk —
    // locals, then params, then functions for this print), each slot its
    // ORIGINAL name and kind, paired by the DECL POSITION, never the slot
    // order
    let spelled: Vec<(u32, &str, NameKind)> = whole
        .names
        .iter()
        .map(|n| (n.slot, n.name.as_str(), n.kind))
        .collect();
    assert_eq!(
        spelled,
        vec![
            (0, "activeCount", NameKind::Local),
            (1, "queue", NameKind::Local),
            (2, "item", NameKind::Param),
            (3, "concurrency", NameKind::Param),
            (4, "inner", NameKind::Function),
            (5, "pLimit", NameKind::Function),
        ],
        "{spelled:?}"
    );
    let inner = rows
        .iter()
        .find(|r| r.names.len() == 2)
        .expect("the inner print");
    let spelled: Vec<(u32, &str, NameKind)> = inner
        .names
        .iter()
        .map(|n| (n.slot, n.name.as_str(), n.kind))
        .collect();
    // the inner print's OWN mapping (the walk slots its body's references
    // first — i, r, e, l — then its id): only the two identifiers DECLARED
    // inside it carry names, at their own print's slots
    assert_eq!(
        spelled,
        vec![
            (1, "item", NameKind::Param),
            (4, "inner", NameKind::Function)
        ],
        "{spelled:?}"
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn with_names_off_no_row_carries_names() {
    let (root, code, map) = fixture("plain");
    let (rows, _) = unit_rows(
        bun_node(),
        &code,
        &map,
        &root.join("out"),
        SpongeRegions::Drop,
        false,
    );
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|r| r.names.is_empty()));
    // and the row shape stays byte-identical with the names off: no
    // `names` key is even written (the serde skip)
    let text = serde_json::to_string(&rows[0]).unwrap();
    assert!(!text.contains("names"), "{text}");
    std::fs::remove_dir_all(&root).ok();
}
