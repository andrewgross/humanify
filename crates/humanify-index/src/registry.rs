//! The registry snapshot (docs/design/library-index.md §2 step 1): each
//! package's registry document (`https://registry.npmjs.org/<name>` —
//! what `npm install` itself resolves against) fetched ONCE by curl into
//! `<dir>/<pkg>.json` with the fetch time, and read from there
//! afterwards. The snapshot is an INPUT: the manifest records each
//! file's sha256 and the fetch-time range.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::asof::{Packument, normalize_time};
use crate::shard::hex_sha256;

pub fn packument_path(dir: &Path, name: &str) -> PathBuf {
    dir.join(format!("{}.json", name.replace('/', "__")))
}

/// The recorded packument of `name` and its file's sha256; fetched first
/// unless `offline` (then a missing snapshot is an error).
pub fn load(dir: &Path, name: &str, offline: bool) -> Result<(Packument, String), String> {
    let path = packument_path(dir, name);
    if !path.exists() {
        if offline {
            return Err(format!(
                "{name}: no recorded packument at {} (--offline)",
                path.display()
            ));
        }
        let p = fetch(name)?;
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let mut bytes = serde_json::to_vec_pretty(&p).map_err(|e| e.to_string())?;
        bytes.push(b'\n');
        std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let text = String::from_utf8_lossy(&bytes);
    Ok((Packument::from_json(&text)?, hex_sha256(&bytes)))
}

/// One registry document's curl (seconds; the deadline is for a registry
/// that never answers).
const FETCH_DEADLINE: std::time::Duration = std::time::Duration::from_secs(5 * 60);

/// The registry base every packument comes from (the public npm registry).
pub const REGISTRY_BASE: &str = "https://registry.npmjs.org";

/// The error text a NOT-FOUND fetch carries; `fetch` matches it to
/// return an ABSENT packument (absence is data — the planner's own
/// decision points decide what it means); any other error fails loud
/// (after the one retry).
pub const NOT_FOUND: &str = "registry says 404 (E404)";

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
    mut op: impl FnMut() -> Result<T, String>,
    site: &str,
) -> Result<T, String> {
    match op() {
        Ok(t) => Ok(t),
        Err(first) => match op() {
            Ok(t) => Ok(t),
            Err(second) => Err(format!("{site}: {first}; retry: {second}")),
        },
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
fn fetch(name: &str) -> Result<Packument, String> {
    let url = registry_url(name);
    let (code, body) = with_retry(
        || {
            let out = humanify_core::subprocess::run(
                Command::new("curl").args(["-sS", "--compressed", "-w", "\n%{http_code}", &url]),
                None,
                FETCH_DEADLINE,
            )
            .map_err(|e| format!("curl {url}: {e}"))?;
            if !out.status.success() {
                return Err(format!(
                    "curl {url}: {}",
                    String::from_utf8_lossy(&out.stderr).trim()
                ));
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
        other => return Err(format!("curl {url}: registry answered HTTP {other}")),
    }
    let doc: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("curl {url}: {e}"))?;
    match Packument::from_doc(&doc, Some(now_utc())) {
        Ok(p) => Ok(p),
        // the tombstone is absence (its error carries the marker)
        Err(e) if e.contains(NOT_FOUND) => Ok(absent_packument(name)),
        Err(e) => Err(e),
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
    let (days, rem) = ((secs / 86_400) as i64, secs % 86_400);
    // civil-from-days (Howard Hinnant)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    let t = format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    );
    normalize_time(&t).unwrap_or(t)
}
