//! `cargo dev env-check`: the toolchain pin (successor of scripts/rust-env-check.sh).

use crate::HostEnv;
use crate::settings::Parsed;

/// Run the verb. (Skeleton: filled in by its L4 sub-lane.)
pub fn run(_p: &Parsed, _host: &HostEnv) -> (u8, String, String) {
    crate::under_construction("env-check")
}
