//! Which npm file a bundled module came from — read from the bundler's
//! sourcemap (the lab's `label.mjs` rule): a module's span belongs to the
//! source file owning the majority of the mapping segments inside it, and
//! only when that file owns nearly all of them (a mixed span is nobody's).
//! The file's nearest `package.json` under the LAST `node_modules/` names
//! the owner package and version. Builder-only: a bundle the pipeline
//! processes has no sourcemap.

use std::collections::{BTreeMap, HashMap};
use std::path::{Component, Path, PathBuf};

/// A decoded sourcemap: its sources and every segment that names one, as
/// (generated line, generated UTF-16 column, source index).
#[derive(Clone, Debug, Default)]
pub struct SourceMap {
    pub sources: Vec<String>,
    pub segments: Vec<(u32, u32, u32)>,
}

fn vlq_digit(c: u8) -> Option<i64> {
    let v = match c {
        b'A'..=b'Z' => c - b'A',
        b'a'..=b'z' => c - b'a' + 26,
        b'0'..=b'9' => c - b'0' + 52,
        b'+' => 62,
        b'/' => 63,
        _ => return None,
    };
    Some(i64::from(v))
}

/// The fields of one segment.
fn decode_segment(seg: &[u8]) -> Result<Vec<i64>, String> {
    let mut out = Vec::new();
    let (mut value, mut shift) = (0i64, 0u32);
    for &c in seg {
        let d = vlq_digit(c).ok_or_else(|| format!("bad VLQ character {:?}", c as char))?;
        value += (d & 31) << shift;
        if d & 32 != 0 {
            shift += 5;
            continue;
        }
        out.push(if value & 1 == 1 {
            -(value >> 1)
        } else {
            value >> 1
        });
        value = 0;
        shift = 0;
    }
    if shift != 0 {
        return Err("truncated VLQ".into());
    }
    Ok(out)
}

/// Source map v3 `mappings` → (line, column, source) for every segment
/// with a source.
pub fn decode_mappings(mappings: &str) -> Result<Vec<(u32, u32, u32)>, String> {
    let mut out = Vec::new();
    let mut src: i64 = 0;
    for (line, text) in mappings.split(';').enumerate() {
        let mut col: i64 = 0;
        for seg in text.split(',').filter(|s| !s.is_empty()) {
            let f = decode_segment(seg.as_bytes())?;
            col += f[0];
            if f.len() >= 4 {
                src += f[1];
                out.push((line as u32, col as u32, src as u32));
            }
        }
    }
    Ok(out)
}

/// (line, UTF-16 column) positions → byte offsets into `text` (None past a
/// line's end or past the last line). Linear: positions are visited in
/// (line, column) order and each line is walked once — a minified bundle
/// is one line with a segment every few characters.
pub fn utf16_to_byte_offsets(text: &str, positions: &[(u32, u32)]) -> Vec<Option<usize>> {
    let mut starts = vec![0usize];
    starts.extend(text.match_indices('\n').map(|(i, _)| i + 1));
    let mut order: Vec<usize> = (0..positions.len()).collect();
    order.sort_unstable_by_key(|&i| positions[i]);
    let mut out = vec![None; positions.len()];
    // cursor: (line, byte offset, UTF-16 units from the line start)
    let mut cur: Option<(u32, usize, u32)> = None;
    for i in order {
        let (line, col) = positions[i];
        let Some(&start) = starts.get(line as usize) else {
            continue;
        };
        let end = starts.get(line as usize + 1).map_or(text.len(), |e| e - 1);
        let (mut at, mut units) = match cur {
            Some((l, at, u)) if l == line => (at, u),
            _ => (start, 0),
        };
        while units < col && at < end {
            let c = text[at..].chars().next().expect("char boundary");
            units += c.len_utf16() as u32;
            at += c.len_utf8();
        }
        cur = Some((line, at, units));
        out[i] = (units == col).then_some(at);
    }
    out
}

