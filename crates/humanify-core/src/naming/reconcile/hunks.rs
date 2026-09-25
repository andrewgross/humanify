//! The system diff and its hunk analysis — diff-reconcile.ts
//! `computeNormalDiff`, `parseNormalDiff`, `analyzeHunks` and the exp088
//! skeleton-vote pool.
//!
//! `computeNormalDiff` shells out to the system `diff` exactly as the TS
//! does (normal format, CRLF-normalized temp files): the alignment IS
//! GNU diff's, and re-implementing its LCS heuristics would be a second
//! diff with its own hunk boundaries. The binary is a pipeline dependency
//! on both sides until the TS core is deleted (phase 6).

use std::collections::{BTreeSet, HashMap};

use super::lexer::{PairDiff, compare_line_pair, line_skeleton, units};

/// One `diff` hunk (`DiffHunk`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffHunk {
    pub op: u8,
    /// 1-based first line in the prior text.
    pub prior_start: usize,
    /// 1-based first line in the new text.
    pub new_start: usize,
    pub prior_lines: Vec<String>,
    pub new_lines: Vec<String>,
}

/// `computeNormalDiff(prior, new)`: `diff prior new` (normal format) over
/// CRLF-normalized copies. Err on a real `diff` failure (exit ≥ 2, spawn
/// failure) — the step treats that as "skip", never fatal.
pub fn compute_normal_diff(prior_text: &str, new_text: &str) -> Result<String, String> {
    let dir = tempdir()?;
    let prior_path = dir.join("prior.js");
    let new_path = dir.join("new.js");
    let result = (|| {
        std::fs::write(&prior_path, prior_text.replace("\r\n", "\n"))
            .map_err(|e| format!("write prior: {e}"))?;
        std::fs::write(&new_path, new_text.replace("\r\n", "\n"))
            .map_err(|e| format!("write new: {e}"))?;
        let out = std::process::Command::new("diff")
            .arg(&prior_path)
            .arg(&new_path)
            .output()
            .map_err(|e| format!("diff failed (status null): {e}"))?;
        match out.status.code() {
            Some(0 | 1) => Ok(String::from_utf8_lossy(&out.stdout).into_owned()),
            code => Err(format!(
                "diff failed (status {code:?}): {}",
                String::from_utf8_lossy(&out.stderr)
            )),
        }
    })();
    let _ = std::fs::remove_dir_all(&dir);
    result
}

/// A private temp directory (`fs.mkdtempSync(os.tmpdir()/humanify-reconcile-)`).
fn tempdir() -> Result<std::path::PathBuf, String> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let base = std::env::temp_dir();
    for _ in 0..100 {
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        let dir = base.join(format!(
            "humanify-reconcile-{}-{n}-{nanos}",
            std::process::id()
        ));
        if std::fs::create_dir(&dir).is_ok() {
            return Ok(dir);
        }
    }
    Err("cannot create a reconcile temp dir".to_string())
}

/// `HUNK_HEADER`: `^(\d+)(?:,\d+)?([acd])(\d+)(?:,\d+)?$`.
fn parse_hunk_header(line: &str) -> Option<DiffHunk> {
    let b = line.as_bytes();
    let mut i = 0;
    let digits = |i: &mut usize| -> Option<usize> {
        let s = *i;
        while *i < b.len() && b[*i].is_ascii_digit() {
            *i += 1;
        }
        if *i == s {
            return None;
        }
        // JS Number() of a digit run: exact for line counts.
        line[s..*i].parse().ok()
    };
    let skip_range = |i: &mut usize| -> bool {
        if *i < b.len() && b[*i] == b',' {
            *i += 1;
            let s = *i;
            while *i < b.len() && b[*i].is_ascii_digit() {
                *i += 1;
            }
            return *i > s;
        }
        true
    };
    let prior_start = digits(&mut i)?;
    if !skip_range(&mut i) {
        return None;
    }
    let op = *b.get(i)?;
    if !matches!(op, b'a' | b'c' | b'd') {
        return None;
    }
    i += 1;
    let new_start = digits(&mut i)?;
    if !skip_range(&mut i) || i != b.len() {
        return None;
    }
    Some(DiffHunk {
        op,
        prior_start,
        new_start,
        prior_lines: Vec::new(),
        new_lines: Vec::new(),
    })
}

