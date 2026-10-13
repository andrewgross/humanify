//! The CLI crate (02 §2): the ONE parse point (`surface` declares the
//! options, `config` resolves and validates them into the run's typed
//! `Config`), the ONE environment reader (`env`), exit codes / ERROR
//! blocks / progress UI. Below the parse point nothing reads argv, the
//! environment or a file for a setting — `tests/env_guard.rs` enforces it.

pub mod config;
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
pub mod split_stage;
pub mod surface;
pub mod unified;
pub mod unminify;
pub mod util;
pub mod writers;

/// The pipeline program's entry (`humanify <input> [options]`): parse
/// (`surface`, clap), resolve and validate the `Config` (`config`), run,
/// return the process exit code. Help and version print to stdout and exit
/// 0; a usage error, or any refusal of the configuration, prints to stderr
/// and exits 1 — before anything is read, created or asked.
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
        surface::PipelineParse::Run { input, opts } => {
            match config::Config::from_process(&input, &opts) {
                Ok(config) => unified::run(&config),
                Err(refusals) => {
                    for m in refusals {
                        eprintln!("Error: {m}");
                    }
                    1
                }
            }
        }
    }
}

#[cfg(test)]
mod config_file_test;
#[cfg(test)]
mod config_test;
#[cfg(test)]
mod discover_verb_test;
#[cfg(test)]
mod kill_switches_test;
#[cfg(test)]
mod launch_parity_test;
#[cfg(test)]
mod match_verb_test;
#[cfg(test)]
mod surface_test;
#[cfg(test)]
mod vectors_test;
#[cfg(test)]
mod writers_test;
