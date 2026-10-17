//! The build cache without the bytes nobody reads (exp131). A bundler's
//! sourcemap embeds every source file (`sourcesContent`, ~3/4 of a map,
//! ~half of a work dir's build outputs); the builder reads only `sources`
//! and `mappings` ([`SourceMap::parse`]). A new build is slimmed before it
//! is stamped ([`crate::recipe::RecipeBuild::build`]), and
//! `humanify-index slim-cache` slims the builds an existing work dir
//! already holds — in place, through the inode, so a map hard-linked into
//! several work dirs is rewritten once and stays one shared file.
//!
//! What stays byte-for-byte: every other byte of the map (the member is
//! cut out of the text; nothing is re-serialised), and what the builder
//! reads from it — each rewrite is checked by parsing both texts with the
//! builder's own reader before the file is touched.

use std::collections::HashSet;
use std::io::Write;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use crate::sourcemap::SourceMap;

const KEY: &[u8] = b"\"sourcesContent\"";

fn skip_ws(b: &[u8], mut i: usize) -> usize {
    while i < b.len() && matches!(b[i], b' ' | b'\t' | b'\n' | b'\r') {
        i += 1;
    }
    i
}

/// The end (one past the closing quote) of the JSON string opening at `i`.
fn skip_string(b: &[u8], i: usize) -> Result<usize, String> {
    let mut j = i + 1;
    while j < b.len() {
        match b[j] {
            b'\\' => j += 2,
            b'"' => return Ok(j + 1),
            _ => j += 1,
        }
    }
    Err("map: unterminated string".into())
}

/// The end of the JSON value starting at `i` (containers are matched by
/// depth only: the result is validated by a real parse before use).
fn skip_value(b: &[u8], i: usize) -> Result<usize, String> {
    match b.get(i) {
        Some(b'"') => skip_string(b, i),
        Some(b'{' | b'[') => {
            let (mut j, mut depth) = (i, 0usize);
            while j < b.len() {
                match b[j] {
                    b'"' => {
                        j = skip_string(b, j)?;
                        continue;
                    }
                    b'{' | b'[' => depth += 1,
                    b'}' | b']' => {
                        depth -= 1;
                        if depth == 0 {
                            return Ok(j + 1);
                        }
                    }
                    _ => {}
                }
                j += 1;
            }
            Err("map: unterminated container".into())
        }
        Some(_) => {
            let mut j = i;
            while j < b.len() && !matches!(b[j], b',' | b'}' | b']' | b' ' | b'\t' | b'\n' | b'\r')
            {
                j += 1;
            }
            if j == i {
                return Err("map: missing value".into());
            }
            Ok(j)
        }
        None => Err("map: missing value".into()),
    }
}

/// `text` (a sourcemap: one JSON object) without its top-level
/// `sourcesContent` member and the comma that joined it, every other byte
/// kept; `None` when it has none. Refuses text that is not one object.
pub fn strip_sources_content(text: &str) -> Result<Option<String>, String> {
    let b = text.as_bytes();
    let mut i = skip_ws(b, 0);
    if b.get(i) != Some(&b'{') {
        return Err("map: not a JSON object".into());
    }
    i += 1;
    // the end of the previous member's value (a last member takes the
    // comma before it), and the byte range to cut
    let mut prev_end: Option<usize> = None;
    let mut cut: Option<(usize, usize)> = None;
    loop {
        i = skip_ws(b, i);
        match b.get(i) {
            Some(b'}') if prev_end.is_none() => {
                i += 1;
                break;
            }
            Some(b'"') => {}
            _ => return Err("map: expected a member name".into()),
        }
        let key_start = i;
        let key_end = skip_string(b, i)?;
        i = skip_ws(b, key_end);
        if b.get(i) != Some(&b':') {
            return Err("map: expected ':'".into());
        }
        i = skip_ws(b, i + 1);
        let value_end = skip_value(b, i)?;
        i = skip_ws(b, value_end);
        let target = cut.is_none() && &b[key_start..key_end] == KEY;
        match b.get(i) {
            Some(b',') => {
                if target {
                    cut = Some((key_start, skip_ws(b, i + 1)));
                }
                prev_end = Some(value_end);
                i += 1;
            }
            Some(b'}') => {
                if target {
                    cut = Some((prev_end.unwrap_or(key_start), value_end));
                }
                i += 1;
                break;
            }
            _ => return Err("map: expected ',' or '}'".into()),
        }
    }
    if skip_ws(b, i) != b.len() {
        return Err("map: text after the object".into());
    }
    Ok(cut.map(|(s, e)| format!("{}{}", &text[..s], &text[e..])))
}

