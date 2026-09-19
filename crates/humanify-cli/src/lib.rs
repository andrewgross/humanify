//! The CLI crate (02 §2): argv-to-config translation, the ONE
//! env/kill-switch-reading module, exit codes / ERROR blocks / progress UI.
//! `humanify-core` receives config as values and never touches
//! `std::env` — the guard test in this crate enforces it.

pub mod env;
pub mod kill_switches;
pub mod log;
pub mod util;

#[cfg(test)]
mod kill_switches_test;
