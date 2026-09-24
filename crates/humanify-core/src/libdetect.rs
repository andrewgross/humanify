//! Stage 3: library detection (WPB.3) — which unpacked files are
//! third-party library code (skipped by the rename pipeline) and, inside
//! scope-hoisted bundles, which comment-delimited regions are. TS
//! originals: `src/library-detection/index.ts` (the detector registry),
//! `banner-patterns.ts`, `comment-regions.ts`, `adapters/bun.ts`,
//! `adapters/default.ts`, `types.ts`.
//!
//! Every TS regex is a non-unicode V8 `RegExp`; each is emulated by hand
//! over `crate::detect::js_text`'s primitives (`\s` = ECMAScript WhiteSpace
//! and LineTerminator, `\S` its complement, `\d` ASCII), and each emulation
//! names the pattern it mirrors. All four banner patterns open with a
//! literal and every quantifier in them is followed by a disjoint class, so
//! the backtracking matcher has exactly one way to match at each start —
//! the emulations are direct scans.
//!
//! Offsets are UTF-8 byte offsets (the Rust span unit); the TS's are UTF-16
//! code units. `CommentRegion` carries bytes; a writer that must reproduce
//! the TS JSON converts with `js_text::utf16_offset`.

use std::collections::HashMap;
use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::detect::js_text::{is_js_space, js_prefix, skip_js_space};
use crate::unpack::bun::bun_manifest_path;
use crate::unpack::{ModuleMetadata, UnpackedFile};

/// A banner-delimited library region (`CommentRegion`): from its banner to
/// the next banner, the last one to EOF (`end` None).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommentRegion {
    pub library_name: String,
    /// Byte offset of the banner match.
    pub start: usize,
    /// Byte offset of the next banner, None = to EOF.
    pub end: Option<usize>,
}

/// Which layer identified a library file (`detectedBy`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DetectedBy {
    Path,
    Comment,
    CommentRegion,
}

impl DetectedBy {
    pub fn as_str(self) -> &'static str {
        match self {
            DetectedBy::Path => "path",
            DetectedBy::Comment => "comment",
            DetectedBy::CommentRegion => "comment-region",
        }
    }
}

/// One file's verdict (`LibraryDetection`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibraryDetection {
    pub is_library: bool,
    pub library_name: Option<String>,
    pub detected_by: Option<DetectedBy>,
    pub module_metadata: Option<ModuleMetadata>,
}

/// A file with interleaved library/app code (`MixedFileDetection`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MixedFileDetection {
    pub regions: Vec<CommentRegion>,
    /// The regions' library names, unique, in first-seen order.
    pub library_names: Vec<String>,
}

/// `LibraryDetectionResult`, with the TS Maps as insertion-ordered vectors
/// (a re-set key keeps its first position and takes the last value).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LibraryDetectionResult {
    pub library_files: Vec<(PathBuf, LibraryDetection)>,
    pub novel_files: Vec<PathBuf>,
    pub mixed_files: Vec<(PathBuf, MixedFileDetection)>,
}

/// `Map.set` on an insertion-ordered vector.
fn map_set<V>(map: &mut Vec<(PathBuf, V)>, key: &Path, value: V) {
    match map.iter_mut().find(|(k, _)| k == key) {
        Some(slot) => slot.1 = value,
        None => map.push((key.to_path_buf(), value)),
    }
}

// ---------------------------------------------------------------------------
// The registry (index.ts)
// ---------------------------------------------------------------------------

/// The registered detectors, in registry order: bun first, default last
/// (the fallback — it supports every config).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LibraryDetector {
    Bun,
    Default,
}

impl LibraryDetector {
    pub fn name(self) -> &'static str {
        match self {
            LibraryDetector::Bun => "bun",
            LibraryDetector::Default => "default",
        }
    }

    /// `supports(config)`: the bun detector only behind the bun unpack
    /// adapter.
    pub fn supports(self, unpack_adapter_name: &str) -> bool {
        match self {
            LibraryDetector::Bun => unpack_adapter_name == "bun",
            LibraryDetector::Default => true,
        }
    }
}

