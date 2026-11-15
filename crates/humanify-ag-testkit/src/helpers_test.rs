use crate::helpers::{ScratchDir, strings};

#[test]
fn strings_owns_each_str() {
    assert_eq!(
        strings(&["a", "bc"]),
        vec!["a".to_string(), "bc".to_string()]
    );
    assert!(strings(&[]).is_empty());
}

#[test]
fn a_scratch_dir_exists_until_dropped() {
    let path = {
        let dir = ScratchDir::new("helpers-test");
        assert!(dir.path().is_dir());
        std::fs::write(dir.path().join("f"), "x").unwrap();
        dir.path().to_path_buf()
    };
    assert!(!path.exists());
}
