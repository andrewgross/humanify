//! A private scratch directory — the ONE owner of "a temp dir nobody else
//! is using" (`fs.mkdtempSync(os.tmpdir()/<prefix>)`): the reconcile's
//! `diff` inputs and the `discover` verb's unpack dir. Unique per call,
//! not per process, so concurrent callers in one process (the par seam,
//! parallel tests) each get their own.

/// No scratch directory could be made: 100 fresh names in a row were
/// refused (the temp dir is missing, full or not writable).
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("cannot create a {prefix} temp dir")]
pub struct ScratchError {
    pub prefix: String,
}

impl From<ScratchError> for String {
    fn from(e: ScratchError) -> String {
        e.to_string()
    }
}

/// Create and return a new, empty directory under the system temp dir,
/// named `<prefix>-<pid>-<seq>-<nanos>`.
pub fn temp_dir(prefix: &str) -> Result<std::path::PathBuf, ScratchError> {
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
    Err(ScratchError {
        prefix: prefix.to_string(),
    })
}