/// `selectLibraryDetector(config)`.
pub fn select_library_detector(unpack_adapter_name: &str) -> LibraryDetector {
    [LibraryDetector::Bun, LibraryDetector::Default]
        .into_iter()
        .find(|d| d.supports(unpack_adapter_name))
        .unwrap_or(LibraryDetector::Default)
}

/// `detector.detectLibraries(files)`.
pub fn detect_libraries(
    detector: LibraryDetector,
    files: &[UnpackedFile],
) -> Result<LibraryDetectionResult, String> {
    match detector {
        LibraryDetector::Bun => detect_bun(files),
        LibraryDetector::Default => detect_default(files),
    }
}

/// `fs.readFile(path, "utf-8")`: invalid bytes become U+FFFD.
fn read_utf8(path: &Path) -> Result<String, String> {
    fs::read(path)
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .map_err(|e| format!("read {}: {e}", path.display()))
}

// ---------------------------------------------------------------------------
// Banner patterns (banner-patterns.ts)
// ---------------------------------------------------------------------------

/// `BANNER_PATTERNS`, in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Banner {
    /// `/\/\*!\s*(\S+)\s+(?:-\s+)?v[\d.]+/` — `/*! name v1.2.3` or
    /// `/*! name - v1.2.3`.
    Bang,
    /// `/\/\*\*?\s*@license\s+(\S+)/`.
    License,
    /// `/\/\*\*?\s*@module\s+(\S+)/`.
    Module,
    /// `/\*\s+(\S+)\s+v\d+\.\d+\.\d+/` — ` * name v1.2.3` inside a block.
    Star,
}

const BANNERS: [Banner; 4] = [Banner::Bang, Banner::License, Banner::Module, Banner::Star];

/// One banner match: where it starts, the capture, where it ends.
struct BannerMatch<'s> {
    start: usize,
    name: &'s str,
    end: usize,
}

/// `\S+` from `at` (maximal, at least one char) → its end.
fn non_space_run(s: &str, at: usize) -> Option<usize> {
    let end = s[at..]
        .char_indices()
        .find(|&(_, c)| is_js_space(c))
        .map_or(s.len(), |(i, _)| at + i);
    (end > at).then_some(end)
}

/// `\s+` from `at` → its end.
fn space_run(s: &str, at: usize) -> Option<usize> {
    let end = skip_js_space(s, at);
    (end > at).then_some(end)
}

/// A run of bytes satisfying `pred` from `at` (at least one) → its end.
fn byte_run(s: &str, at: usize, pred: fn(u8) -> bool) -> Option<usize> {
    let end = s.as_bytes()[at..]
        .iter()
        .position(|&b| !pred(b))
        .map_or(s.len(), |i| at + i);
    (end > at).then_some(end)
}

impl Banner {
    /// The literal every match starts with.
    fn head(self) -> &'static str {
        match self {
            Banner::Bang => "/*!",
            Banner::License | Banner::Module => "/*",
            Banner::Star => "*",
        }
    }

    /// The pattern anchored at `at` (where `head` occurs).
    fn match_at(self, s: &str, at: usize) -> Option<BannerMatch<'_>> {
        let after = at + self.head().len();
        let (name_at, name_end, end) = match self {
            Banner::Bang => {
                let name_at = skip_js_space(s, after);
                let name_end = non_space_run(s, name_at)?;
                let mut v_at = space_run(s, name_end)?;
                if s[v_at..].starts_with('-') {
                    v_at = space_run(s, v_at + 1)?;
                }
                if !s[v_at..].starts_with('v') {
                    return None;
                }
                let end = byte_run(s, v_at + 1, |b| b.is_ascii_digit() || b == b'.')?;
                (name_at, name_end, end)
            }
            Banner::License | Banner::Module => {
                let keyword = if self == Banner::License {
                    "@license"
                } else {
                    "@module"
                };
                // `\*?` greedy first, then without it.
                let with_star = s[after..]
                    .starts_with('*')
                    .then(|| skip_js_space(s, after + 1))
                    .filter(|&k| s[k..].starts_with(keyword));
                let k = with_star.or_else(|| {
                    let k = skip_js_space(s, after);
                    s[k..].starts_with(keyword).then_some(k)
                })?;
                let name_at = space_run(s, k + keyword.len())?;
                let name_end = non_space_run(s, name_at)?;
                (name_at, name_end, name_end)
            }
            Banner::Star => {
                let name_at = space_run(s, after)?;
                let name_end = non_space_run(s, name_at)?;
                let v_at = space_run(s, name_end)?;
                if !s[v_at..].starts_with('v') {
                    return None;
                }
                let mut p = v_at + 1;
                for part in 0..3 {
                    p = byte_run(s, p, |b| b.is_ascii_digit())?;
                    if part < 2 {
                        if !s[p..].starts_with('.') {
                            return None;
                        }
                        p += 1;
                    }
                }
                (name_at, name_end, p)
            }
        };
        Some(BannerMatch {
            start: at,
            name: &s[name_at..name_end],
            end,
        })
    }

    /// `regex.exec(s)` with `lastIndex = from`: the leftmost match at or
    /// after `from`.
    fn find_from(self, s: &str, from: usize) -> Option<BannerMatch<'_>> {
        s[from..]
            .match_indices(self.head())
            .find_map(|(i, _)| self.match_at(s, from + i))
    }
}

