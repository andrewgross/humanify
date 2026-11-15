//! The output tree's copies of a Bun app's embedded files (exp217): the
//! files the linked program reads by path, under `embedded/` at the tree
//! root, byte for byte.
// io: test code reads fixtures and writes scratch trees; it is not the pipeline
#![allow(clippy::disallowed_methods)]

use std::fs;

use super::{js_files_under, write_embedded_files};
use crate::unpack::chunk_link::{EMBEDDED_DIR, EmbeddedFile};

#[test]
fn every_embedded_file_is_copied_byte_for_byte() {
    let input = humanify_ag_io::scratch::temp_dir("embedded-in").unwrap();
    let out = humanify_ag_io::scratch::temp_dir("embedded-out").unwrap();
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
    let mut tree = crate::output_tree::disk_test::tree_on_disk(&out);
    assert_eq!(write_embedded_files(&mut tree, &files), 2);
    crate::output_tree::disk_test::write(&tree);
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
fn an_embedded_file_is_a_copy_the_tree_never_holds() {
    // The shell copies it when it writes the tree (a missing source fails
    // that write: cli `output_tree`).
    let mut tree = crate::output_tree::OutputTree::new(
        std::path::Path::new("out"),
        Box::new(crate::output_tree::NothingOnDisk),
    );
    let files = [EmbeddedFile {
        rel: "w/gone.bin".into(),
        source: "in/gone.bin".into(),
    }];
    assert_eq!(write_embedded_files(&mut tree, &files), 1);
    let placed: Vec<_> = tree.files().collect();
    assert_eq!(
        placed,
        vec![(
            std::path::Path::new("out/embedded/w/gone.bin"),
            &crate::output_tree::TreeFile::CopyOf("in/gone.bin".into())
        )]
    );
}

#[test]
fn the_tree_walk_skips_the_embedded_folder_at_the_root_only() {
    // An embedded `.js` (a compressed blob Bun's `file` loader kept) is
    // not program code; a source folder that happens to be called
    // `embedded` is.
    let out = humanify_ag_io::scratch::temp_dir("embedded-walk").unwrap();
    for f in ["embedded/mermaid.min.js", "src/embedded/x.js", "index.js"] {
        fs::create_dir_all(out.join(f).parent().unwrap()).unwrap();
        fs::write(out.join(f), "x").unwrap();
    }
    let mut got: Vec<String> = js_files_under(&crate::output_tree::disk_test::tree_on_disk(&out))
        .unwrap()
        .iter()
        .map(|p| p.strip_prefix(&out).unwrap().to_string_lossy().into_owned())
        .collect();
    got.sort();
    assert_eq!(got, ["index.js", "src/embedded/x.js"]);
}
