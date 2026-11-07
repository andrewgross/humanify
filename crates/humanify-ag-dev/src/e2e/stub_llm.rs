//! `cargo dev stub-llm serve`: the one stub LLM as a process (successor of scripts/lib/stub-llm.ts).

use crate::HostEnv;
use crate::settings::Parsed;

/// Run the verb. (Skeleton: filled in by its L4 sub-lane.)
pub fn run(_p: &Parsed, _host: &HostEnv) -> (u8, String, String) {
    crate::under_construction("stub-llm serve")
}
