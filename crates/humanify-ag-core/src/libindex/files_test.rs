//! The index file seam: the in-memory set answers like a directory does
//! (a missing name is `NotFound`), a reader reads by position, and a table
//! opened through a reader answers exactly as the same bytes in memory.
// io: test code reads fixtures and writes scratch trees; it is not the pipeline
#![allow(clippy::disallowed_methods)]

use std::collections::BTreeMap;
use std::io::{BufRead, ErrorKind};
use std::path::Path;

use super::files::{IndexFiles, MemoryIndexFiles, Sequential};

/// The files under `dir` as they are now, in memory: the tests write an
/// index directory, then load it through the seam the shell would fill.
pub(crate) fn dir_files(dir: &Path) -> Box<dyn IndexFiles> {
    let files = std::fs::read_dir(dir)
        .expect("the index dir reads")
        .map(|entry| {
            let path = entry.expect("an entry").path();
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            (name, std::fs::read(&path).expect("an index file reads"))
        })
        .collect();
    Box::new(MemoryIndexFiles(files))
}

fn set(files: &[(&str, &[u8])]) -> MemoryIndexFiles {
    MemoryIndexFiles(
        files
            .iter()
            .map(|(name, bytes)| (name.to_string(), bytes.to_vec()))
            .collect::<BTreeMap<_, _>>(),
    )
}

#[test]
fn a_name_the_set_lacks_is_not_found_like_a_missing_file() {
    let files = set(&[("manifest.json", b"{}")]);
    assert_eq!(files.read_text("manifest.json").unwrap(), "{}");
    assert_eq!(
        files.read_text("packages.json").unwrap_err().kind(),
        ErrorKind::NotFound
    );
    assert_eq!(
        files.open("units.bin").err().map(|e| e.kind()),
        Some(ErrorKind::NotFound)
    );
}

#[test]
fn a_reader_reads_by_position_and_refuses_past_its_end() {
    let reader = set(&[("t", b"abcdef")]).open("t").unwrap();
    assert_eq!(reader.size(), 6);
    let mut buf = [0u8; 3];
    reader.read_exact_at(2, &mut buf).unwrap();
    assert_eq!(&buf, b"cde");
    assert_eq!(
        reader.read_exact_at(4, &mut buf).unwrap_err().kind(),
        ErrorKind::UnexpectedEof
    );
}

#[test]
fn a_sequential_reader_streams_every_line() {
    let reader = set(&[("rows", b"one\ntwo\n\nthree")]).open("rows").unwrap();
    let lines: Vec<String> = std::io::BufReader::with_capacity(2, Sequential::new(reader))
        .lines()
        .map(Result::unwrap)
        .collect();
    assert_eq!(lines, ["one", "two", "", "three"]);
}

#[test]
fn a_table_read_through_pages_answers_as_the_same_bytes_in_memory() {
    use crate::libindex::format::LiteralRow;
    use crate::libindex::table::{LiteralTable, Source};
    // enough rows that the table spans several 4 KiB pages
    let rows: Vec<LiteralRow> = (0..400)
        .map(|i| LiteralRow {
            owner: format!("pkg{:03}", i % 7),
            file: format!("f{i}.js"),
            lits: (0..6).map(|j| format!("S:lit{i}-{j}")).collect(),
            versions: Vec::new(),
        })
        .collect();
    let mut packages: Vec<String> = rows.iter().map(|r| r.owner.clone()).collect();
    packages.sort();
    packages.dedup();
    let bytes = LiteralTable::encode(&rows, &packages);
    assert!(bytes.len() > 3 * 4096, "the table spans pages");
    let memory = LiteralTable::open(Source::Memory(bytes.clone())).unwrap();
    let paged = LiteralTable::open(Source::paged(Box::new(bytes))).unwrap();
    for f in [0u32, 1, 199, 399] {
        assert_eq!(paged.file_owner(f), memory.file_owner(f));
        assert_eq!(paged.file_literals(f), memory.file_literals(f));
    }
    assert_eq!(paged.resolve("S:lit250-3"), memory.resolve("S:lit250-3"));
    assert!(paged.resolve("S:lit250-3").is_some());
}
