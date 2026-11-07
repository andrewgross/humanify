//! The eval's KPIs, defined ONCE (TS: `034/kpis.ts`): each column's key,
//! where its per-pair value lives in a card, its field in `summary.json`'s
//! `totals`, and its DIRECTION — data, because the eval's most expensive
//! mistakes have been direction mistakes (a column that must HOLD reads
//! exactly like one that must FALL once it is digits). The summary, the
//! leaderboard and the bands all read this table.
//!
//! Scorer v2 keeps every KPI's name and meaning; only the statement hash
//! behind the statement-keyed columns changed (README, "Definitions").

use std::sync::LazyLock;

use humanify_ag_model::js::JsValue;

use super::card::number_at;

/// What a move in a KPI means.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    /// Reducible noise: down is the point.
    Lower,
    /// Coverage: up is good.
    Higher,
    /// REAL CODE CHANGE: must not move in either direction.
    Hold,
    /// Reported; a move means nothing on its own.
    Context,
}

impl Direction {
    /// The header mark.
    pub fn mark(self) -> &'static str {
        match self {
            Direction::Lower => "↓",
            Direction::Higher => "↑",
            Direction::Hold => "=",
            Direction::Context => "~",
        }
    }
}

/// The `count (pct%)` denominator of a column.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Denominator {
    Statements,
    Names,
}

#[derive(Clone, Debug)]
pub struct Kpi {
    /// Column header, and its name in every write-up.
    pub key: String,
    /// Field in `summary.json`'s `totals`.
    pub total: String,
    /// The card's dotted path to the per-pair value.
    pub path: String,
    pub direction: Direction,
    pub denominator: Option<Denominator>,
    /// Why a naive reading misleads.
    pub caveat: Option<&'static str>,
}

impl Kpi {
    /// The per-pair value; None when the card did not score it.
    pub fn from_card(&self, card: &JsValue) -> Option<f64> {
        number_at(card, &self.path)
    }

    /// The label total; None when no card carried it.
    pub fn from_totals(&self, totals: &JsValue) -> Option<f64> {
        match totals.as_object()?.get(&self.total)? {
            JsValue::Number(n) => Some(*n),
            _ => None,
        }
    }
}

fn kpi(key: &str, total: &str, path: &str, direction: Direction) -> Kpi {
    Kpi {
        key: key.into(),
        total: total.into(),
        path: path.into(),
        direction,
        denominator: None,
        caveat: None,
    }
}

fn with_denominator(mut k: Kpi, d: Denominator) -> Kpi {
    k.denominator = Some(d);
    k
}

fn with_caveat(mut k: Kpi, c: &'static str) -> Kpi {
    k.caveat = Some(c);
    k
}

use Direction::{Context, Higher, Hold, Lower};

/// The headline KPIs.
pub static KPIS: LazyLock<Vec<Kpi>> = LazyLock::new(|| {
    vec![
        with_denominator(
            kpi(
                "clean",
                "unchangedClean",
                "churn.statements.unchangedClean",
                Higher,
            ),
            Denominator::Statements,
        ),
        with_denominator(
            kpi(
                "noise",
                "unchangedChurned",
                "churn.statements.unchangedChurned",
                Lower,
            ),
            Denominator::Statements,
        ),
        with_denominator(
            kpi("novel", "novel", "churn.statements.novel", Hold),
            Denominator::Statements,
        ),
        with_caveat(
            kpi(
                "noiseLn",
                "namingNoiseLines",
                "churn.lines.namingNoiseLines",
                Lower,
            ),
            "scores the BUNDLE, not the split tree. It was structurally pinned at 0 for the \
             whole 054 arc — a pass that only edits split files cannot move it, however much \
             churn it removes. Read `layout.churnLines` for what a reviewer of the tree sees, \
             and never conclude 'no effect' from this column alone.",
        ),
        with_caveat(
            kpi("realLn", "realLines", "churn.lines.realLines", Hold),
            "realLn is STATEMENT MASS — one edited line inside a 5k-line statement charges the \
             whole statement, so it overstates edited lines several-fold",
        ),
        with_caveat(
            with_denominator(
                kpi(
                    "reloc",
                    "sameNameMovedFile",
                    "churn.relocations.sameNameMovedFile",
                    Context,
                ),
                Denominator::Names,
            ),
            "reloc is NAME-keyed, so restoring a statement to its correct file MOVES the \
             recycled name and charges for it — it rose on all three experiments that cut \
             relocation 91%. Read relocation-churn.ts instead",
        ),
        kpi(
            "relocSt",
            "relocatedStatements",
            "churn.tree.relocatedStatements",
            Lower,
        ),
        with_denominator(
            kpi(
                "newName",
                "novelNames",
                "churn.relocations.novelNames",
                Lower,
            ),
            Denominator::Names,
        ),
        kpi(
            "mints",
            "mintedLeftovers",
            "determinism.mintedLeftovers",
            Lower,
        ),
        with_caveat(
            kpi(
                "treeLn",
                "layoutChurnLines",
                "churn.layout.churnLines",
                Lower,
            ),
            "total tree churn, real change INCLUDED — it falls when noise falls but also when \
             upstream simply changed less, so read it beside novel/realLn rather than as a \
             noise column on its own",
        ),
        with_caveat(
            kpi(
                "cleanLn",
                "layoutChurnLinesClean",
                "churn.layout.churnLinesClean",
                Lower,
            ),
            "the CLEAN diff: treeLn minus build-metadata inlining and spelling-only wrapper \
             re-serializations; the raw columns are unchanged and stay the historical trend \
             line. Read it beside treeLn and novel/realLn, exactly like treeLn — it too falls \
             when upstream changed less. A '-' means the label predates 2026-10-02 (or was \
             scored without layout), not a clean diff of zero",
        ),
        with_caveat(
            kpi(
                "cleanReal",
                "layoutRealClean",
                "churn.layout.realClean",
                Hold,
            ),
            "real change minus the soft categories (realExBuild − spellingIdenticalLines) — \
             must not move in either direction, exactly like realLn/vendorReal; a lever that \
             moves it has dropped real change or started swallowing it",
        ),
        with_caveat(
            kpi("reorderLn", "layoutReorder", "churn.layout.reorder", Lower),
            "invisible to every statement-keyed column: the eval matches by hash, so a \
             byte-identical statement emitted elsewhere costs nothing there",
        ),
        with_caveat(
            kpi("vendorLn", "vendorNoise", "churn.vendor.noise", Lower),
            "vendor was UNSCORED before exp046, so older references print `-`, which is not 0. \
             This counts only reducible churn (manifest + bodies that differ solely in \
             minifier-rerolled local names); read it next to vendorReal, never instead of it",
        ),
        with_caveat(
            kpi("vendorReal", "vendorReal", "churn.vendor.real", Hold),
            "genuine dependency change — libraries added, removed, or actually edited. A vendor \
             'win' that moves this has dropped real change, which is the one failure the \
             vendor columns exist to catch",
        ),
    ]
});

