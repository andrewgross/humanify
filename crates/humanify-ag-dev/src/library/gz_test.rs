//! `testdata/python-written.json.gz` was written by Python's gzip
//! (lib096 `write_gz_json`'s exact call) from `python-written.json`.

use super::*;

#[test]
fn python_s_gzip_bytes_are_reproduced() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/library/testdata");
    let raw = std::fs::read(dir.join("python-written.json")).unwrap();
    let want = std::fs::read(dir.join("python-written.json.gz")).unwrap();
    assert_eq!(
        gzip_bytes(&dir.join("python-written.json.gz"), &raw).unwrap(),
        want
    );
    assert_eq!(read_gz(&dir.join("python-written.json.gz")).unwrap(), raw);
}