/// `hunkContent(line, marker)`.
fn hunk_content(line: &str, marker: char) -> Option<&str> {
    let mut chars = line.chars();
    if chars.next() != Some(marker) {
        return None;
    }
    let rest = chars.as_str();
    if rest.is_empty() {
        return Some("");
    }
    rest.strip_prefix(' ')
}

/// `parseNormalDiff`.
pub fn parse_normal_diff(diff_text: &str) -> Vec<DiffHunk> {
    let mut hunks: Vec<DiffHunk> = Vec::new();
    for line in diff_text.split('\n') {
        if let Some(h) = parse_hunk_header(line) {
            hunks.push(h);
            continue;
        }
        let Some(current) = hunks.last_mut() else {
            continue;
        };
        if let Some(prior) = hunk_content(line, '<') {
            current.prior_lines.push(prior.to_string());
            continue;
        }
        if let Some(added) = hunk_content(line, '>') {
            current.new_lines.push(added.to_string());
        }
    }
    hunks
}

/// A rename proposal at a position of the new output (`PositionCandidate`).
#[derive(Clone, Debug)]
pub struct PositionCandidate {
    /// 1-based line (the diff's line numbering).
    pub line: usize,
    /// 0-based UTF-16 column.
    pub col: usize,
    pub from_name: String,
    pub to_name: String,
    pub hunk_index: usize,
    /// The witness key: the line's skeleton (None → `hunk:<index>`).
    pub skeleton: Option<Vec<u16>>,
}

/// A clean rename-noise line's pair info (`NoiseLineInfo`).
#[derive(Clone, Debug)]
pub struct NoiseLineInfo {
    pub hunk_index: usize,
    pub diffs: Vec<PairDiff>,
}

/// `HunkAnalysis`.
#[derive(Default, Debug)]
pub struct HunkAnalysis {
    pub candidates: Vec<PositionCandidate>,
    pub noise_lines: HashMap<usize, NoiseLineInfo>,
    pub changed_new_lines: BTreeSet<usize>,
    pub mixed_hunk_indexes: BTreeSet<usize>,
    pub changed: usize,
    pub genuine: usize,
    pub oversized: usize,
    pub noise_hunks: usize,
    pub mixed_hunks: usize,
}

enum Classified {
    Genuine,
    Oversized,
    Noise(Vec<Option<Vec<PairDiff>>>),
    Mixed(Vec<Option<Vec<PairDiff>>>),
}

fn index_by_skeleton(lines: &[Vec<u16>]) -> HashMap<Vec<u16>, Vec<usize>> {
    let mut by: HashMap<Vec<u16>, Vec<usize>> = HashMap::new();
    for (i, l) in lines.iter().enumerate() {
        if let Some(sk) = line_skeleton(l) {
            by.entry(sk).or_default().push(i);
        }
    }
    by
}

/// `skeletonPairs` (an UNBALANCED hunk paired by unique skeleton).
fn skeleton_pairs(prior: &[Vec<u16>], new: &[Vec<u16>]) -> Option<Vec<Option<Vec<PairDiff>>>> {
    let prior_by = index_by_skeleton(prior);
    let new_by = index_by_skeleton(new);
    let mut pairs = Vec::with_capacity(new.len());
    let mut paired = 0;
    for line in new {
        let matched = line_skeleton(line).and_then(|sk| {
            if new_by.get(&sk).map(Vec::len) != Some(1) {
                return None;
            }
            match prior_by.get(&sk) {
                Some(v) if v.len() == 1 => Some(v[0]),
                _ => None,
            }
        });
        let cmp = matched.and_then(|p| compare_line_pair(&prior[p], line));
        if cmp.is_some() {
            paired += 1;
        }
        pairs.push(cmp);
    }
    (paired > 0).then_some(pairs)
}