/// One split column: `<prefix>:<key>` reading
/// `churn.surfaces.<axis>.<surface>.<field>`.
fn split(surface: &str, key: &str, axis: &str, field: &str, d: Direction) -> Kpi {
    let prefix = if surface == "crossBoundary" {
        "cross"
    } else {
        surface
    };
    kpi(
        &format!("{prefix}:{key}"),
        &format!("split.{axis}.{surface}.{field}"),
        &format!("churn.surfaces.{axis}.{surface}.{field}"),
        d,
    )
}

fn mint(part: &str, key: &str) -> Kpi {
    kpi(
        key,
        &format!("split.mints.{part}"),
        &format!("churn.surfaces.mints.{part}"),
        Lower,
    )
}

fn axis(surface: &str) -> Vec<Kpi> {
    vec![
        split(surface, "noise", "statements", "unchangedChurned", Lower),
        split(surface, "noiseLn", "statements", "namingNoiseLines", Lower),
        split(surface, "novel", "statements", "novel", Hold),
        split(surface, "realLn", "statements", "realLines", Hold),
        split(surface, "reloc", "names", "sameNameMovedFile", Context),
        split(surface, "relocSt", "tree", "relocatedStatements", Lower),
        split(surface, "newName", "names", "novelNames", Lower),
    ]
}

/// The APP / VENDOR / CROSS-BOUNDARY views (`app:X + vendor:X + cross:X =
/// X`; mints add `mints:unattributed`).
pub static SPLIT_KPIS: LazyLock<Vec<Kpi>> = LazyLock::new(|| {
    let mut out = vec![
        split("app", "treeLn", "lines", "churnLines", Lower),
        split("app", "cleanLn", "lines", "churnLinesClean", Lower),
        split("app", "cleanReal", "lines", "realClean", Hold),
        split("app", "reorderLn", "lines", "reorder", Lower),
    ];
    out.extend(axis("app"));
    out.push(mint("app", "app:mints"));
    out.push(split("vendor", "treeLn", "lines", "churnLines", Lower));
    out.push(split("vendor", "vendorLn", "lines", "noise", Lower));
    out.push(with_caveat(
        split("vendor", "vendorReal", "lines", "real", Hold),
        "vendorReal with the statements that moved INTO (or out of) vendor/ taken out — those \
         are cross:treeLn. The old vendorReal charged them as dependency change (exp168: 352 \
         of 506 on exp164)",
    ));
    out.extend(axis("vendor").into_iter().map(|k| {
        if k.key == "vendor:noise" {
            with_caveat(
                k,
                "the vendor:<statement-axis> columns are the BUNDLE statements (app code and \
                 ESM-inlined library code; never CJS library factories) the pipeline filed \
                 into vendor/",
            )
        } else {
            k
        }
    }));
    out.push(mint("vendor", "vendor:mints"));
    out.push(with_caveat(
        split("crossBoundary", "treeLn", "lines", "churnLines", Context),
        "git lines of statements that crossed the src/<->vendor/ boundary, paired by content \
         and charged ONCE here instead of as an app removal plus a vendor addition. A move is \
         not upstream change, but a correct one-time move of library code out of src/ is not a \
         regression either: read the listed moves",
    ));
    out.push(split(
        "crossBoundary",
        "stmts",
        "statements",
        "total",
        Context,
    ));
    out.push(split(
        "crossBoundary",
        "relocSt",
        "tree",
        "relocatedStatements",
        Context,
    ));
    out.push(split(
        "crossBoundary",
        "reloc",
        "names",
        "sameNameMovedFile",
        Context,
    ));
    out.push(mint("unattributed", "mints:unattributed"));
    out
});

/// The KPIs a tool shows, in order. An unknown key is a programming error.
pub fn named(keys: &[&str]) -> Vec<&'static Kpi> {
    keys.iter()
        .map(|k| {
            KPIS.iter()
                .chain(SPLIT_KPIS.iter())
                .find(|x| x.key == *k)
                .unwrap_or_else(|| panic!("unknown KPI {k:?} — see eval/kpis.rs"))
        })
        .collect()
}

/// One line per shown KPI whose direction a reader could get wrong.
pub fn caveat_lines(shown: &[&Kpi]) -> Vec<String> {
    shown
        .iter()
        .filter_map(|k| Some(format!("  {}: {}", k.key, k.caveat?)))
        .collect()
}
