//! The parity differ's CLI (docs/rust-port/07-differential-validation.md §4):
//!
//!   humanify-parity compare <left-dump> <right-dump> [--sections a,b] [--max-divergences N]
//!   humanify-parity selftest
//!
//! Both dumps are the binary's own `--dump-artifacts` output (the TS side
//! was retired at the cutover): two runs that should agree — a determinism
//! check, or a should-change-nothing refactor over a warm cache.
//!
//! Exit codes: 0 identical, 1 divergences, 2 not comparable. Every gate is
//! exact set or byte equality — no tolerance parameter exists (07 §9).

use clap::{Parser, Subcommand};
use humanify_parity::{engine, selftest};

#[derive(Parser)]
#[command(name = "humanify-parity", version, about = "The parity differ (07 §4)")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Compare two artifact dumps section by section.
    Compare {
        /// Left dump directory (the reference side).
        left: String,
        /// Right dump directory (the candidate side).
        right: String,
        /// Comma-separated section list (default: all).
        #[arg(long)]
        sections: Option<String>,
        /// Cap on reported divergences.
        #[arg(long, default_value_t = 20)]
        max_divergences: usize,
    },
    /// Prove the instrument can fail: planted divergences must be detected.
    Selftest,
}

fn main() {
    let cli = Cli::parse();
    let code = match cli.command {
        Command::Selftest => match selftest::run_selftest() {
            Ok(()) => {
                println!(
                    "selftest: every planted divergence detected (control identical, {} planted cases + control)",
                    selftest::planted_case_count()
                );
                0
            }
            Err(msg) => {
                eprintln!("SELFTEST FAILED: {msg}");
                1
            }
        },
        Command::Compare {
            left,
            right,
            sections,
            max_divergences,
        } => run_compare(
            &left,
            &right,
            sections.as_deref().unwrap_or(""),
            max_divergences,
        ),
    };
    std::process::exit(code);
}

fn run_compare(left: &str, right: &str, sections_spec: &str, max_divergences: usize) -> i32 {
    let sections = match engine::parse_sections(sections_spec) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("BAD ARGUMENTS: {e}");
            return 2;
        }
    };
    let outcome = match engine::compare_dumps(
        std::path::Path::new(left),
        std::path::Path::new(right),
        &sections,
        max_divergences,
    ) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("NOT COMPARABLE: {e}");
            return 2;
        }
    };
    if let Some(reason) = &outcome.not_comparable_reason {
        eprintln!("NOT COMPARABLE: {reason}");
        return 2;
    }
    let real = outcome.divergences.len();
    if real == 0 {
        println!(
            "IDENTICAL: {} section(s) compared clean across {left} vs {right}",
            sections.len()
        );
        return 0;
    }
    println!("DIVERGED: {real} divergence(s) across {left} vs {right}:");
    for d in &outcome.divergences {
        println!("  [{}:{}] {}", d.section, d.kind, d.key);
        if let Some(l) = &d.left {
            println!("    left:  {l}");
        }
        if let Some(r) = &d.right {
            println!("    right: {r}");
        }
    }
    1
}
