//! The registry snapshot (docs/design/library-index.md §2 step 1): each
//! package's `time` map and installable `versions`, fetched ONCE with
//! `npm view <pkg> time versions --json` into `<dir>/<pkg>.json` with the
//! fetch time, and read from there afterwards. The snapshot is an INPUT:
//! the manifest records each file's sha256 and the fetch-time range.

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

fn fetch(name: &str) -> Result<Packument, String> {
    let out = Command::new("npm")
        .args(["view", name, "time", "versions", "--json"])
        .output()
        .map_err(|e| format!("npm view {name}: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "npm view {name}: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let v: serde_json::Value =
        serde_json::from_slice(&out.stdout).map_err(|e| format!("npm view {name}: {e}"))?;
    let time = v
        .get("time")
        .and_then(|t| t.as_object())
        .map(|m| {
            m.iter()
                .filter_map(|(k, t)| Some((k.clone(), t.as_str()?.to_string())))
                .collect()
        })
        .unwrap_or_default();
    // one version prints as a string, several as an array
    let versions = match v.get("versions") {
        Some(serde_json::Value::String(s)) => vec![s.clone()],
        Some(serde_json::Value::Array(a)) => a
            .iter()
            .filter_map(|x| x.as_str().map(str::to_string))
            .collect(),
        _ => Vec::new(),
    };
    Ok(Packument {
        name: name.to_string(),
        fetched_at: Some(now_utc()),
        time,
        versions,
    })
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
