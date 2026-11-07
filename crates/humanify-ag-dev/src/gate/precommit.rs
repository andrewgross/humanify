//! `cargo dev precommit`: the git pre-commit hook (successor of husky + lint-staged).

use crate::settings::Parsed;

/// Run the verb. (Skeleton: filled in by its L4 sub-lane.)
pub fn run(_p: &Parsed) -> (u8, String, String) {
    crate::under_construction("precommit")
}