impl SourceMap {
    /// Parse a `.map` file's JSON (`sourceRoot` joined onto each source).
    pub fn parse(json: &str) -> Result<SourceMap, String> {
        let v: serde_json::Value = serde_json::from_str(json).map_err(|e| format!("map: {e}"))?;
        let root = v.get("sourceRoot").and_then(|r| r.as_str()).unwrap_or("");
        let sources = v
            .get("sources")
            .and_then(|s| s.as_array())
            .ok_or("map: no sources")?
            .iter()
            .map(|s| {
                let s = s.as_str().unwrap_or("");
                if root.is_empty() {
                    s.to_string()
                } else {
                    format!("{}/{s}", root.trim_end_matches('/'))
                }
            })
            .collect();
        let mappings = v.get("mappings").and_then(|m| m.as_str()).unwrap_or("");
        Ok(SourceMap {
            sources,
            segments: decode_mappings(mappings)?,
        })
    }

    /// For each byte span of `code`: how many mapping segments inside it
    /// name each source (source index, count), by source index.
    pub fn tally(&self, code: &str, spans: &[(u32, u32)]) -> Vec<Vec<(u32, usize)>> {
        let positions: Vec<(u32, u32)> = self.segments.iter().map(|&(l, c, _)| (l, c)).collect();
        let mut at: Vec<(usize, u32)> = utf16_to_byte_offsets(code, &positions)
            .into_iter()
            .zip(&self.segments)
            .filter_map(|(o, &(_, _, s))| o.map(|o| (o, s)))
            .collect();
        at.sort_unstable();
        spans
            .iter()
            .map(|&(s, e)| {
                let lo = at.partition_point(|&(o, _)| o < s as usize);
                let hi = at.partition_point(|&(o, _)| o < e as usize);
                let mut tally: BTreeMap<u32, usize> = BTreeMap::new();
                for &(_, src) in &at[lo..hi] {
                    *tally.entry(src).or_default() += 1;
                }
                tally.into_iter().collect()
            })
            .collect()
    }

    /// For each byte span of `code`: the source owning at least `purity`
    /// of the segments inside it, else None.
    pub fn attribute(&self, code: &str, spans: &[(u32, u32)], purity: f64) -> Vec<Option<u32>> {
        self.tally(code, spans)
            .into_iter()
            .map(|tally| {
                let total: usize = tally.iter().map(|t| t.1).sum();
                let (src, n) = tally
                    .into_iter()
                    .max_by_key(|&(src, n)| (n, std::cmp::Reverse(src)))?;
                (n as f64 >= purity * total as f64).then_some(src)
            })
            .collect()
    }
}

/// A bundled module's owner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Owner {
    pub package: String,
    pub version: String,
    /// The file, relative to the package root.
    pub file: String,
}

/// `a/b/../c` → `a/c`, lexically (no filesystem access).
fn lexical(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other),
        }
    }
    out
}

/// The owner of a sourcemap `source` (relative to `out_dir`, the map's
/// directory): the package under the LAST `node_modules/` in its path,
/// with the name and version its `package.json` declares. None for a file
/// outside `node_modules` (the app entry) or an unreadable package.json.
pub fn owner_of_source(out_dir: &Path, source: &str) -> Option<Owner> {
    owner_cached(out_dir, source, &mut HashMap::new())
}

/// [`owner_of_source`] with a package.json cache (one bundle reads the
/// same packages many times).
pub fn owner_cached(
    out_dir: &Path,
    source: &str,
    cache: &mut HashMap<PathBuf, Option<(String, String)>>,
) -> Option<Owner> {
    let path = lexical(&out_dir.join(source));
    let parts: Vec<String> = path
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    let nm = parts.iter().rposition(|p| p == "node_modules")?;
    let width = if parts.get(nm + 1)?.starts_with('@') {
        2
    } else {
        1
    };
    if parts.len() <= nm + width + 1 {
        return None;
    }
    let root: PathBuf = parts[..=nm + width].iter().collect();
    let declared = cache
        .entry(root.clone())
        .or_insert_with(|| {
            let text = std::fs::read_to_string(root.join("package.json")).ok()?;
            let v: serde_json::Value = serde_json::from_str(&text).ok()?;
            Some((
                v.get("name")?.as_str()?.to_string(),
                v.get("version")?.as_str()?.to_string(),
            ))
        })
        .clone()?;
    Some(Owner {
        package: declared.0,
        version: declared.1,
        file: parts[nm + width + 1..].join("/"),
    })
}
