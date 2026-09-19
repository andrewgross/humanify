//! The humanify binary (TS: src/index.ts + src/cli.ts, ports WP1.1/WPB.4).
//! WP1.1 lands the scaffolding: the versioned clap program shell. The
//! pipeline command arrives at WPB.4; the showHelpAfterError behavior is
//! clap's own error handling plus usage-on-error (clap 4 default).

use clap::Parser;

#[derive(Parser)]
#[command(
    name = "humanify",
    version,
    about = "Deobfuscate a minified JavaScript bundle into a readable, version-stable source tree"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(clap::Subcommand)]
enum Command {
    /// The pipeline command — the ported pipeline lands at WPB.4; the flag
    /// surface is the pipeline contract (docs/rust-port/14-pipeline-contract.md).
    Run,
    /// WP1.2's ingest gate: parse the given TS-beautified text with oxc,
    /// print the symbol/scope/reference counts. (Migration scaffolding —
    /// deleted at phase 6 with the TS core, 02 §9.)
    Ingest {
        /// The TS-beautified text to parse.
        beautified_input: String,
    },
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Some(Command::Run) => {
            // Placeholder until WPB.4 ports the command surface.
            println!("humanify run: the pipeline command arrives at WPB.4");
        }
        Some(Command::Ingest { beautified_input }) => {
            let text = match std::fs::read_to_string(&beautified_input) {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("Error: cannot read {beautified_input}: {e}");
                    std::process::exit(1);
                }
            };
            let (counts, errors) =
                humanify_core::ingest::ingest_counts_of_file(&text, &beautified_input);
            if !errors.is_empty() {
                for e in errors.iter().take(5) {
                    eprintln!("ERROR: {e}");
                }
                eprintln!(
                    "ERROR: oxc failed to parse {}: {} diagnostic(s) — this run is marked failed.",
                    beautified_input,
                    errors.len()
                );
                std::process::exit(1);
            }
            println!("{}", serde_json::to_string(&counts).unwrap());
        }
        None => {
            // No subcommand: print help (commander's behavior with a
            // required argument is the same shape).
            Cli::command().print_help().expect("help should print");
        }
    }
}

#[allow(unused_imports)]
use clap::CommandFactory;
