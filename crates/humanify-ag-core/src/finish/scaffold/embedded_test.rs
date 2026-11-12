//! The output tree's copies of a Bun app's embedded files (exp217): the
//! files the linked program reads by path, under `embedded/` at the tree
//! root, byte for byte.

use std::fs;

use super::{js_files_under, write_embedded_files};
use crate::unpack::chunk_link::{EMBEDDED_DIR, EmbeddedFile};

#[test]
fn every_embedded_file_is_copied_byte_for_byte() {
    let input = crate::scratch::temp_dir("embedded-in").unwrap();
    let out = crate::scratch::temp_dir("embedded-out").unwrap();
    fs::create_dir_all(input.join("src/w")).unwrap();
    fs::write(input.join("a.bin"), b"\x00\xffzstd").unwrap();
    fs::write(input.join("src/w/b.node"), b"\x7fELF").unwrap();
    let files = [
        EmbeddedFile {
            rel: "a.bin".into(),
            source: input.join("a.bin"),
        },
        EmbeddedFile {
            rel: "src/w/b.node".into(),
            source: input.join("src/w/b.node"),
        },
    ];
    assert_eq!(write_embedded_files(&out, &files).unwrap(), 2);
    assert_eq!(
        fs::read(out.join("embedded/a.bin")).unwrap(),
        b"\x00\xffzstd"
    );
    assert_eq!(
        fs::read(out.join("embedded/src/w/b.node")).unwrap(),
        b"\x7fELF"
    );
    assert_eq!(EMBEDDED_DIR, "embedded");
}

#[test]
fn a_missing_embedded_file_fails_the_write() {
    let out = crate::scratch::temp_dir("embedded-out").unwrap();
    let files = [EmbeddedFile {
        rel: "gone.bin".into(),
        source: out.join("nowhere/gone.bin"),
    }];
    let err = write_embedded_files(&out, &files).unwrap_err();
    assert!(err.contains("gone.bin"), "{err}");
}

#[test]
fn the_tree_walk_skips_the_embedded_folder_at_the_root_only() {
    // An embedded `.js` (a compressed blob Bun's `file` loader kept) is
    // not program code; a source folder that happens to be called
    // `embedded` is.
    let out = crate::scratch::temp_dir("embedded-walk").unwrap();
    for f in ["embedded/mermaid.min.js", "src/embedded/x.js", "index.js"] {
        fs::create_dir_all(out.join(f).parent().unwrap()).unwrap();
        fs::write(out.join(f), "x").unwrap();
    }
    let mut got: Vec<String> = js_files_under(&out)
        .unwrap()
        .iter()
        .map(|p| p.strip_prefix(&out).unwrap().to_string_lossy().into_owned())
        .collect();
    got.sort();
    assert_eq!(got, ["index.js", "src/embedded/x.js"]);
}
