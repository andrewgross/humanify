//! A private scratch directory — the ONE owner of "a temp dir nobody else
//! is using" (`fs.mkdtempSync(os.tmpdir()/<prefix>)`): the reconcile's
//! `diff` inputs and the `discover` verb's unpack dir. Unique per call,
//! not per process, so concurrent callers in one process (the par seam,
//! parallel tests) never share one.

/// Create and return a new, empty directory under the system temp dir,
/// named `<prefix>-<pid>-<seq>-<nanos>`.
pub fn temp_dir(prefix: &str) -> Result<std::path::PathBuf, String> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let base = std::env::temp_dir();
    for _ in 0..100 {
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        let dir = base.join(format!("{prefix}-{}-{n}-{nanos}", std::process::id()));
        if std::fs::create_dir(&dir).is_ok() {
            return Ok(dir);
        }
    }
    Err(format!("cannot create a {prefix} temp dir"))
}
