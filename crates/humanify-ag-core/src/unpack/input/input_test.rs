//! The input-layout registry: which layout reads a run's input file.

use std::path::PathBuf;

use super::{INPUT_LAYOUTS, InputLayout, choose_input_layout, read_input};

fn dir_with(files: &[(&str, &str)]) -> PathBuf {
    let dir = crate::scratch::temp_dir("input-layout").unwrap();
    for (name, text) in files {
        std::fs::write(dir.join(name), text).unwrap();
    }
    dir.join(files[0].0)
}

#[test]
fn single_file_is_registered_last_and_supports_everything() {
    assert_eq!(INPUT_LAYOUTS.last(), Some(&InputLayout::SingleFile));
    let entry = dir_with(&[("index.js", "anything at all")]);
    assert!(InputLayout::SingleFile.supports(&entry, "anything at all"));
    let names: Vec<&str> = INPUT_LAYOUTS.iter().map(|l| l.name()).collect();
    assert_eq!(names, ["bun-chunks", "single-file"]);
}

#[test]
fn a_bun_code_split_entry_takes_the_bun_chunks_layout() {
    let entry_text = "// @bun\nimport{a as b}from\"/$bunfs/root/chunk-a.js\";b();\n";
    let entry = dir_with(&[
        ("index.js", entry_text),
        ("chunk-a.js", "// @bun\nfunction f(){}export{f as a};\n"),
    ]);
    assert_eq!(
        choose_input_layout(&entry, entry_text),
        InputLayout::BunChunks
    );
    let bundle = read_input(&entry).unwrap();
    assert_eq!(bundle.layout, InputLayout::BunChunks);
    assert!(bundle.link.is_some());
}

#[test]
fn anything_else_falls_back_to_single_file_byte_for_byte() {
    for text in [
        "// @bun @bun-cjs\n(function(exports, require, module, __filename, __dirname) {})\n",
        "// @bun\nimport\"/$bunfs/root/chunk-missing.js\";\n",
        "var x = 1;\n",
    ] {
        let entry = dir_with(&[("index.js", text)]);
        assert_eq!(choose_input_layout(&entry, text), InputLayout::SingleFile);
        let bundle = read_input(&entry).unwrap();
        assert_eq!(bundle.layout, InputLayout::SingleFile);
        assert!(bundle.link.is_none());
        assert_eq!(bundle.text, text);
    }
}
