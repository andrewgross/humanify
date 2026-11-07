//! The scorer's verbs (`cargo dev eval rescore|summarize|leaderboard|bands|
//! diff`): each takes its parsed command line and returns (exit code,
//! stdout, stderr), like every runner in main.rs.

use std::path::PathBuf;

use crate::settings::Parsed;

type Out = (u8, String, String);

fn fail(verb: &str, e: impl std::fmt::Display) -> Out {
    (1, String::new(), format!("eval {verb}: {e}\n"))
}

fn usage(u: &str) -> Out {
    (
        2,
        String::new(),
        format!("usage: humanify-ag-dev eval {u}\n"),
    )
}

/// Where the labels live: `--results`, else the committed results dir.
pub fn results_dir(parsed: &Parsed) -> PathBuf {
    parsed
        .settings
        .text("scorer.results")
        .map(PathBuf::from)
        .unwrap_or_else(|| crate::paths::repo_root().join(crate::paths::EVAL_RESULTS))
}

pub fn rescore(parsed: &Parsed) -> Out {
    let u = "rescore <label> --cards <dir> --work <dir> [--results <dir>]";
    let ([label], Some(cards), Some(work)) = (
        parsed.positionals.as_slice(),
        parsed.settings.text("scorer.cards"),
        parsed.settings.text("scorer.work"),
    ) else {
        return usage(u);
    };
    let out = results_dir(parsed).join(label);
    match super::rescore::rescore(&PathBuf::from(cards), &PathBuf::from(work), &out) {
        Err(e) => fail("rescore", e),
        Ok((lines, ok)) => {
            let mut text: String = lines.iter().map(|l| format!("{label} {l}\n")).collect();
            match summarize_label(&out, label) {
                Ok(s) => text.push_str(&s),
                Err(e) => return (1, text, format!("eval rescore: summarize: {e}\n")),
            }
            (u8::from(!ok), text, String::new())
        }
    }
}

fn summarize_label(dir: &std::path::Path, label: &str) -> Result<String, String> {
    let reference = crate::committed::eval::self_hop_reference(&crate::paths::repo_root())?;
    super::summarize::summarize(dir, label, &reference.value)
}

pub fn summarize(parsed: &Parsed) -> Out {
    let [label] = parsed.positionals.as_slice() else {
        return usage("summarize <label> [--results <dir>]");
    };
    match summarize_label(&results_dir(parsed).join(label), label) {
        Ok(s) => (0, s, String::new()),
        Err(e) => fail("summarize", e),
    }
}

pub fn leaderboard(parsed: &Parsed) -> Out {
    let bands = crate::committed::eval::noise_bands(&crate::paths::repo_root());
    let bands = match &bands {
        Ok(b) => Some(&b.value),
        Err(e) => return fail("leaderboard", e),
    };
    match super::leaderboard::leaderboard(
        &results_dir(parsed),
        &parsed.positionals,
        bands,
        parsed.settings.flag_on("run.force_mixed"),
    ) {
        Ok(s) => (0, s, String::new()),
        Err(e) => (2, String::new(), format!("eval leaderboard: {e}\n")),
    }
}

pub fn bands(parsed: &Parsed) -> Out {
    let file = crate::paths::repo_root()
        .join(crate::paths::EVAL_DIR)
        .join("noise-bands.json");
    match super::bands::bands(&results_dir(parsed), &parsed.positionals, &file) {
        Ok(s) => (0, s, String::new()),
        Err(e) => (2, String::new(), format!("eval bands: {e}\n")),
    }
}

pub fn diff(parsed: &Parsed) -> Out {
    let (prior, fresh, label) = match parsed.positionals.as_slice() {
        [p, f] => (p, f, ""),
        [p, f, l] => (p, f, l.as_str()),
        _ => return usage("diff <priorTree> <freshTree> [label]"),
    };
    // A tree root holding src/ means its src/.
    let src = |a: &str| {
        let with = PathBuf::from(a).join("src");
        if with.is_dir() {
            with
        } else {
            PathBuf::from(a)
        }
    };
    match super::real_ledger::real_ledger(&src(prior), &src(fresh), label) {
        Ok(s) => (0, s, String::new()),
        Err(e) => fail("diff", e),
    }
}
