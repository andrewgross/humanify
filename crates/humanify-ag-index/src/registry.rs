//! The registry snapshot (docs/design/library-index.md §2 step 1): each
//! package's registry document (`https://registry.npmjs.org/<name>` —
//! what `npm install` itself resolves against) fetched ONCE by curl into
//! `<dir>/<pkg>.json` with the fetch time, and read from there
//! afterwards. The snapshot is an INPUT: the manifest records each
//! file's sha256 and the fetch-time range.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::asof::{Packument, PackumentError};
use crate::error::{FileError, at};

/// Why a package's registry document could not be had.
#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("{name}: no recorded packument at {} (--offline)", .path.display())]
    Offline { name: String, path: PathBuf },
    #[error(
        "{count} packages are missing from the registry snapshot (--offline), \
         e.g. {first}; drop --offline or take the snapshot first"
    )]
    OfflineWave { count: usize, first: String },
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Encode(serde_json::Error),
    #[error(transparent)]
    File(#[from] FileError),
    #[error(transparent)]
    Packument(#[from] PackumentError),
    #[error("curl {url}: {why}")]
    Curl {
        url: String,
        why: humanify_ag_io::subprocess::SubprocessError,
    },
    #[error("curl {url}: {stderr}")]
    CurlFailed { url: String, stderr: String },
    #[error("curl {url}: registry answered HTTP {code}")]
    Http { url: String, code: String },
    #[error("curl {url}: {source}")]
    Doc {
        url: String,
        source: serde_json::Error,
    },
    #[error("{site}: {first}; retry: {second}")]
    Retried {
        site: String,
        first: Box<RegistryError>,
        second: Box<RegistryError>,
    },
}
use crate::shard::hex_sha256;

pub fn packument_path(dir: &Path, name: &str) -> PathBuf {
    dir.join(format!("{}.json", name.replace('/', "__")))
}

/// The recorded packument of `name` and its file's sha256; fetched first
/// unless `offline` (then a missing snapshot is an error).
pub fn load(dir: &Path, name: &str, offline: bool) -> Result<(Packument, String), RegistryError> {
    let path = packument_path(dir, name);
    if !path.exists() {
        if offline {
            return Err(RegistryError::Offline {
                name: name.to_string(),
                path,
            });
        }
        let p = fetch(name)?;
        std::fs::create_dir_all(dir)?;
        let mut bytes = serde_json::to_vec_pretty(&p).map_err(RegistryError::Encode)?;
        bytes.push(b'\n');
        std::fs::write(&path, bytes).map_err(at(&path))?;
    }
    let bytes = std::fs::read(&path).map_err(at(&path))?;
    let text = String::from_utf8_lossy(&bytes);
    Ok((Packument::from_json(&text)?, hex_sha256(&bytes)))
}

/// One registry document's curl (seconds; the deadline is for a registry
/// that never answers).
const FETCH_DEADLINE: std::time::Duration = std::time::Duration::from_secs(5 * 60);

/// The registry base every packument comes from (the public npm registry).
pub const REGISTRY_BASE: &str = "https://registry.npmjs.org";

/// The registry URL of a package name: a scoped name's `/` is escaped,
/// because the registry routes `/@scope/name` as ONE path segment.
pub fn registry_url(name: &str) -> String {
    if let Some((scope, pkg)) = name.strip_prefix('@').and_then(|r| r.split_once('/')) {
        return format!("{REGISTRY_BASE}/@{scope}%2F{pkg}");
    }
    format!("{REGISTRY_BASE}/{name}")
}

/// The ONE retry layer a fetch gets: a transient registry error (a
/// network blip in a wave that runs for hours) is retried once; a
/// persistent one fails after the second attempt, naming the site.
pub(crate) fn with_retry<T>(
    mut op: impl FnMut() -> Result<T, RegistryError>,
    site: &str,
) -> Result<T, RegistryError> {
    match op() {
        Ok(t) => Ok(t),
        Err(first) => op().map_err(|second| RegistryError::Retried {
            site: site.to_string(),
            first: Box::new(first),
            second: Box::new(second),
        }),
    }
}

/// One package's registry document, fetched with curl (a subprocess —
/// the crate has no HTTP client; a bare one has no deadline covering the
/// whole exchange, clippy.toml's disallowed-methods note). The FULL
/// document, not `npm view`: npm view's multi-version output omits
/// `dist.unpackedSize` and `os`/`cpu` and drops versions that define
/// none of the fields asked for, which left the spec's platform and
/// size rules dead on the first default-spec fetch (exp184). A 404 or
/// an unpublished tombstone reads as an ABSENT packument (recorded in
/// the snapshot; the planner skips it); a transport failure or another
/// HTTP error is retried once and then fails loud.
fn fetch(name: &str) -> Result<Packument, RegistryError> {
    let url = registry_url(name);
    let (code, body) = with_retry(
        || {
            let out = humanify_ag_io::subprocess::run(
                Command::new("curl").args(["-sS", "--compressed", "-w", "\n%{http_code}", &url]),
                None,
                FETCH_DEADLINE,
            )
            .map_err(|why| RegistryError::Curl {
                url: url.clone(),
                why,
            })?;
            if !out.status.success() {
                return Err(RegistryError::CurlFailed {
                    url: url.clone(),
                    stderr: String::from_utf8_lossy(&out.stderr).trim().to_string(),
                });
            }
            let text = String::from_utf8_lossy(&out.stdout);
            match text.rsplit_once('\n') {
                Some((b, c)) => Ok((c.trim().to_string(), b.trim_end().to_string())),
                None => Ok(("000".to_string(), text.trim().to_string())),
            }
        },
        &url,
    )?;
    match code.as_str() {
        "200" => {}
        // absence is data, not an error: an absent packument is WRITTEN
        // into the snapshot, so the offline replay skips it too
        "404" => return Ok(absent_packument(name)),
        _ => return Err(RegistryError::Http { url, code }),
    }
    let doc: serde_json::Value =
        serde_json::from_str(&body).map_err(|source| RegistryError::Doc { url, source })?;
    match Packument::from_doc(&doc, Some(now_utc())) {
        Ok(p) => Ok(p),
        // the tombstone is absence
        Err(PackumentError::Unpublished(_)) => Ok(absent_packument(name)),
        Err(e) => Err(e.into()),
    }
}

/// The packument recorded for a package the registry does not know.
fn absent_packument(name: &str) -> Packument {
    Packument {
        name: name.to_string(),
        fetched_at: Some(now_utc()),
        time: BTreeMap::new(),
        versions: Vec::new(),
        deps: BTreeMap::new(),
        deps_fetched: true,
        absent: true,
    }
}

/// The current UTC time, normalised (the snapshot's fetch time — an
/// input that is recorded, never read back into a decision).
fn now_utc() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    humanify_ag_model::time::iso_from_unix_ms(u128::from(secs) * 1000)
}
