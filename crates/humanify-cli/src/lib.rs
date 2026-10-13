//! The CLI crate (02 §2): argv-to-config translation, the ONE
//! env/kill-switch-reading module, exit codes / ERROR blocks / progress UI.
//! `humanify-core` receives config as values and never touches
//! `std::env` — the guard test in this crate enforces it.

pub mod discover_verb;
pub mod env;
pub mod failed_output;
pub mod kill_switches;
pub mod llm_sim;
pub mod log;
pub mod match_verb;
pub mod output_validation;
pub mod pipeline_config;
pub mod progress;
pub mod provider_failures;
pub mod report;
pub mod settings;
pub mod split_stage;
pub mod surface;
pub mod unified;
pub mod unminify;
pub mod util;
pub mod writers;

/// The pipeline program's entry (`humanify <input> [options]`): parse
/// (`surface`, clap), run, return the process exit code. Help and version
/// print to stdout and exit 0; a usage error prints to stderr and exits 1.
pub fn pipeline_main(argv: &[String]) -> i32 {
    match surface::parse_pipeline(argv) {
        surface::PipelineParse::Exit {
            code,
            stdout,
            stderr,
        } => {
            use std::io::Write;
            let _ = std::io::stdout().write_all(stdout.as_bytes());
            let _ = std::io::stderr().write_all(stderr.as_bytes());
            code
        }
        surface::PipelineParse::Run { input, opts } => unified::run(&input, &opts),
    }
}

#[cfg(test)]
mod discover_verb_test;
#[cfg(test)]
mod kill_switches_test;
#[cfg(test)]
mod launch_parity_test;
#[cfg(test)]
mod match_verb_test;
#[cfg(test)]
mod settings_test;
#[cfg(test)]
mod surface_test;
#[cfg(test)]
mod vectors_test;
#[cfg(test)]
mod writers_test;
