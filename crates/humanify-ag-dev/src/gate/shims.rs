//! `cargo dev shims`: syntax check of the remaining JS shims (the webcrack shim).

use crate::HostEnv;
use crate::settings::Parsed;

/// Run the verb. (Skeleton: filled in by its L4 sub-lane.)
pub fn run(_p: &Parsed, _host: &HostEnv) -> (u8, String, String) {
    crate::under_construction("shims")
}
