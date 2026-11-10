//! The 034 eval's KPI scorer, scorer version 2 (harness lane L2a): the
//! statement identity, the per-pair scorecard and everything that reads
//! cards (summary, leaderboard, bands). Version 1 was the TS scorer
//! (`experiments/034-eval-harness/`); its cards read as version 1.

pub mod analyze;
pub mod bands;
pub mod card;
pub mod compose;
pub mod difftool;
pub mod kpis;
pub mod leaderboard;
pub mod line_churn;
pub mod line_tokens;
pub mod llm_diag;
pub mod matching;
pub mod real_ledger;
pub mod report;
pub mod rescore;
pub mod stmts;
pub mod summarize;
pub mod surfaces;
pub mod text;
pub mod vendor;
pub mod verbs;
