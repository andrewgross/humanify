//! Reading a whole folder into memory: every file under the root by its
//! `/`-separated relative path, the skipped folders left out, and a
//! folder that cannot be read reported with its path.

use super::read_tree;
use crate::scratch::temp_dir;

fn write(root: &std::path::Path, rel: &str, bytes: &[u8]) {
    let path = root.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

#[test]
fn every_file_is_read_by_its_relative_path() {
    let root = temp_dir("humanify-io-tree").unwrap();
    write(&root, "index.js", b"a");
    write(&root, "src/app/main.js", b"b");
    write(&root, "vendor/@scope/pkg.js", b"\xff\xfe");
    let files = read_tree(&root, &|_| true).unwrap();
    let paths: Vec<&str> = files.keys().map(String::as_str).collect();
    assert_eq!(
        paths,
        ["index.js", "src/app/main.js", "vendor/@scope/pkg.js"]
    );
    assert_eq!(files["vendor/@scope/pkg.js"], b"\xff\xfe");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_folder_the_filter_refuses_is_not_read() {
    let root = temp_dir("humanify-io-tree").unwrap();
    write(&root, "src/a.js", b"a");
    write(&root, "node_modules/x/index.js", b"x");
    write(&root, "src/node_modules/y.js", b"y");
    let files = read_tree(&root, &|name| name != "node_modules").unwrap();
    let paths: Vec<&str> = files.keys().map(String::as_str).collect();
    assert_eq!(paths, ["src/a.js"]);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_missing_root_is_an_error_naming_it() {
    let root = temp_dir("humanify-io-tree").unwrap().join("absent");
    let err = read_tree(&root, &|_| true).expect_err("no such folder");
    assert_eq!(err.path, root);
    assert!(
        err.to_string()
            .starts_with(&format!("read {}: ", root.display()))
    );
}
