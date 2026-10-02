//! The finish driver's manifest gate: `loadBunManifest` reads the vendor
//! manifest BOTH bundler adapters write (the format is shared; the esbuild
//! adapter advertises the runnable split the same way bun's does).

use std::fs;
use std::path::{Path, PathBuf};

use super::load_bun_manifest;
use crate::unpack::bun::bun_manifest_path;

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> TempDir {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "humanify-finish-{tag}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        fs::remove_dir_all(&dir).ok();
        fs::create_dir_all(dir.join("vendor")).unwrap();
        TempDir(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).ok();
    }
}

/// Write a manifest with the given adapter stamp and one factory.
fn manifest(dir: &Path, adapter: &str) {
    fs::write(
        bun_manifest_path(dir),
        format!(
            "{{\"adapter\":\"{adapter}\",\"hashVersion\":3,\"runtimeFile\":\"runtime.js\",\
             \"factories\":[{{\"fileName\":\"vendor/lib_aaaaaaaa.js\",\"name\":\"lib_aaaaaaaa\",\
             \"nameSource\":\"fallback\",\"structuralHash\":\"aaaaaaaaaaaaaaaa\",\
             \"runtimeIdentifier\":\"lib_aaaaaaaa\"}}]}}\n"
        ),
    )
    .unwrap();
}

#[test]
fn the_manifest_gate_accepts_both_bundler_adapters() {
    let bun_dir = TempDir::new("bun");
    manifest(&bun_dir.0, "bun");
    let bun = load_bun_manifest(&bun_dir.0)
        .expect("reads")
        .expect("bun manifest loads");
    assert_eq!(bun.factories.len(), 1);

    // The esbuild adapter writes the same format under its own stamp —
    // the relink machinery is shared (exp075's port).
    let esbuild_dir = TempDir::new("esbuild");
    manifest(&esbuild_dir.0, "esbuild");
    let esbuild = load_bun_manifest(&esbuild_dir.0)
        .expect("reads")
        .expect("the esbuild manifest loads the same way");
    assert_eq!(esbuild.factories.len(), 1);
    assert_eq!(esbuild.runtime_file.as_deref(), Some("runtime.js"));
}

#[test]
fn unknown_adapter_stamps_still_decline() {
    let dir = TempDir::new("other");
    manifest(&dir.0, "webcrack");
    assert!(load_bun_manifest(&dir.0).expect("reads").is_none());
}
