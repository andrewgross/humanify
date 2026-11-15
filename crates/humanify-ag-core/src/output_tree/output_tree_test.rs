//! The output tree's overlay: this run's files over what is on disk, less
//! what it removed — the view the finish reads.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::{FolderEntry, FolderReader, Lookup, NothingOnDisk, OutputTree, TreeFile};

/// A disk held in memory: files by path; folders are implied by them.
pub(crate) struct FakeDisk(pub BTreeMap<PathBuf, Vec<u8>>);

impl FakeDisk {
    pub(crate) fn with(files: &[(&str, &str)]) -> FakeDisk {
        FakeDisk(
            files
                .iter()
                .map(|(p, t)| (PathBuf::from(p), t.as_bytes().to_vec()))
                .collect(),
        )
    }
}

impl FolderReader for FakeDisk {
    fn read(&self, path: &Path) -> std::io::Result<Vec<u8>> {
        self.0
            .get(path)
            .cloned()
            .ok_or_else(|| std::io::ErrorKind::NotFound.into())
    }

    fn list(&self, dir: &Path) -> std::io::Result<Vec<FolderEntry>> {
        let mut out: BTreeMap<String, FolderEntry> = BTreeMap::new();
        for path in self.0.keys() {
            let Ok(rest) = path.strip_prefix(dir) else {
                continue;
            };
            let mut parts = rest.components();
            let Some(first) = parts.next() else { continue };
            let name = first.as_os_str().to_string_lossy().into_owned();
            let is_dir = parts.next().is_some();
            out.entry(name.clone()).or_insert(FolderEntry {
                name,
                is_dir,
                leads_to_dir: is_dir,
            });
        }
        if out.is_empty() {
            return Err(std::io::ErrorKind::NotFound.into());
        }
        Ok(out.into_values().collect())
    }
}

fn names(entries: &[FolderEntry]) -> Vec<(String, bool)> {
    entries.iter().map(|e| (e.name.clone(), e.is_dir)).collect()
}

#[test]
fn a_written_file_reads_back_and_shadows_the_disk() {
    let disk = FakeDisk::with(&[("out/a.js", "old")]);
    let mut tree = OutputTree::new(Path::new("out"), Box::new(disk));
    assert_eq!(tree.read_lossy(Path::new("out/a.js")).unwrap(), "old");
    tree.put_rel("a.js", "new");
    assert_eq!(tree.read_lossy(Path::new("out/a.js")).unwrap(), "new");
    assert!(matches!(
        tree.lookup(Path::new("out/a.js")),
        Lookup::Written(TreeFile::Bytes(_))
    ));
}

#[test]
fn a_removed_file_reads_as_not_found_and_writing_it_again_restores_it() {
    let disk = FakeDisk::with(&[("out/a.js", "old")]);
    let mut tree = OutputTree::new(Path::new("out"), Box::new(disk));
    tree.put_rel("a.js", "new");
    tree.remove(Path::new("out/a.js"));
    let err = tree.read(Path::new("out/a.js")).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
    assert_eq!(
        tree.removed().collect::<Vec<_>>(),
        vec![Path::new("out/a.js")]
    );
    tree.put_rel("a.js", "again");
    assert_eq!(tree.removed().count(), 0);
    assert_eq!(tree.read_lossy(Path::new("out/a.js")).unwrap(), "again");
}

#[test]
fn a_folder_lists_the_disk_and_the_tree_less_the_removed_files_sorted() {
    let disk = FakeDisk::with(&[
        ("out/vendor/stale.js", "s"),
        ("out/vendor/kept.js", "k"),
        ("out/z.js", "z"),
    ]);
    let mut tree = OutputTree::new(Path::new("out"), Box::new(disk));
    tree.put_rel("vendor/new.js", "n");
    tree.put_rel("vendor/@scope/pkg/index.js", "p");
    tree.remove(Path::new("out/vendor/kept.js"));
    let vendor = tree.list(Path::new("out/vendor")).unwrap();
    assert_eq!(
        names(&vendor),
        vec![
            ("@scope".to_string(), true),
            ("new.js".to_string(), false),
            ("stale.js".to_string(), false),
        ]
    );
    assert_eq!(
        names(&tree.list(Path::new("out")).unwrap()),
        vec![("vendor".to_string(), true), ("z.js".to_string(), false)]
    );
}

#[test]
fn a_folder_only_the_tree_places_lists_without_the_disk() {
    let mut tree = OutputTree::new(Path::new("out"), Box::new(NothingOnDisk));
    // The root is always the tree's (the shell creates it when it writes).
    assert!(tree.list(Path::new("out")).unwrap().is_empty());
    tree.put_rel("src/a.js", "a");
    assert_eq!(
        names(&tree.list(Path::new("out/src")).unwrap()),
        vec![("a.js".to_string(), false)]
    );
    // A folder neither the disk nor the tree holds is the disk's error.
    let err = tree.list(Path::new("out/vendor")).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
    // A sibling whose name extends the folder's is not inside it.
    tree.put_rel("srcx/b.js", "b");
    assert_eq!(tree.list(Path::new("out/src")).unwrap().len(), 1);
}

#[test]
fn strict_text_refuses_invalid_utf8_in_the_standard_wording() {
    let mut tree = OutputTree::new(Path::new("out"), Box::new(NothingOnDisk));
    tree.put_rel("bad.js", vec![b'a', 0xff]);
    let err = tree.read_text(Path::new("out/bad.js")).unwrap_err();
    assert!(
        err.to_string()
            .starts_with("read out/bad.js: stream did not contain valid UTF-8"),
        "{err}"
    );
    assert_eq!(
        tree.read_lossy(Path::new("out/bad.js")).unwrap(),
        "a\u{fffd}"
    );
}

#[test]
fn a_copy_reads_its_source_from_disk_and_is_never_held() {
    let disk = FakeDisk::with(&[("in/asset.bin", "bytes")]);
    let mut tree = OutputTree::new(Path::new("out"), Box::new(disk));
    tree.copy(
        PathBuf::from("out/embedded/asset.bin"),
        PathBuf::from("in/asset.bin"),
    );
    assert_eq!(
        tree.read_lossy(Path::new("out/embedded/asset.bin"))
            .unwrap(),
        "bytes"
    );
    assert!(matches!(
        tree.lookup(Path::new("out/embedded/asset.bin")),
        Lookup::Written(TreeFile::CopyOf(_))
    ));
}