/// `normalizeLibraryName`: strip a trailing `[,;:!]+` run, one leading
/// `@`, lowercase (`toLowerCase`: Unicode default case mapping).
pub fn normalize_library_name(name: &str) -> String {
    let trimmed = name.trim_end_matches([',', ';', ':', '!']);
    let unscoped = trimmed.strip_prefix('@').unwrap_or(trimmed);
    unscoped.to_lowercase()
}

/// The first pattern (in `BANNER_PATTERNS` order) with a match anywhere in
/// `s`, its capture normalized (`scanForBanner`, and over a 1KB prefix
/// `detectLibraryFromHeader`).
fn first_banner_name(s: &str) -> Option<String> {
    BANNERS
        .iter()
        .find_map(|b| b.find_from(s, 0))
        .map(|m| normalize_library_name(m.name))
}

// ---------------------------------------------------------------------------
// Comment regions (comment-regions.ts)
// ---------------------------------------------------------------------------

/// `findCommentRegions`: every match of every banner pattern (global scan,
/// `lastIndex` past each match), sorted by offset (stable — the pattern
/// order breaks ties), deduplicated by offset (the first kept); each
/// region runs to the next banner, the last to EOF.
pub fn find_comment_regions(code: &str) -> Vec<CommentRegion> {
    let mut matches: Vec<(usize, String)> = Vec::new();
    for banner in BANNERS {
        let mut from = 0;
        while let Some(m) = banner.find_from(code, from) {
            matches.push((m.start, normalize_library_name(m.name)));
            from = m.end;
        }
    }
    matches.sort_by_key(|(offset, _)| *offset);
    matches.dedup_by_key(|(offset, _)| *offset);
    let starts: Vec<usize> = matches.iter().map(|(o, _)| *o).collect();
    matches
        .into_iter()
        .enumerate()
        .map(|(i, (start, library_name))| CommentRegion {
            library_name,
            start,
            end: starts.get(i + 1).copied(),
        })
        .collect()
}

/// `classifyFunctionsByRegion`: each function (key, start offset — None
/// skips it) inside a region → that region's library name, in input order
/// (a repeated key keeps its first position, last value — the TS Map).
/// Regions are sorted by start; a function before every region, or at/after
/// a bounded region's end, is app code.
pub fn classify_functions_by_region<K: Clone + PartialEq>(
    functions: impl IntoIterator<Item = (K, Option<usize>)>,
    regions: &[CommentRegion],
) -> Vec<(K, String)> {
    let mut out: Vec<(K, String)> = Vec::new();
    if regions.is_empty() {
        return out;
    }
    for (key, start) in functions {
        let Some(start) = start else { continue };
        let Some(i) = find_region(regions, start) else {
            continue;
        };
        let name = regions[i].library_name.clone();
        match out.iter_mut().find(|(k, _)| *k == key) {
            Some(slot) => slot.1 = name,
            None => out.push((key, name)),
        }
    }
    out
}

/// `findRegion`: the last region starting at or before `offset`, if
/// `offset` is inside it.
fn find_region(regions: &[CommentRegion], offset: usize) -> Option<usize> {
    let i = regions
        .partition_point(|r| r.start <= offset)
        .checked_sub(1)?;
    match regions[i].end {
        Some(end) if offset >= end => None,
        _ => Some(i),
    }
}

