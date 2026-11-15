//! A planted crate tree for the census tests: a fresh temp root, removed
//! on drop.

use std::path::PathBuf;

pub struct Planted(pub PathBuf);

impl Planted {
    pub fn new(prefix: &str) -> Planted {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "{prefix}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Planted(dir)
    }

    pub fn write(&self, rel: &str, text: &str) {
        let p = self.0.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
}

impl Drop for Planted {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
