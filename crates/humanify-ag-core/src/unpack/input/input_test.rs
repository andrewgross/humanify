//! The input-layout registry: which layout reads a run's input file.
// io: test code reads fixtures and writes scratch trees; it is not the pipeline
#![allow(clippy::disallowed_methods)]

use crate::output_tree::disk_test::Disk;
use std::path::PathBuf;

use super::{INPUT_LAYOUTS, InputLayout, choose_input_layout, read_input};

fn dir_with(files: &[(&str, &str)]) -> PathBuf {
    let dir = humanify_ag_io::scratch::temp_dir("input-layout").unwrap();
    for (name, text) in files {
        std::fs::write(dir.join(name), text).unwrap();
    }
    dir.join(files[0].0)
}

#[test]
fn single_file_is_registered_last_and_supports_everything() {
    assert_eq!(INPUT_LAYOUTS.last(), Some(&InputLayout::SingleFile));
    let entry = dir_with(&[("index.js", "anything at all")]);
    assert!(InputLayout::SingleFile.supports(&entry, "anything at all", &Disk));
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
        choose_input_layout(&entry, entry_text, &Disk),
        InputLayout::BunChunks
    );
    let bundle = read_input(&entry, &Disk).unwrap();
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
        assert_eq!(
            choose_input_layout(&entry, text, &Disk),
            InputLayout::SingleFile
        );
        let bundle = read_input(&entry, &Disk).unwrap();
        assert_eq!(bundle.layout, InputLayout::SingleFile);
        assert!(bundle.link.is_none());
        assert_eq!(bundle.text, text);
    }
}

/// Files held in memory: the input reader opens nothing itself, it asks
/// the folder reader it is handed (the shell's disk in a run).
struct MemoryDisk(Vec<(&'static str, &'static str)>);

impl crate::output_tree::FolderReader for MemoryDisk {
    fn read(&self, path: &std::path::Path) -> std::io::Result<Vec<u8>> {
        self.0
            .iter()
            .find(|(p, _)| std::path::Path::new(p) == path)
            .map(|(_, t)| t.as_bytes().to_vec())
            .ok_or_else(|| std::io::ErrorKind::NotFound.into())
    }

    fn list(&self, dir: &std::path::Path) -> std::io::Result<Vec<crate::output_tree::FolderEntry>> {
        Ok(self
            .0
            .iter()
            .map(|(p, _)| std::path::Path::new(p))
            .filter(|p| p.parent() == Some(dir))
            .map(|p| crate::output_tree::FolderEntry {
                name: p.file_name().unwrap().to_string_lossy().into_owned(),
                is_dir: false,
                leads_to_dir: false,
            })
            .collect())
    }
}

#[test]
fn the_input_and_its_chunks_are_read_through_the_folder_reader() {
    let disk = MemoryDisk(vec![
        (
            "/nowhere/index.js",
            "// @bun\nimport{a as b}from\"/$bunfs/root/chunk-a.js\";b();\n",
        ),
        (
            "/nowhere/chunk-a.js",
            "// @bun\nfunction f(){}export{f as a};\n",
        ),
        ("/nowhere/stray.js", "// @bun\n"),
    ]);
    let bundle = read_input(std::path::Path::new("/nowhere/index.js"), &disk).unwrap();
    assert_eq!(bundle.layout, InputLayout::BunChunks);
    let link = bundle.link.expect("linked");
    assert_eq!(link.chunks_linked, 2);
    assert_eq!(link.js_files_unreached, 1);
    let missing = read_input(std::path::Path::new("/nowhere/none.js"), &disk).unwrap_err();
    assert!(
        missing
            .to_string()
            .starts_with("cannot read /nowhere/none.js: "),
        "{missing}"
    );
}