// ---------------------------------------------------------------------------
// The default detector (adapters/default.ts)
// ---------------------------------------------------------------------------

/// Header window for the banner layer: 1024 UTF-16 code units.
const SCAN_LIMIT: usize = 1024;

/// `isLibraryPath`: `/node_modules\//` anywhere, or one of the runtime
/// packages as a prefix (`^@babel\/runtime`, `^core-js`,
/// `^regenerator-runtime`, `^tslib`, `^webpack\/runtime`).
pub fn is_library_path(module_path: &str) -> bool {
    module_path.contains("node_modules/")
        || [
            "@babel/runtime",
            "core-js",
            "regenerator-runtime",
            "tslib",
            "webpack/runtime",
        ]
        .iter()
        .any(|p| module_path.starts_with(p))
}

/// `[^/]+` from `at` → its end.
fn non_slash_run(s: &str, at: usize) -> Option<usize> {
    byte_run(s, at, |b| b != b'/')
}

/// `@[^/]+\/[^/]+` at `at` → its end.
fn scoped_at(s: &str, at: usize) -> Option<usize> {
    if !s[at..].starts_with('@') {
        return None;
    }
    let scope_end = non_slash_run(s, at + 1)?;
    if !s[scope_end..].starts_with('/') {
        return None;
    }
    non_slash_run(s, scope_end + 1)
}

/// `extractLibraryNameFromPath`: the package after the first
/// `node_modules/` that is followed by a name
/// (`/node_modules\/(@[^/]+\/[^/]+|[^/]+)/`), else a leading scoped
/// package (`/^(@[^/]+\/[^/]+)/`), else the first path segment.
pub fn extract_library_name_from_path(module_path: &str) -> String {
    let after_node_modules = module_path
        .match_indices("node_modules/")
        .find_map(|(i, lit)| {
            let at = i + lit.len();
            let end = scoped_at(module_path, at).or_else(|| non_slash_run(module_path, at))?;
            Some(&module_path[at..end])
        });
    if let Some(name) = after_node_modules {
        return name.to_string();
    }
    if let Some(end) = scoped_at(module_path, 0) {
        return module_path[..end].to_string();
    }
    module_path.split('/').next().unwrap_or("").to_string()
}

fn detect_default(files: &[UnpackedFile]) -> Result<LibraryDetectionResult, String> {
    let mut result = LibraryDetectionResult::default();
    for file in files {
        // Layer 1: the module path webcrack reported.
        if let Some(meta) = &file.metadata
            && !meta.module_path.is_empty()
            && is_library_path(&meta.module_path)
        {
            let detection = LibraryDetection {
                is_library: true,
                library_name: Some(extract_library_name_from_path(&meta.module_path)),
                detected_by: Some(DetectedBy::Path),
                module_metadata: file.metadata.clone(),
            };
            map_set(&mut result.library_files, &file.path, detection);
            continue;
        }
        let code = read_utf8(&file.path)?;
        // Layer 2: a banner in the first ~1KB.
        if let Some(name) = first_banner_name(js_prefix(&code, SCAN_LIMIT)) {
            let detection = LibraryDetection {
                is_library: true,
                library_name: Some(name),
                detected_by: Some(DetectedBy::Comment),
                module_metadata: file.metadata.clone(),
            };
            map_set(&mut result.library_files, &file.path, detection);
            continue;
        }
        // Layer 3: banner regions anywhere in the file.
        let regions = find_comment_regions(&code);
        if !regions.is_empty() {
            let mut library_names: Vec<String> = Vec::new();
            for r in &regions {
                if !library_names.contains(&r.library_name) {
                    library_names.push(r.library_name.clone());
                }
            }
            map_set(
                &mut result.mixed_files,
                &file.path,
                MixedFileDetection {
                    regions,
                    library_names,
                },
            );
        }
        result.novel_files.push(file.path.clone());
    }
    Ok(result)
}

// ---------------------------------------------------------------------------
// The Bun detector (adapters/bun.ts)
// ---------------------------------------------------------------------------

/// Deepest a factory file sits below the output root:
/// vendor/@scope/pkg/file.js — three directory levels.
const MAX_VENDOR_DEPTH: usize = 3;

