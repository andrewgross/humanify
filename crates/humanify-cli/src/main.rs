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
    /// WPB.1's detection gate: the bundler/minifier verdict for one input,
    /// printed as the TS `JSON.stringify(detectBundle(code))` shape.
    Detect {
        /// The bundle to classify (read as UTF-8, invalid bytes replaced —
        /// the TS `readFileSync(path, "utf-8")`).
        input: String,
        /// Write a Chrome trace-event profile (the TS `--profile` shape)
        /// of the read + detection spans; the summary goes to stderr.
        #[arg(long)]
        profile: Option<String>,
    },
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
    /// The vendor-naming gate (WPB.2-adjacent): rebuild the Bun vendor
    /// manifest + the structuralSignature partition family from a TS dump's
    /// MINIFIED text and the prior release's `vendor/_bun-modules.json`.
    /// (Migration scaffolding — deleted at phase 6.)
    VendorNames {
        /// The TS dump directory (its meta.json + text/minified.js).
        ts_dump: String,
        /// The Rust-side dump directory to write.
        out_dir: String,
        /// The prior release's written vendor/_bun-modules.json (carry-over
        /// input; e.g. /work/exp050-cold/<v>-rebased/vendor/_bun-modules.json).
        /// Absent or unparseable = no carry-over, bundle order.
        prior_manifest: Option<String>,
        /// The LLM response cache dir to replay (the oracle runs'
        /// /work/neutrality-cache). Without it the LLM pass is SKIPPED —
        /// the TS does the same (the pass runs only when a namer is wired).
        #[arg(long)]
        llm_cache: Option<String>,
        /// The model the cached prompts were issued with (part of the cache
        /// key).
        #[arg(long, default_value = "openai/gpt-oss-20b")]
        model: String,
        /// The reasoning effort the cached prompts were issued with.
        #[arg(long, default_value = "low")]
        reasoning_effort: String,
        /// Max tokens the cached prompts were issued with (absent from the
        /// key when not set).
        #[arg(long)]
        max_tokens: Option<u64>,
        /// The TS run's own written vendor/_bun-modules.json to diff the
        /// rebuilt manifest against — any divergence fails the command.
        #[arg(long)]
        expect_manifest: Option<String>,
        /// Write the TS-as-written degenerate partitions member keys
        /// ({fresh,0,0}) instead of the file paths (see vendor_dump.rs).
        #[arg(long, default_value_t = false)]
        collapsed_member_keys: bool,
    },
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Some(Command::Run) => {
            // Placeholder until WPB.4 ports the command surface.
            println!("humanify run: the pipeline command arrives at WPB.4");
        }
        Some(Command::Detect { input, profile }) => run_detect(&input, profile.as_deref()),
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
        Some(Command::VendorNames {
            ts_dump,
            out_dir,
            prior_manifest,
            llm_cache,
            model,
            reasoning_effort,
            max_tokens,
            expect_manifest,
            collapsed_member_keys,
        }) => {
            run_vendor_names(
                &ts_dump,
                &out_dir,
                prior_manifest.as_deref(),
                llm_cache.as_deref(),
                &model,
                &reasoning_effort,
                max_tokens,
                expect_manifest.as_deref(),
                collapsed_member_keys,
            );
        }
        None => {
            // No subcommand: print help (commander's behavior with a
            // required argument is the same shape).
            Cli::command().print_help().expect("help should print");
        }
    }
}

