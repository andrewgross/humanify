//! `cargo dev format-golden`: the formatter against its frozen goldens (successor of scripts/format-golden.sh).

use crate::HostEnv;
use crate::settings::Parsed;

/// Run the verb. (Skeleton: filled in by its L4 sub-lane.)
pub fn run(_p: &Parsed, _host: &HostEnv) -> (u8, String, String) {
    crate::under_construction("format-golden")
}