fn classify_change_hunk(
    prior: &[Vec<u16>],
    new: &[Vec<u16>],
    max_hunk_lines: usize,
    mixed_hunk_tier: bool,
) -> Classified {
    if prior.len() != new.len() {
        if !mixed_hunk_tier {
            return Classified::Genuine;
        }
        return match skeleton_pairs(prior, new) {
            None => Classified::Genuine,
            Some(pairs) => Classified::Mixed(pairs),
        };
    }
    if new.len() > max_hunk_lines {
        return Classified::Oversized;
    }
    let mut pairs = Vec::with_capacity(new.len());
    let mut dirty = 0;
    for (p, n) in prior.iter().zip(new) {
        match compare_line_pair(p, n) {
            None => {
                if !mixed_hunk_tier {
                    return Classified::Genuine;
                }
                dirty += 1;
                pairs.push(None);
            }
            Some(d) => pairs.push(Some(d)),
        }
    }
    if dirty == 0 {
        Classified::Noise(pairs)
    } else if dirty == pairs.len() {
        Classified::Genuine
    } else {
        Classified::Mixed(pairs)
    }
}

fn record_hunk_pairs(
    analysis: &mut HunkAnalysis,
    hunk: &DiffHunk,
    new: &[Vec<u16>],
    hunk_index: usize,
    pairs: Vec<Option<Vec<PairDiff>>>,
) {
    for (k, pair) in pairs.into_iter().enumerate() {
        let Some(diffs) = pair else { continue };
        let line = hunk.new_start + k;
        let skeleton = line_skeleton(&new[k]);
        for d in &diffs {
            analysis.candidates.push(PositionCandidate {
                line,
                col: d.col,
                from_name: d.from_name.clone(),
                to_name: d.to_name.clone(),
                hunk_index,
                skeleton: skeleton.clone(),
            });
        }
        analysis
            .noise_lines
            .insert(line, NoiseLineInfo { hunk_index, diffs });
    }
}

/// `analyzeHunks`.
pub fn analyze_hunks(
    hunks: &[DiffHunk],
    max_hunk_lines: usize,
    mixed_hunk_tier: bool,
    skeleton_vote_tier: bool,
) -> HunkAnalysis {
    let mut analysis = HunkAnalysis::default();
    for (hunk_index, hunk) in hunks.iter().enumerate() {
        for k in 0..hunk.new_lines.len() {
            analysis.changed_new_lines.insert(hunk.new_start + k);
        }
        if hunk.op != b'c' {
            continue;
        }
        analysis.changed += 1;
        let prior: Vec<Vec<u16>> = hunk.prior_lines.iter().map(|l| units(l)).collect();
        let new: Vec<Vec<u16>> = hunk.new_lines.iter().map(|l| units(l)).collect();
        let pairs = match classify_change_hunk(&prior, &new, max_hunk_lines, mixed_hunk_tier) {
            Classified::Genuine => {
                analysis.genuine += 1;
                continue;
            }
            Classified::Oversized => {
                analysis.oversized += 1;
                continue;
            }
            Classified::Mixed(pairs) => {
                analysis.mixed_hunks += 1;
                analysis.mixed_hunk_indexes.insert(hunk_index);
                pairs
            }
            Classified::Noise(pairs) => {
                analysis.noise_hunks += 1;
                pairs
            }
        };
        record_hunk_pairs(&mut analysis, hunk, &new, hunk_index, pairs);
    }
    if skeleton_vote_tier {
        add_skeleton_vote_candidates(&mut analysis, hunks);
    }
    analysis
}

/// One new-side changed line of the skeleton pool.
struct PoolLine {
    text: Vec<u16>,
    hunk_index: usize,
    line: usize,
}

