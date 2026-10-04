//! The ONE owner of "does this prompt show what it asks about"
//! (2026-10-04, fix/sweep-sees-its-targets).
//!
//! Every ask names identifiers and shows the model some code. When an
//! asked identifier is not IN the code shown, the model can only guess:
//! the coverage sweep showed the first 500 lines of a multi-thousand-line
//! wrapper, and 300 of 306 sweep asks across 16 base builds named an
//! identifier the model never saw — ~222 came back as confident made-up
//! names (`squareRoot` for an S3-header setup function), every one checked
//! wrong, invisible to the leftover meter and carried into later
//! versions. Precision first: an honest minified name beats a made-up
//! one, so an identifier the shown code does not contain is NOT asked,
//! and an answer for it is never applied.
//!
//! "Shown" is a whole-token occurrence in the shown text
//! ([`crate::naming::code_window::line_has_identifier`], the one owner of
//! the token rule). Each prompt site states WHAT it shows for an
//! identifier (a function window, a module binding's declaration +
//! assignments + usages) and asks this module; a site whose cut can drop
//! a subject either windows around it by construction or refuses it here.

use crate::naming::code_window::line_has_identifier;

/// The trail / diagnostics reason recorded on an identifier the guard
/// refused (it was never put to the model).
pub const NOT_SHOWN: &str = "not-shown";

/// Whether `shown` holds `id` as a whole identifier token.
pub fn shows(shown: &str, id: &str) -> bool {
    !id.is_empty() && line_has_identifier(shown, id)
}

/// The asked identifiers `shown` does NOT contain, in ask order.
pub fn unshown<'a>(shown: &str, asked: &'a [String]) -> Vec<&'a str> {
    asked
        .iter()
        .map(String::as_str)
        .filter(|id| !shows(shown, id))
        .collect()
}

/// Fail loud in debug builds and tests: a prompt site that guarantees its
/// subjects by construction asserts it here. Release builds count the
/// miss at the site (the stats' `promptGuard` block) instead of crashing
/// a run — a non-zero count is a finding.
pub fn debug_assert_all_shown(site: &str, shown: &str, asked: &[String]) {
    debug_assert!(
        unshown(shown, asked).is_empty(),
        "prompt guard ({site}): asked {:?} but the shown code does not contain them",
        unshown(shown, asked)
    );
}

/// How many unshown examples a tally keeps.
const EXAMPLES: usize = 20;

/// The wave asks' guard measurement, per prompt site: identifiers asked,
/// and how many of them the shown code did NOT contain. Every site windows
/// its subjects by construction, so a non-zero `*_unshown` is a finding;
/// `examples` names the first few (`site:name`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GuardTally {
    pub fn_asked: usize,
    pub fn_unshown: usize,
    pub retry_asked: usize,
    pub retry_unshown: usize,
    pub module_asked: usize,
    pub module_unshown: usize,
    pub examples: Vec<String>,
}

/// The wave prompt sites the tally splits by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Site {
    /// A function's first-round ask (`code_window` selection).
    Fn,
    /// A function's retry ask (the retry snippet).
    Retry,
    /// A module-level ask (declaration + assignments + usages).
    Module,
}

impl GuardTally {
    /// Record one ask's guard result.
    pub fn note(&mut self, site: Site, asked: usize, unshown: &[&str]) {
        let (a, u, tag) = match site {
            Site::Fn => (&mut self.fn_asked, &mut self.fn_unshown, "fn"),
            Site::Retry => (&mut self.retry_asked, &mut self.retry_unshown, "retry"),
            Site::Module => (&mut self.module_asked, &mut self.module_unshown, "module"),
        };
        *a += asked;
        *u += unshown.len();
        for id in unshown {
            if self.examples.len() < EXAMPLES {
                self.examples.push(format!("{tag}:{id}"));
            }
        }
    }
}

#[cfg(test)]
mod shown_test;