/// What slimming one map did.
#[derive(Debug, PartialEq, Eq)]
pub enum SlimOutcome {
    /// Rewritten: its size before and after.
    Stripped { before: u64, after: u64 },
    /// No `sourcesContent`: untouched.
    Absent,
    /// Not a map the builder can read: untouched (the builder reads it the
    /// same way it did).
    Unreadable(String),
}

/// Overwrite `path`'s inode with `bytes` (every hard link sees it), mtime
/// kept.
fn overwrite(path: &Path, bytes: &[u8], mtime: std::time::SystemTime) -> Result<(), String> {
    let io = |e: std::io::Error| format!("{}: {e}", path.display());
    let mut f = std::fs::File::options()
        .write(true)
        .open(path)
        .map_err(io)?;
    f.write_all(bytes).map_err(io)?;
    f.set_len(bytes.len() as u64).map_err(io)?;
    f.set_modified(mtime).map_err(io)?;
    f.sync_data().map_err(io)
}

/// Write the journal entry for one rewrite (`<dev>-<ino>`: the path, a
/// newline, the new text), committed by a rename.
fn journal_entry(
    journal: &Path,
    path: &Path,
    meta: &std::fs::Metadata,
    slim: &str,
) -> Result<PathBuf, String> {
    let shown = path.to_str().filter(|p| !p.contains('\n')).ok_or(format!(
        "{}: a path the journal cannot record",
        path.display()
    ))?;
    let entry = journal.join(format!("{}-{}", meta.dev(), meta.ino()));
    let tmp = entry.with_extension("tmp");
    let jio = |e: std::io::Error| format!("journal {}: {e}", tmp.display());
    let mut f = std::fs::File::create(&tmp).map_err(jio)?;
    f.write_all(format!("{shown}\n").as_bytes()).map_err(jio)?;
    f.write_all(slim.as_bytes()).map_err(jio)?;
    f.sync_all().map_err(jio)?;
    std::fs::rename(&tmp, &entry).map_err(jio)?;
    Ok(entry)
}

/// Slim one map in place. With a `journal` directory the new text is
/// written there first, so a rewrite interrupted half-way is finished by
/// [`recover`] rather than leaving a truncated map behind a valid build
/// stamp.
pub fn slim_map_file(path: &Path, journal: Option<&Path>) -> Result<SlimOutcome, String> {
    let io = |e: std::io::Error| format!("{}: {e}", path.display());
    let Ok(text) = String::from_utf8(std::fs::read(path).map_err(io)?) else {
        return Ok(SlimOutcome::Unreadable("not UTF-8".into()));
    };
    let mut slim = match strip_sources_content(&text) {
        Ok(None) => return Ok(SlimOutcome::Absent),
        Ok(Some(s)) => s,
        Err(e) => return Ok(SlimOutcome::Unreadable(e)),
    };
    // a repeated key (JSON keeps the last): every copy goes
    while let Ok(Some(s)) = strip_sources_content(&slim) {
        slim = s;
    }
    let old = match SourceMap::parse(&text) {
        Ok(m) => m,
        Err(e) => return Ok(SlimOutcome::Unreadable(e)),
    };
    let new =
        SourceMap::parse(&slim).map_err(|e| format!("{}: slimmed map: {e}", path.display()))?;
    if old.sources != new.sources || old.segments != new.segments {
        return Err(format!(
            "{}: the slimmed map reads differently",
            path.display()
        ));
    }
    let meta = std::fs::metadata(path).map_err(io)?;
    let mtime = meta.modified().map_err(io)?;
    let entry = journal
        .map(|j| journal_entry(j, path, &meta, &slim))
        .transpose()?;
    overwrite(path, slim.as_bytes(), mtime)?;
    if let Some(entry) = entry {
        std::fs::remove_file(&entry).map_err(|e| format!("journal {}: {e}", entry.display()))?;
    }
    Ok(SlimOutcome::Stripped {
        before: text.len() as u64,
        after: slim.len() as u64,
    })
}

