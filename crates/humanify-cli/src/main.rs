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
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Some(Command::Run) => {
            // Placeholder until WPB.4 ports the command surface.
            println!("humanify run: the pipeline command arrives at WPB.4");
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
