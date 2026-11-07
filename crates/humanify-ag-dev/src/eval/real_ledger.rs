//! `eval diff <priorTree> <freshTree>` (TS: `055/real-ledger.ts` over
//! `037/diff-composition.ts`): what the on-disk diff of two split trees is
//! made of. A. what is inside the composition's REAL column — every edited
//! pair's billed lines classified NAME-ONLY (identifier tokens differ and
//! nothing else, `line_tokens::name_only`) or EDITED, one-sided statements
//! counted apart; B. does the decomposition add up — every common file's
//! modelled charge against GNU `diff`'s line count, in aggregate and per
//! file (a total that matches by cancellation says nothing per file).

use std::collections::HashSet;
use std::path::Path;

use humanify_ag_model::js::to_fixed;

use super::compose::{SampleKind, compose_diff, compose_file};
use super::difftool::Differ;
use super::line_tokens::name_only;
use super::text::{list_js, read};

/// The billed lines of an edited pair (set-based, as the tally bills
/// them), paired positionally; the unpaired rest is real change.
fn billed_pairs<'a>(prior: &'a str, fresh: &'a str) -> (Vec<(&'a str, &'a str)>, usize) {
    let fl: Vec<&str> = fresh.split('\n').collect();
    let pl: Vec<&str> = prior.split('\n').collect();
    let ps: HashSet<&str> = pl.iter().copied().collect();
    let fs: HashSet<&str> = fl.iter().copied().collect();
    let added: Vec<&str> = fl.iter().copied().filter(|l| !ps.contains(l)).collect();
    let removed: Vec<&str> = pl.iter().copied().filter(|l| !fs.contains(l)).collect();
    let k = added.len().min(removed.len());
    let pairs = (0..k).map(|i| (removed[i], added[i])).collect();
    (pairs, added.len() + removed.len() - 2 * k)
}

fn pct(n: f64, d: f64) -> String {
    if d != 0.0 {
        format!("{}%", to_fixed(100.0 * n / d, 1))
    } else {
        "n/a".into()
    }
}

/// The ledger's text for two src/ dirs.
pub fn real_ledger(prior: &Path, fresh: &Path, label: &str) -> Result<String, String> {
    let comp = compose_diff(prior, fresh, true)?;
    let t = comp.tally;
    let (mut name_ln, mut edited_ln, mut one_sided) = (0usize, 0usize, 0usize);
    let mut examples = Vec::new();
    for s in comp.samples.iter().filter(|s| s.kind == SampleKind::Real) {
        let (Some(p), Some(f)) = (&s.prior_text, &s.fresh_text) else {
            one_sided += s.lines;
            continue;
        };
        let (pairs, unpaired) = billed_pairs(p, f);
        edited_ln += unpaired;
        for (a, b) in pairs {
            if name_only(a, b) {
                name_ln += 2;
                if examples.len() < 25 {
                    let cut = |x: &str| x.trim().chars().take(150).collect::<String>();
                    examples.push(format!(
                        "{}\n      - {}\n      + {}",
                        s.file,
                        cut(a),
                        cut(b)
                    ));
                }
            } else {
                edited_ln += 2;
            }
        }
    }
    let differ = Differ;
    let prior_files: HashSet<String> = list_js(prior)?.into_iter().collect();
    let (mut truth, mut modelled, mut abs_err) = (0i64, 0i64, 0i64);
    let mut per_file: Vec<f64> = Vec::new();
    for f in list_js(fresh)? {
        if !prior_files.contains(&f) {
            continue;
        }
        let (a, b) = (read(&prior.join(&f))?, read(&fresh.join(&f))?);
        if a == b {
            continue;
        }
        let g = differ.changed_lines(&prior.join(&f), &fresh.join(&f))? as i64;
        let c = compose_file(&a, &b, &f)?;
        let m = (c.real + c.naming + c.alias + c.reorder + c.file_add_remove) as i64;
        truth += g;
        modelled += m;
        abs_err += (m - g).abs();
        if g > 0 {
            per_file.push((m - g).abs() as f64 / g as f64);
        }
    }
    per_file.sort_by(f64::total_cmp);
    let median = per_file.get(per_file.len() / 2).copied().unwrap_or(0.0);
    let p90 = per_file
        .get(per_file.len() * 9 / 10)
        .copied()
        .unwrap_or(0.0);
    let noise = t.naming + t.alias + t.reorder;
    let real = t.real as f64;
    let w = |n: usize| format!("{n:>9}");
    let title = if label.is_empty() {
        format!("{} -> {}", prior.display(), fresh.display())
    } else {
        label.to_string()
    };
    let mut out = vec![
        format!("=== REAL-COLUMN LEDGER — {title} ==="),
        format!("  composeDiff REAL total          {}", w(t.real)),
        format!(
            "    NAME-ONLY (hidden noise)      {}  {}",
            w(name_ln),
            pct(name_ln as f64, real)
        ),
        format!(
            "    EDITED (genuine)              {}  {}",
            w(edited_ln),
            pct(edited_ln as f64, real)
        ),
        format!(
            "    one-sided add/remove          {}  {}",
            w(one_sided),
            pct(one_sided as f64, real)
        ),
        format!("  classified noise                {}", w(noise)),
        format!(
            "ROW|{label}|real|{}|{name_ln}|{edited_ln}|{one_sided}|{noise}",
            t.real
        ),
        String::new(),
        "  === accounting vs GNU diff ===".into(),
        format!("  ground truth (diff)             {truth:>9}"),
        format!(
            "  decomposition total             {modelled:>9}   ({} off)",
            pct((modelled - truth) as f64, truth as f64)
        ),
        format!(
            "  sum of |per-file error|         {abs_err:>9}   cancels {}x",
            if truth != 0 {
                to_fixed(abs_err as f64 / (modelled - truth).abs().max(1) as f64, 1)
            } else {
                "n/a".into()
            }
        ),
        format!(
            "  per-file relative error: median {}%, p90 {}%, files {}",
            to_fixed(100.0 * median, 1),
            to_fixed(100.0 * p90, 1),
            per_file.len()
        ),
        format!(
            "ROW|{label}|accounting|{truth}|{modelled}|{abs_err}|{}|{}",
            to_fixed(100.0 * median, 1),
            to_fixed(100.0 * p90, 1)
        ),
        format!(
            "  composition: real {} (spelling-identical {} inside it) · naming {} · alias {} · reorder {} · files added/removed {}",
            t.real, t.spelling_identical_lines, t.naming, t.alias, t.reorder, t.file_add_remove
        ),
        "\n  NAME-ONLY lines inside REAL statements — first 25:".into(),
    ];
    out.extend(examples.into_iter().map(|e| format!("    {e}")));
    Ok(out.join("\n") + "\n")
}

#[cfg(test)]
#[path = "real_ledger_test.rs"]
mod real_ledger_test;