/// Finish every rewrite the journal holds (an entry still named `.tmp` was
/// never committed: its map was never touched, and it is dropped). Returns
/// how many were finished. Refuses an entry whose path no longer names
/// the inode it was written for.
pub fn recover(journal: &Path) -> Result<usize, String> {
    let Ok(entries) = std::fs::read_dir(journal) else {
        return Ok(0);
    };
    let mut done = 0;
    for entry in entries.flatten() {
        let p = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        let err = |e: std::io::Error| format!("{}: {e}", p.display());
        if name.ends_with(".tmp") {
            std::fs::remove_file(&p).map_err(err)?;
            continue;
        }
        let bytes = std::fs::read(&p).map_err(err)?;
        let nl = bytes
            .iter()
            .position(|&c| c == b'\n')
            .ok_or(format!("{}: no path line", p.display()))?;
        let target = PathBuf::from(String::from_utf8_lossy(&bytes[..nl]).into_owned());
        let meta = std::fs::metadata(&target).map_err(|e| format!("{}: {e}", target.display()))?;
        if name != format!("{}-{}", meta.dev(), meta.ino()) {
            return Err(format!(
                "{}: {} is another file now",
                p.display(),
                target.display()
            ));
        }
        let mtime = meta.modified().map_err(err)?;
        overwrite(&target, &bytes[nl + 1..], mtime)?;
        std::fs::remove_file(&p).map_err(err)?;
        done += 1;
    }
    Ok(done)
}

fn sorted_entries(dir: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| rd.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
    out.sort();
    out
}

/// What a walk has collected: one path per (device, inode), and the
/// symlinks it did not follow.
#[derive(Default)]
struct Walk {
    seen: HashSet<(u64, u64)>,
    maps: Vec<PathBuf>,
    skipped: usize,
}

/// Every `*.map` under `dir`, symlinks not followed.
fn walk_maps(dir: &Path, walk: &mut Walk) {
    for p in sorted_entries(dir) {
        let Ok(meta) = std::fs::symlink_metadata(&p) else {
            continue;
        };
        if meta.file_type().is_symlink() {
            walk.skipped += 1;
        } else if meta.is_dir() {
            walk_maps(&p, walk);
        } else if p.extension().is_some_and(|e| e == "map")
            && walk.seen.insert((meta.dev(), meta.ino()))
        {
            walk.maps.push(p);
        }
    }
}

/// The maps a slim pass rewrites: every `*.map` under each work dir's
/// `<install>/build/` (never `node_modules`, never the entries), one path
/// per inode across all the work dirs, in path order. Symlinks are never
/// followed or replaced (what one points at is slimmed through its own
/// work dir); their count is returned beside the maps.
pub fn cache_maps(work_dirs: &[PathBuf]) -> (Vec<PathBuf>, usize) {
    let mut walk = Walk::default();
    for w in work_dirs {
        for install in sorted_entries(w) {
            let build = install.join("build");
            match std::fs::symlink_metadata(&build) {
                Ok(m) if m.file_type().is_symlink() => walk.skipped += 1,
                Ok(m) if m.is_dir() => walk_maps(&build, &mut walk),
                _ => {}
            }
        }
    }
    (walk.maps, walk.skipped)
}

/// What a slim pass did.
#[derive(Debug, Default)]
pub struct SlimReport {
    pub maps: usize,
    pub stripped: usize,
    pub absent: usize,
    pub bytes_before: u64,
    pub bytes_after: u64,
    pub skipped_symlinks: usize,
    pub recovered: usize,
    pub unreadable: Vec<(PathBuf, String)>,
    pub errors: Vec<String>,
}

/// Slim every map of `work_dirs` (journaled in `journal`, which is
/// recovered first), on the core's parallel map (bounded by the process's
/// CPU affinity: run it under `taskset`). `progress` sees the report after
/// every batch.
pub fn slim_cache(
    work_dirs: &[PathBuf],
    journal: &Path,
    progress: &mut dyn FnMut(&SlimReport),
) -> Result<SlimReport, String> {
    std::fs::create_dir_all(journal).map_err(|e| format!("{}: {e}", journal.display()))?;
    let mut report = SlimReport {
        recovered: recover(journal)?,
        ..SlimReport::default()
    };
    let (maps, skipped) = cache_maps(work_dirs);
    report.skipped_symlinks = skipped;
    for batch in maps.chunks(2048) {
        let results = humanify_core::par::map_ordered(batch, |p| slim_map_file(p, Some(journal)));
        for (p, r) in batch.iter().zip(results) {
            report.maps += 1;
            match r {
                Ok(SlimOutcome::Stripped { before, after }) => {
                    report.stripped += 1;
                    report.bytes_before += before;
                    report.bytes_after += after;
                }
                Ok(SlimOutcome::Absent) => report.absent += 1,
                Ok(SlimOutcome::Unreadable(e)) => report.unreadable.push((p.clone(), e)),
                Err(e) => report.errors.push(e),
            }
        }
        progress(&report);
    }
    Ok(report)
}