/// `humanify detect`: read like the TS pipeline (lossy UTF-8, BOM kept),
/// classify, print one JSON line. With `--profile`, the read and the
/// detection run inside spans shaped like the TS pipeline's
/// (`file-io:read` {path, bytes} and `detection` {bundler}; the TS
/// detection span's `adapter` key arrives with the unpack adapter
/// registry, WPB.2).
fn run_detect(input: &str, profile: Option<&str>) {
    use humanify_core::profiling::{Profiler, format_profile_summary, to_trace_events};
    use humanify_model::profiling::{JsObject, trace_tid};

    let profiler = Profiler::new(profile.is_some());
    let read = profiler.start_span("file-io:read", "io", trace_tid::PIPELINE, None);
    let code = match std::fs::read(input) {
        Ok(b) => String::from_utf8_lossy(&b).into_owned(),
        Err(e) => {
            eprintln!("Error: cannot read {input}: {e}");
            std::process::exit(1);
        }
    };
    // `bytes: code.length` — UTF-16 code units, as the TS records it.
    read.end(Some(
        JsObject::new()
            .with("path", input)
            .with("bytes", code.encode_utf16().count()),
    ));
    let span = profiler.pipeline_span("detection");
    let verdict = humanify_core::detect::detect_bundle(&code);
    let bundler = serde_json::to_value(verdict.bundler.kind).expect("an enum serializes");
    span.end(Some(JsObject::new().with("bundler", bundler)));
    println!(
        "{}",
        serde_json::to_string(&verdict).expect("a detection verdict serializes")
    );
    if let Some(path) = profile {
        let report = profiler.finalize(Some(input));
        let trace =
            serde_json::to_string_pretty(&to_trace_events(&report)).expect("a trace serializes");
        if let Err(e) = std::fs::write(path, trace) {
            eprintln!("Error: cannot write {path}: {e}");
            std::process::exit(1);
        }
        eprintln!("{}", format_profile_summary(&report));
        eprintln!("Profile written to {path}");
    }
}

/// The vendor-naming gate's CLI wiring: build the cache-replaying namer
/// from the flags (defaults = the oracle runs' own: gpt-oss-20b, effort
/// low, no max tokens, a literal temperature 0), run the gate, print the
/// summary, and fail loud on any --expect-manifest divergence.
#[allow(clippy::too_many_arguments)]
fn run_vendor_names(
    ts_dump: &str,
    out_dir: &str,
    prior_manifest: Option<&str>,
    llm_cache: Option<&str>,
    model: &str,
    reasoning_effort: &str,
    max_tokens: Option<u64>,
    expect_manifest: Option<&str>,
    collapsed_member_keys: bool,
) {
    let mut namer = llm_cache.map(|dir| {
        humanify_core::modules::vendor_names::CacheReplayNamer::new(
            std::path::PathBuf::from(dir),
            humanify_core::modules::vendor_names::CacheKeyParams {
                model: model.to_string(),
                // The TS passes a literal 0 (unified.ts buildProvider).
                temperature: 0,
                max_tokens,
                reasoning_effort: Some(reasoning_effort.to_string()),
            },
        )
    });
    let gate = humanify_core::modules::vendor_dump::VendorNamesGate {
        namer: namer
            .as_mut()
            .map(|n| n as &mut dyn humanify_core::modules::vendor_names::VendorNamer),
        collapsed_member_keys,
        expect_manifest: expect_manifest.map(str::to_string),
    };
    match humanify_core::modules::vendor_dump::dump_vendor_names(
        std::path::Path::new(ts_dump),
        prior_manifest.map(std::path::Path::new),
        std::path::Path::new(out_dir),
        gate,
    ) {
        Ok(report) => {
            let sources = {
                let mut keys: Vec<_> = report.name_sources.iter().collect();
                keys.sort();
                keys.iter()
                    .map(|(k, v)| format!("{k}={v}"))
                    .collect::<Vec<_>>()
                    .join(",")
            };
            println!(
                "vendornames: {sources} over {} factor(y/ies), {} family member(s) -> {out_dir}",
                report.factories, report.family_members
            );
            if let Some(namer) = namer {
                println!("llm-cache misses: {}", namer.misses);
            }
            for d in &report.manifest_divergences {
                eprintln!("DIVERGENCE: {d}");
            }
            if !report.manifest_divergences.is_empty() {
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("ERROR: {e}");
            std::process::exit(1);
        }
    }
}

#[allow(unused_imports)]
use clap::CommandFactory;
