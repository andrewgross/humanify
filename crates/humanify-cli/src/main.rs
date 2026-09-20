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
    /// WP1.3's partition gate: rebuild the statementHash partition from a
    /// TS dump's shipped text into a Rust-side dump dir, then diff with
    /// `humanify-parity compare --sections partitions`. (Migration
    /// scaffolding — deleted at phase 6.)
    Partitions {
        /// The TS dump directory (its meta.json + text/shipped.js).
        ts_dump: String,
        /// The Rust-side dump directory to write.
        out_dir: String,
    },
    /// WP1.4's graph gate: rebuild functions.json's kind=function rows
    /// (edges + scope parents + hashes) from a TS dump's shipped text.
    Functions { ts_dump: String, out_dir: String },
    /// WP1.5's module-boundary gate: rebuild the TS modules.json rows
    /// (helper var + wrapper + factory records) from a TS dump's fresh
    /// text. (Migration scaffolding — deleted at phase 6.)
    Modules { ts_dump: String, out_dir: String },
    /// WP2.3's twins gate: rebuild the TS twins.json rows (the two
    /// inventories + the unique-tier 1:1 join) from a TS dump's two texts.
    /// (Migration scaffolding — deleted at phase 6.)
    Twins { ts_dump: String, out_dir: String },
    /// WP2.1's matches gate: rebuild the TS matches.json rows (the two
    /// cascades over the dump's fresh + prior texts — the cascade is fully
    /// cold). (Migration scaffolding — deleted at phase 6.)
    Matches {
        ts_dump: String,
        out_dir: String,
        /// SIZING probe: visit optional calls (fix babel's blind spot).
        #[arg(long, default_value_t = false)]
        visit_optional: bool,
    },
    /// WP2.1 debugging: dump both sides' statement contexts (the
    /// enclosing-statement rung's evidence) as JSONL. (Migration
    /// scaffolding — deleted at phase 6.)
    Stmtctx { ts_dump: String, out_dir: String },
    /// WP2.1 debugging: canonical hash + token stream for the (side, span)
    /// graph-entry nodes in a spans JSON file. (Migration scaffolding —
    /// deleted at phase 6.)
    Hashprobe {
        ts_dump: String,
        spans: String,
        out: String,
    },
    /// WP2.1 debugging: the reference-identity evidence rows for the
    /// (side, span) function nodes in a spans JSON file — the raw resolved
    /// references joined against the matchable/holder identity maps.
    /// (Migration scaffolding — deleted at phase 6.)
    Refprobe {
        ts_dump: String,
        spans: String,
        out: String,
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
        Some(Command::Partitions { ts_dump, out_dir }) => {
            match humanify_core::hash::partition_dump::dump_partitions(
                std::path::Path::new(&ts_dump),
                std::path::Path::new(&out_dir),
            ) {
                Ok(count) => println!("partitions: {count} statement member(s) -> {out_dir}"),
                Err(e) => {
                    eprintln!("ERROR: {e}");
                    std::process::exit(1);
                }
            }
        }
        Some(Command::Functions { ts_dump, out_dir }) => {
            match humanify_core::graph::functions_dump::dump_functions(
                std::path::Path::new(&ts_dump),
                std::path::Path::new(&out_dir),
            ) {
                Ok(count) => println!("functions: {count} row(s) -> {out_dir}"),
                Err(e) => {
                    eprintln!("ERROR: {e}");
                    std::process::exit(1);
                }
            }
        }
        Some(Command::Twins { ts_dump, out_dir }) => {
            match humanify_core::twins::twins_dump::dump_twins(
                std::path::Path::new(&ts_dump),
                std::path::Path::new(&out_dir),
            ) {
                Ok(count) => println!("twins: {count} unique twin pair(s) -> {out_dir}"),
                Err(e) => {
                    eprintln!("ERROR: {e}");
                    std::process::exit(1);
                }
            }
        }
        Some(Command::Matches {
            ts_dump,
            out_dir,
            visit_optional,
        }) => {
            // The propagation trace's config comes through the ONE env
            // reader (02 §2); core never reads std::env itself.
            humanify_core::propagation::trace::configure(
                humanify_cli::env::get("HUMANIFY_MATCH_TRACE", None).is_some(),
                humanify_cli::env::get("HUMANIFY_MATCH_WATCH", None)
                    .map(|v| v.split(',').map(str::to_string).collect()),
            );
            match humanify_core::matching::matches_dump::dump_matches_opts(
                std::path::Path::new(&ts_dump),
                std::path::Path::new(&out_dir),
                visit_optional,
            ) {
                Ok(count) => println!("matches: {count} pair row(s) -> {out_dir}"),
                Err(e) => {
                    eprintln!("ERROR: {e}");
                    std::process::exit(1);
                }
            }
        }
        Some(Command::Modules { ts_dump, out_dir }) => {
            match humanify_core::modules::modules_dump::dump_modules(
                std::path::Path::new(&ts_dump),
                std::path::Path::new(&out_dir),
            ) {
                Ok(count) => println!("modules: {count} factory row(s) -> {out_dir}"),
                Err(e) => {
                    eprintln!("ERROR: {e}");
                    std::process::exit(1);
                }
            }
        }
        Some(Command::Stmtctx { ts_dump, out_dir }) => {
            match humanify_core::matching::matches_dump::dump_stmt_contexts(
                std::path::Path::new(&ts_dump),
                std::path::Path::new(&out_dir),
            ) {
                Ok(count) => println!("stmtctx: {count} row(s) -> {out_dir}"),
                Err(e) => {
                    eprintln!("ERROR: {e}");
                    std::process::exit(1);
                }
            }
        }
        Some(Command::Hashprobe {
            ts_dump,
            spans,
            out,
        }) => {
            match humanify_core::matching::matches_dump::dump_hash_probe(
                std::path::Path::new(&ts_dump),
                std::path::Path::new(&spans),
                std::path::Path::new(&out),
            ) {
                Ok(count) => println!("hashprobe: {count} row(s) -> {out}"),
                Err(e) => {
                    eprintln!("ERROR: {e}");
                    std::process::exit(1);
                }
            }
        }
        Some(Command::Refprobe {
            ts_dump,
            spans,
            out,
        }) => {
            match humanify_core::matching::matches_dump::dump_ref_probe(
                std::path::Path::new(&ts_dump),
                std::path::Path::new(&spans),
                std::path::Path::new(&out),
            ) {
                Ok(count) => println!("refprobe: {count} row(s) -> {out}"),
                Err(e) => {
                    eprintln!("ERROR: {e}");
                    std::process::exit(1);
                }
            }
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
