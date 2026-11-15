//! Small test helpers, one copy for every crate.

/// Each `&str` as an owned `String`.
pub fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

/// A fresh, empty temp directory, removed when dropped. The name carries
/// the tag, the process and the thread, so parallel tests never share one.
pub struct ScratchDir(std::path::PathBuf);

impl ScratchDir {
    pub fn new(tag: &str) -> ScratchDir {
        let dir = std::env::temp_dir().join(format!(
            "humanify-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a scratch directory");
        ScratchDir(dir)
    }

    pub fn path(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