/// The manifest fields the detector reads.
struct Manifest {
    runtime_file: Option<String>,
    /// fileName → name (a repeated fileName keeps the LAST entry — `new
    /// Map(entries)`).
    factories: HashMap<String, Option<String>>,
}

/// Parse a manifest; None when it is not JSON (the TS `JSON.parse` throw,
/// caught by the walk). A manifest without a `factories` array is an error:
/// the TS `manifest.factories.map` throws out of `detectLibraries`.
fn parse_manifest(raw: &str) -> Option<Result<Manifest, String>> {
    let value: serde_json::Value = serde_json::from_str(raw).ok()?;
    let Some(entries) = value.get("factories").and_then(|f| f.as_array()) else {
        return Some(Err("bun manifest without a factories array".to_string()));
    };
    let mut factories = HashMap::new();
    for e in entries {
        if let Some(file_name) = e.get("fileName").and_then(|f| f.as_str()) {
            factories.insert(
                file_name.to_string(),
                e.get("name").and_then(|n| n.as_str()).map(str::to_string),
            );
        }
    }
    Some(Ok(Manifest {
        runtime_file: value
            .get("runtimeFile")
            .and_then(|r| r.as_str())
            .map(str::to_string),
        factories,
    }))
}

/// `loadManifest`: walk up from the first file's directory (at most
/// `MAX_VENDOR_DEPTH` levels) until `<dir>/vendor/_bun-modules.json`
/// reads and parses.
fn load_manifest(files: &[UnpackedFile]) -> Option<Result<(Manifest, PathBuf), String>> {
    let mut dir = files.first()?.path.parent()?.to_path_buf();
    for _ in 0..=MAX_VENDOR_DEPTH {
        if let Ok(raw) = read_utf8(&bun_manifest_path(&dir))
            && let Some(parsed) = parse_manifest(&raw)
        {
            return Some(parsed.map(|m| (m, dir)));
        }
        let parent = dir.parent()?.to_path_buf();
        if parent == dir {
            break;
        }
        dir = parent;
    }
    None
}

/// Node's `path.relative(from, to)` over absolute or same-rooted paths,
/// '/'-joined (`toManifestPath`).
pub fn relative_posix(from: &Path, to: &Path) -> String {
    let norm = |p: &Path| -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for c in p.components() {
            match c {
                Component::Normal(s) => out.push(s.to_string_lossy().into_owned()),
                Component::ParentDir => {
                    out.pop();
                }
                _ => {}
            }
        }
        out
    };
    let (a, b) = (norm(from), norm(to));
    let common = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let mut parts: Vec<String> = vec!["..".to_string(); a.len() - common];
    parts.extend(b[common..].iter().cloned());
    parts.join("/")
}

fn detect_bun(files: &[UnpackedFile]) -> Result<LibraryDetectionResult, String> {
    let mut result = LibraryDetectionResult::default();
    if let Some(found) = load_manifest(files) {
        let (manifest, root) = found?;
        for file in files {
            let rel = relative_posix(&root, &file.path);
            if manifest.runtime_file.as_deref() == Some(rel.as_str()) {
                result.novel_files.push(file.path.clone());
                continue;
            }
            let Some(name) = manifest.factories.get(&rel) else {
                result.novel_files.push(file.path.clone());
                continue;
            };
            let detection = LibraryDetection {
                is_library: true,
                library_name: name.clone(),
                detected_by: Some(DetectedBy::Comment),
                module_metadata: file.metadata.clone(),
            };
            map_set(&mut result.library_files, &file.path, detection);
        }
        return Ok(result);
    }
    // No manifest: every file with a banner ANYWHERE is library (a Bun
    // factory is one module).
    for file in files {
        let code = read_utf8(&file.path)?;
        match first_banner_name(&code) {
            Some(name) => {
                let detection = LibraryDetection {
                    is_library: true,
                    library_name: Some(name),
                    detected_by: Some(DetectedBy::Comment),
                    module_metadata: file.metadata.clone(),
                };
                map_set(&mut result.library_files, &file.path, detection);
            }
            None => result.novel_files.push(file.path.clone()),
        }
    }
    Ok(result)
}
