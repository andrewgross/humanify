//! The CLI crate (02 §2): argv-to-config translation, the ONE
//! env/kill-switch-reading module, exit codes / ERROR blocks / progress UI.
//! `humanify-core` receives config as values and never touches
//! `std::env` — the guard test in this crate enforces it.

pub mod commander;
pub mod env;
pub mod env_reads;
pub mod failed_output;
pub mod kill_switches;
pub mod log;
pub mod output_validation;
pub mod pipeline_config;
pub mod progress;
pub mod report;
pub mod settings;
pub mod split_stage;
pub mod surface;
pub mod unified;
pub mod unminify;
pub mod util;
pub mod writers;

/// The pipeline program's entry (`humanify <input>` / `humanify env-reads`):
/// parse with the commander-grammar surface, run the action, return the
/// process exit code. Help, version and usage errors print exactly what
/// commander prints, where it prints it.
pub fn pipeline_main(argv: &[String]) -> i32 {
    use commander::ParseOutcome;
    match surface::program().parse(argv) {
        ParseOutcome::Exit {
            exit_code,
            stdout,
            stderr,
            ..
        } => {
            use std::io::Write;
            let _ = std::io::stdout().write_all(stdout.as_bytes());
            let _ = std::io::stderr().write_all(stderr.as_bytes());
            exit_code
        }
        ParseOutcome::Action {
            command,
            args,
            opts,
        } => match command.as_str() {
            "env-reads" => env_reads::run(
                &args[0],
                opts.bool("markdown") == Some(true),
                opts.str("output"),
            ),
            _ => unified::run(&args[0], &opts),
        },
    }
}

#[cfg(test)]
mod env_reads_test;
#[cfg(test)]
mod kill_switches_test;
#[cfg(test)]
mod settings_test;
#[cfg(test)]
mod surface_test;
#[cfg(test)]
mod util_test;
#[cfg(test)]
mod vectors_test;