/// `addSkeletonVoteCandidates` (exp088): pair residual changed lines by
/// unique skeleton across the whole diff. The TS iterates its `newAt` Map
/// in INSERTION order (first occurrence of each skeleton) — kept here.
fn add_skeleton_vote_candidates(analysis: &mut HunkAnalysis, hunks: &[DiffHunk]) {
    let mut prior_at: HashMap<Vec<u16>, Vec<Vec<u16>>> = HashMap::new();
    let mut new_order: Vec<Vec<u16>> = Vec::new();
    let mut new_at: HashMap<Vec<u16>, Vec<PoolLine>> = HashMap::new();
    for (hunk_index, hunk) in hunks.iter().enumerate() {
        for text in &hunk.prior_lines {
            let u = units(text);
            if let Some(sk) = line_skeleton(&u) {
                prior_at.entry(sk).or_default().push(u);
            }
        }
        for (k, text) in hunk.new_lines.iter().enumerate() {
            let u = units(text);
            let Some(sk) = line_skeleton(&u) else {
                continue;
            };
            let entry = new_at.entry(sk.clone()).or_insert_with(|| {
                new_order.push(sk);
                Vec::new()
            });
            entry.push(PoolLine {
                text: u,
                hunk_index,
                line: hunk.new_start + k,
            });
        }
    }
    for sk in new_order {
        let infos = &new_at[&sk];
        let Some(priors) = prior_at.get(&sk) else {
            continue;
        };
        if priors.len() != infos.len() {
            continue;
        }
        if !unanimous_mapping(priors, infos) {
            continue;
        }
        emit_shape_votes(analysis, &sk, priors, infos);
    }
}

/// `unanimousMapping`: every cross pairing clean with the SAME rename set.
fn unanimous_mapping(priors: &[Vec<u16>], infos: &[PoolLine]) -> bool {
    let mut canonical: Option<Vec<String>> = None;
    for p in priors {
        for info in infos {
            let Some(diffs) = compare_line_pair(p, &info.text) else {
                return false;
            };
            let mut key: Vec<String> = diffs
                .iter()
                .map(|d| format!("{}>{}", d.from_name, d.to_name))
                .collect();
            // JS `.sort()` on ASCII identifier strings: a byte sort.
            key.sort();
            match &canonical {
                None => canonical = Some(key),
                Some(c) if *c != key => return false,
                Some(_) => {}
            }
        }
    }
    true
}

fn emit_shape_votes(
    analysis: &mut HunkAnalysis,
    sk: &[u16],
    priors: &[Vec<u16>],
    infos: &[PoolLine],
) {
    for (k, info) in infos.iter().enumerate() {
        if analysis.noise_lines.contains_key(&info.line) {
            continue;
        }
        let Some(diffs) = compare_line_pair(&priors[k], &info.text) else {
            continue;
        };
        if diffs.is_empty() {
            continue;
        }
        for d in &diffs {
            analysis.candidates.push(PositionCandidate {
                line: info.line,
                col: d.col,
                from_name: d.from_name.clone(),
                to_name: d.to_name.clone(),
                hunk_index: info.hunk_index,
                skeleton: Some(sk.to_vec()),
            });
        }
        analysis.noise_lines.insert(
            info.line,
            NoiseLineInfo {
                hunk_index: info.hunk_index,
                diffs,
            },
        );
    }
}

/// `priorTooDissimilar` (the corpus gate).
pub fn prior_too_dissimilar(hunks: &[DiffHunk], prior_line_count: Option<usize>) -> bool {
    const MIN_CORPUS_LINES: usize = 8;
    const MIN_CORPUS_SIMILARITY: f64 = 0.5;
    let Some(count) = prior_line_count.filter(|&c| c >= MIN_CORPUS_LINES) else {
        return false;
    };
    let changed: usize = hunks.iter().map(|h| h.prior_lines.len()).sum();
    // JS number arithmetic: `unchanged` may go negative.
    let unchanged = count as f64 - changed as f64;
    unchanged / (count as f64) < MIN_CORPUS_SIMILARITY
}
