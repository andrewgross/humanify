//! The humanify binary (TS: src/index.ts + src/cli.ts, ports WP1.1/WPB.4).
//!
//! Two surfaces share the binary:
//! - the PIPELINE program (`humanify <input> [options]`, `humanify
//!   env-reads <path>`) — the commander-13 grammar the harness drives
//!   (contract 14 §1), parsed by `humanify_cli::commander` from the
//!   declarations in `humanify_cli::surface` (gated against the real TS
//!   program: test/parity/wpb4-cli-surface.json);
//! - the migration VERBS below (dump/gate commands, deleted at phase 6),
//!   parsed by clap. `argv[1]` naming a verb selects clap; anything else is
//!   the pipeline's (so an input file literally named like a verb must be
//!   passed as `./<name>`).

/// mimalloc, not glibc malloc: the parallel stages allocate from many
/// threads at once and glibc's arenas contend (00-control §3, 2026-09-24).
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

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
    /// WP3.2's phase-3 gate: run parse → graph → matching → the prior-version
    /// transfer pipeline on a TS dump's texts and write
    /// transfers-mechanical.json + votes.json. (Migration scaffolding —
    /// deleted at phase 6.)
    Transfers { ts_dump: String, out_dir: String },
    /// WP4.3's phase-4 step-1 gate: run the match stage, the transfer
    /// stage and the LLM naming waves on a TS dump's texts, the LLM
    /// answered by WARM REPLAY of `--llm-cache` (a miss is an error, never
    /// a live call; nothing is written to the cache), and write
    /// prompts.jsonl + cache-keys.jsonl + names.json. (Migration
    /// scaffolding — deleted at phase 6.)
    Waves {
        ts_dump: String,
        out_dir: String,
        /// The cache to replay (a SCRATCH COPY of the standing cache).
        #[arg(long)]
        llm_cache: Option<String>,
        /// Write only the naming graph's bisection probe over the original
        /// names (graph-probe.jsonl; compare with
        /// test/parity/wp43-gen-probe.ts).
        #[arg(long, default_value_t = false)]
        probe_graph: bool,
        /// With --probe-graph: session ids (comma-separated) whose full
        /// code/body text the probe writes.
        #[arg(long, value_delimiter = ',')]
        probe_only: Vec<String>,
        /// Plant an order bug (gate red runs): barrier-reverse | no-recrawl
        /// | no-retries.
        #[arg(long)]
        plant: Option<String>,
    },
    /// WP3.1's bundle-scale check of the Babel scope view: one JSON line
    /// per scope and per binding (UTF-16 spans), byte-comparable with
    /// `test/parity/wp31-scope-bundle-probe.mjs` on the same text.
    /// (Migration scaffolding — deleted at phase 6.)
    ScopeView { text: String, out: String },
    /// WP3.1's bundle-scale check of the validated-rename rules: the
    /// deterministic rename sequence of
    /// `test/parity/wp31-rename-bundle-probe.mjs`, one line per step.
    /// (Migration scaffolding — deleted at phase 6.)
    RenameProbe { text: String, out: String },
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
    /// WPB.2's unpack stage: detect the bundler, select the unpack adapter
    /// and write its tree (bun: vendor/*.js + runtime.js +
    /// vendor/_bun-modules.json; webcrack: the subprocess shim;
    /// passthrough: index.js). Prints one summary line.
    Unpack {
        /// The bundle (read as UTF-8, invalid bytes replaced).
        input: String,
        /// The output directory.
        out_dir: String,
        /// The prior release's humanified.js (`--prior-version`): its tree's
        /// vendor manifest feeds carry-over names and the manifest order.
        #[arg(long)]
        prior_version: Option<String>,
        /// Replay the vendor LLM namer from this cache dir (read-only;
        /// misses fail the batch). Without it the LLM pass is skipped.
        #[arg(long)]
        llm_cache: Option<String>,
        #[arg(long, default_value = "openai/gpt-oss-20b")]
        model: String,
        #[arg(long, default_value = "low")]
        reasoning_effort: String,
        #[arg(long)]
        max_tokens: Option<u64>,
        /// GATE SEAM (migration scaffolding): substitute the TS structural
        /// hash bytes from this TS dump's modules.json after proving the
        /// hash classes are a bijection (unpack::gate).
        #[arg(long)]
        inject_ts_hashes: Option<String>,
        /// Write the vendor LLM batches (keys, evidence, proposals) + stats
        /// as JSON here (the TS probe's `.llm.json` shape).
        #[arg(long)]
        llm_log: Option<String>,
        /// Write the extracted modules in BUNDLE order ({factoryVar,
        /// fileName, runtimeIdentifier}) as JSON here.
        #[arg(long)]
        index: Option<String>,
        /// The webcrack shim script (scripts/webcrack-shim.ts), run with
        /// `npx tsx` from its repo root; required for webpack/browserify.
        #[arg(long)]
        webcrack_shim: Option<String>,
    },
    /// WPB.3's library-detection gate: detect → select the unpack adapter →
    /// unpack (or take a given file list) → select the library detector →
    /// detect; prints the verdict as the TS probe's JSON
    /// (test/parity/wpb3-libdetect-probe.ts): paths relative to the unpack
    /// dir, region offsets in UTF-16 code units.
    Libdetect {
        /// The bundle.
        input: String,
        /// The unpack directory (written unless `--files` is given).
        unpack_dir: String,
        /// Use this unpack file list ([{path, metadata?}], JSON) instead of
        /// unpacking — the Bun case, whose file names derive from the hash
        /// bytes (00-control §3).
        #[arg(long)]
        files: Option<String>,
        /// The webcrack shim script (scripts/webcrack-shim.ts).
        #[arg(long)]
        webcrack_shim: Option<String>,
    },
    /// WP4.1's replay gate: re-derive every TS dispatch's cache key in Rust
    /// and replay it through the Rust cache — key, hit/miss, response bytes
    /// and entry bytes must all equal the TS's. (Migration scaffolding —
    /// deleted at phase 6.)
    LlmReplayGate {
        /// The full typed requests, one per dispatch ({seq, params, request,
        /// cacheKey} — the capture hook's rows).
        requests: String,
        /// The TS CachedLLMProvider's answers over the same cache
        /// (test/parity/wp41-replay-probe.ts).
        ts_replay: String,
        /// The cache directory (a scratch COPY; opened read-only).
        cache: String,
        /// The oracle dump's cache-keys.jsonl: the requests must reproduce
        /// its key sequence exactly.
        #[arg(long)]
        dump_keys: Option<String>,
    },
    /// WP5.1/5.2's placement gate: run placement on a TS dump's shipped
    /// text against the prior release's split ledger and write the
    /// Rust-side placement.json (`compare --sections placement`).
    /// (Migration scaffolding — deleted at phase 6.)
    Placement {
        ts_dump: String,
        out_dir: String,
        /// The prior release's split-ledger.json (READ ONLY).
        #[arg(long)]
        prior_ledger: Option<String>,
        /// Replay the mint namer from this cache dir (read-only; a miss
        /// fails the batch, as a dead endpoint does in the TS).
        #[arg(long)]
        llm_cache: Option<String>,
        #[arg(long, default_value = "openai/gpt-oss-20b")]
        model: String,
        #[arg(long, default_value = "low")]
        reasoning_effort: String,
        #[arg(long)]
        max_tokens: Option<u64>,
        /// GATE SEAM: substitute the TS statement-hash bytes from the
        /// dump's partitions.json after proving the partitions bijective.
        #[arg(long, default_value_t = false)]
        inject_ts_hashes: bool,
        /// `fossil` (every Bun bundle), `tiers` (the prior-carried
        /// PLACEMENT_TIERS, `--disable fossil-split`) or `cluster` (the
        /// fresh grouping: no prior, no fossils).
        #[arg(long, default_value = "fossil")]
        regime: String,
        /// tiers: the prior release's humanified.js (content-anchor carry).
        #[arg(long)]
        prior_text: Option<String>,
        /// tiers: the final-name → prior-name map JSON (identity carry).
        #[arg(long)]
        match_map: Option<String>,
        /// Placement kill switches, comma-separated (`--disable` names).
        #[arg(long, default_value = "")]
        disable: String,
    },
    /// WP5.3's emit gate: place + emit a TS dump's shipped text and write
    /// emit.json (`compare --sections emit`), the emitted tree and the
    /// ledger's layout fields. (Migration scaffolding — deleted at phase 6.)
    Emit {
        ts_dump: String,
        out_dir: String,
        /// The prior release's split-ledger.json (READ ONLY).
        #[arg(long)]
        prior_ledger: Option<String>,
        /// Replay the fossil mint namer from this cache dir (read-only).
        #[arg(long)]
        llm_cache: Option<String>,
        #[arg(long, default_value = "openai/gpt-oss-20b")]
        model: String,
        #[arg(long, default_value = "low")]
        reasoning_effort: String,
        /// Emit kill switches, comma-separated: emit-align, name-align,
        /// registrar-exemption.
        #[arg(long, default_value = "")]
        disable: String,
    },
    /// WP4.2's prompt gate: rebuild every prompt of an oracle pair from its
    /// typed request and require the TS's bytes; with --capture, also
    /// rebuild every module-level prompt, code window and naming context
    /// from the TS's captured builder inputs. (Migration scaffolding —
    /// deleted at phase 6.)
    PromptGate {
        /// The oracle pair's dump dir (prompts.jsonl + cache-keys.jsonl).
        dump: String,
        /// The capture hook's rows dir (test/parity/wp42-capture-hook.mjs).
        #[arg(long)]
        capture: Option<String>,
    },
}

/// The migration verbs clap owns (every `Command` variant's kebab name).
fn is_migration_verb(arg: &str) -> bool {
    Cli::command()
        .get_subcommands()
        .any(|c| c.get_name() == arg)
}

/// `humanify ingest`: the WP1.2 counts gate — parse, print the counts,
/// fail loud (exit 1) on any oxc diagnostic.
fn run_ingest(beautified_input: &str) {
    let text = match std::fs::read_to_string(beautified_input) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Error: cannot read {beautified_input}: {e}");
            std::process::exit(1);
        }
    };
    let (counts, errors) = humanify_core::ingest::ingest_counts_of_file(&text, beautified_input);
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

fn main() {
    let argv = humanify_cli::env::user_args();
    if !argv.first().is_some_and(|a| is_migration_verb(a)) {
        std::process::exit(humanify_cli::pipeline_main(&argv));
    }
    let cli = Cli::parse();
    match cli.command {
        Some(Command::Detect { input, profile }) => run_detect(&input, profile.as_deref()),
        Some(Command::Ingest { beautified_input }) => run_ingest(&beautified_input),
        Some(Command::ScopeView { text, out }) => write_probe_lines(
            &text,
            &out,
            humanify_core::rename::validated::scope_dump::scope_view_lines,
        ),
        Some(Command::RenameProbe { text, out }) => write_probe_lines(
            &text,
            &out,
            humanify_core::rename::validated::scope_dump::rename_probe_lines,
        ),
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
        Some(Command::Waves {
            ts_dump,
            out_dir,
            llm_cache,
            probe_graph,
            probe_only,
            plant,
        }) => run_waves_verb(
            &ts_dump,
            &out_dir,
            llm_cache,
            probe_graph,
            probe_only,
            plant,
        ),
        Some(Command::Transfers { ts_dump, out_dir }) => {
            match humanify_core::rename::transfer::dump::dump_transfers(
                std::path::Path::new(&ts_dump),
                std::path::Path::new(&out_dir),
            ) {
                Ok(s) => println!(
                    "transfers: {} row(s), {} vote row(s) -> {out_dir}",
                    s.rows, s.votes
                ),
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
        Some(Command::Unpack {
            input,
            out_dir,
            prior_version,
            llm_cache,
            model,
            reasoning_effort,
            max_tokens,
            inject_ts_hashes,
            llm_log,
            index,
            webcrack_shim,
        }) => {
            let args = UnpackArgs {
                prior_version,
                llm_cache,
                key_params: humanify_model::llm::CacheKeyParams {
                    model,
                    // The TS passes a literal 0 (unified.ts buildProvider).
                    temperature: Some(0.0),
                    max_tokens,
                    reasoning_effort: Some(reasoning_effort),
                },
                inject_ts_hashes,
                llm_log,
                index,
                webcrack_shim,
            };
            if let Err(e) = run_unpack(&input, &out_dir, args) {
                eprintln!("ERROR: {e}");
                std::process::exit(1);
            }
        }
        Some(Command::Libdetect {
            input,
            unpack_dir,
            files,
            webcrack_shim,
        }) => match run_libdetect(
            &input,
            &unpack_dir,
            files.as_deref(),
            webcrack_shim.as_deref(),
        ) {
            Ok(json) => println!("{json}"),
            Err(e) => {
                eprintln!("ERROR: {e}");
                std::process::exit(1);
            }
        },
        Some(Command::LlmReplayGate {
            requests,
            ts_replay,
            cache,
            dump_keys,
        }) => run_llm_replay_gate(&requests, &ts_replay, &cache, dump_keys.as_deref()),
        Some(Command::PromptGate { dump, capture }) => run_prompt_gate(&dump, capture.as_deref()),
        Some(Command::Emit {
            ts_dump,
            out_dir,
            prior_ledger,
            llm_cache,
            model,
            reasoning_effort,
            disable,
        }) => emit_verb(EmitArgs {
            ts_dump,
            out_dir,
            prior_ledger,
            llm_cache,
            model,
            reasoning_effort,
            disable,
        }),
        Some(Command::Placement {
            ts_dump,
            out_dir,
            prior_ledger,
            llm_cache,
            model,
            reasoning_effort,
            max_tokens,
            inject_ts_hashes,
            regime,
            prior_text,
            match_map,
            disable,
        }) => {
            let params = humanify_model::llm::CacheKeyParams {
                model,
                // The TS passes a literal 0 (unified.ts buildProvider).
                temperature: Some(0.0),
                max_tokens,
                reasoning_effort: Some(reasoning_effort),
            };
            let inputs = PlacementInputs {
                prior_ledger,
                prior_text,
                match_map,
                regime,
                disable,
                inject_ts_hashes,
            };
            if let Err(e) = run_placement(&ts_dump, &out_dir, inputs, llm_cache.as_deref(), &params)
            {
                eprintln!("ERROR: {e}");
                std::process::exit(1);
            }
        }
        None => {
            // No subcommand: print help (commander's behavior with a
            // required argument is the same shape).
            Cli::command().print_help().expect("help should print");
        }
    }
}

/// `humanify waves`: the WP4.3 gate's dump over a replay-only client.
fn run_waves_verb(
    ts_dump: &str,
    out_dir: &str,
    llm_cache: Option<String>,
    probe_graph: bool,
    probe_only: Vec<String>,
    plant: Option<String>,
) {
    use humanify_core::naming::waves::processor::Plant;
    let plant = match plant.as_deref() {
        None => None,
        Some("barrier-reverse") => Some(Plant::BarrierReversed),
        Some("no-recrawl") => Some(Plant::NoRecrawl),
        Some("no-retries") => Some(Plant::NoRetries),
        Some(other) => {
            eprintln!("ERROR: unknown --plant {other}");
            std::process::exit(2);
        }
    };
    let cache = llm_cache.map(std::path::PathBuf::from);
    if cache.is_none() && !probe_graph {
        eprintln!("ERROR: --llm-cache is required (the waves replay; no live calls)");
        std::process::exit(2);
    }
    let options = humanify_core::naming::waves::dump::WavesDumpOptions {
        probe_graph,
        probe_only,
        llm_cache: cache.clone(),
        plant,
    };
    let replay_dir = cache.unwrap_or_default();
    match humanify_core::naming::waves::dump::dump_waves(
        std::path::Path::new(ts_dump),
        std::path::Path::new(out_dir),
        &options,
        |params| humanify_llm::LlmClient::replay_only(&replay_dir, params),
    ) {
        Ok(s) => println!("waves: {s:?} -> {out_dir}"),
        Err(e) => {
            eprintln!("ERROR: {e}");
            std::process::exit(1);
        }
    }
}

/// The webcrack shim for a `--webcrack-shim <script>` flag (the one owner:
/// `humanify_cli::unminify::webcrack_shim`).
fn webcrack_shim(script: &str) -> humanify_core::unpack::webcrack::WebcrackShim {
    humanify_cli::unminify::webcrack_shim(std::path::Path::new(script))
}

/// `humanify libdetect`: the verdict as the TS probe's JSON.
fn run_libdetect(
    input: &str,
    unpack_dir: &str,
    files_json: Option<&str>,
    shim_script: Option<&str>,
) -> Result<String, String> {
    use humanify_core::libdetect::{detect_libraries, select_library_detector};
    use humanify_core::unpack::{UnpackedFile, bun, run_adapter, select_adapter};
    use humanify_model::js::{JsObject, JsValue, stringify};
    use std::path::Path;

    let code = std::fs::read(input)
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .map_err(|e| format!("cannot read {input}: {e}"))?;
    let dir = Path::new(unpack_dir);
    let adapter = select_adapter(&humanify_core::detect::detect_bundle(&code), None);
    let files: Vec<UnpackedFile> = match files_json {
        Some(path) => {
            let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
            // The same `{files:[{path, metadata?}]}` contract the webcrack
            // shim prints, on one line.
            let wrapped = format!("{{\"files\":{}}}", text.replace('\n', ""));
            humanify_core::unpack::webcrack::parse_shim_output(&wrapped)?.files
        }
        None => {
            let shim = shim_script.map(webcrack_shim);
            run_adapter(
                adapter,
                &code,
                dir,
                bun::BunUnpackOptions::default(),
                shim.as_ref(),
            )?
            .files
        }
    };
    let detector = select_library_detector(adapter.name());
    let result = detect_libraries(detector, &files)?;

    let rel = |p: &Path| JsValue::str(humanify_core::libdetect::relative_posix(dir, p));
    let regions_js = |text: &str, regions: &[humanify_core::libdetect::CommentRegion]| {
        JsValue::Array(
            regions
                .iter()
                .map(|r| {
                    let mut o = JsObject::new();
                    o.insert("libraryName", JsValue::str(&r.library_name));
                    o.insert("startOffset", JsValue::Number(utf16(text, r.start)));
                    o.insert(
                        "endOffset",
                        r.end
                            .map_or(JsValue::Null, |e| JsValue::Number(utf16(text, e))),
                    );
                    JsValue::Object(o)
                })
                .collect(),
        )
    };
    let mut library_files = Vec::new();
    for (path, d) in &result.library_files {
        let mut o = JsObject::new();
        o.insert("isLibrary", JsValue::Bool(d.is_library));
        o.insert_opt("libraryName", d.library_name.as_deref().map(JsValue::str));
        o.insert_opt(
            "detectedBy",
            d.detected_by.map(|b| JsValue::str(b.as_str())),
        );
        o.insert_opt(
            "moduleMetadata",
            d.module_metadata.as_ref().map(|m| {
                let mut mo = JsObject::new();
                mo.insert("id", JsValue::str(&m.id));
                mo.insert("modulePath", JsValue::str(&m.module_path));
                mo.insert("isEntry", JsValue::Bool(m.is_entry));
                JsValue::Object(mo)
            }),
        );
        library_files.push(JsValue::Array(vec![rel(path), JsValue::Object(o)]));
    }
    let mut mixed_files = Vec::new();
    for (path, m) in &result.mixed_files {
        let text = std::fs::read(path)
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .map_err(|e| format!("{}: {e}", path.display()))?;
        let mut o = JsObject::new();
        o.insert("regions", regions_js(&text, &m.regions));
        o.insert("libraryNames", JsValue::str_array(&m.library_names));
        mixed_files.push(JsValue::Array(vec![rel(path), JsValue::Object(o)]));
    }
    let mut out = JsObject::new();
    out.insert("adapter", JsValue::str(adapter.name()));
    out.insert("detector", JsValue::str(detector.name()));
    out.insert("libraryFiles", JsValue::Array(library_files));
    out.insert(
        "novelFiles",
        JsValue::Array(result.novel_files.iter().map(|p| rel(p)).collect()),
    );
    out.insert("mixedFiles", JsValue::Array(mixed_files));
    out.insert(
        "inputRegions",
        regions_js(
            &code,
            &humanify_core::libdetect::find_comment_regions(&code),
        ),
    );
    Ok(stringify(&JsValue::Object(out)))
}

/// A byte offset as the JS string index the TS reports.
fn utf16(text: &str, byte_at: usize) -> f64 {
    humanify_core::detect::js_text::utf16_offset(text, byte_at) as f64
}

/// `humanify unpack`'s flags beyond the two paths.
struct UnpackArgs {
    prior_version: Option<String>,
    llm_cache: Option<String>,
    key_params: humanify_model::llm::CacheKeyParams,
    inject_ts_hashes: Option<String>,
    llm_log: Option<String>,
    index: Option<String>,
    webcrack_shim: Option<String>,
}

/// `humanify unpack`: detect, select the adapter, write its tree, print a
/// summary line (+ the LLM cache counts when replaying).
fn run_unpack(input: &str, out_dir: &str, args: UnpackArgs) -> Result<(), String> {
    use humanify_core::modules::vendor_names::{ProviderVendorNamer, VendorNamer};
    use humanify_core::unpack::{UnpackAdapter, bun, gate, run_adapter, select_adapter};
    use std::path::Path;

    let code = std::fs::read(input)
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .map_err(|e| format!("cannot read {input}: {e}"))?;
    let out = Path::new(out_dir);
    let adapter = select_adapter(&humanify_core::detect::detect_bundle(&code), None);
    if adapter != UnpackAdapter::Bun {
        let shim = args.webcrack_shim.as_deref().map(webcrack_shim);
        let result = run_adapter(
            adapter,
            &code,
            out,
            bun::BunUnpackOptions::default(),
            shim.as_ref(),
        )?;
        println!(
            "unpack: adapter={} files={}",
            adapter.name(),
            result.files.len()
        );
        return Ok(());
    }

    let client = args
        .llm_cache
        .as_ref()
        .map(|dir| humanify_llm::LlmClient::replay_only(Path::new(dir), args.key_params.clone()));
    let mut provider_namer = client.as_ref().map(|c| ProviderVendorNamer::new(c));
    let mut recording = provider_namer.as_mut().map(|n| gate::RecordingNamer {
        inner: n as &mut dyn VendorNamer,
        batches: Vec::new(),
    });
    let ts_rows = args
        .inject_ts_hashes
        .as_deref()
        .map(|p| gate::read_ts_factory_hashes(Path::new(p)))
        .transpose()?;
    let injected = std::cell::Cell::new(None);
    let hook = |c: &mut humanify_core::modules::BunModuleClassification| {
        if let Some(rows) = &ts_rows {
            injected.set(Some(gate::inject_ts_hashes(c, rows)?));
        }
        Ok(())
    };
    let prior = args.prior_version.as_deref().map(Path::new);
    let outcome = bun::unpack_bun(
        &code,
        out,
        bun::BunUnpackOptions {
            namer: recording.as_mut().map(|n| n as &mut dyn VendorNamer),
            prior_vendor_names: prior.and_then(bun::load_prior_vendor_names_from),
            prior_manifest_factories: prior.and_then(bun::load_prior_manifest_factories_from),
            classification_hook: Some(&hook),
        },
    )?;
    let mut sources: Vec<(String, usize)> = Vec::new();
    for f in outcome.manifest.iter().flat_map(|m| &m.factories) {
        match sources.iter_mut().find(|(k, _)| k == f.name_source) {
            Some((_, n)) => *n += 1,
            None => sources.push((f.name_source.to_string(), 1)),
        }
    }
    sources.sort();
    println!(
        "unpack: adapter=bun files={} sources={} llm-renamed={}{}",
        outcome.result.files.len(),
        sources
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join(","),
        outcome.llm_renamed,
        injected
            .get()
            .map(|r| format!(
                " ts-hashes-injected={} factories/{} classes (bijection)",
                r.factories, r.classes
            ))
            .unwrap_or_default()
    );
    if let Some(path) = &args.index {
        let json = serde_json::to_string_pretty(&outcome.bundle_order).expect("json");
        std::fs::write(
            path,
            json + "
",
        )
        .map_err(|e| format!("write {path}: {e}"))?;
    }
    let batches = recording.map(|r| r.batches).unwrap_or_default();
    let stats = provider_namer.map(|n| n.stats).unwrap_or_default();
    let cache = client.as_ref().and_then(|c| c.cache_stats());
    if let Some(cache) = &cache {
        println!(
            "llm-cache hits: {} misses: {} writes: {}",
            cache.hits, cache.misses, cache.writes
        );
    }
    if let Some(path) = &args.llm_log {
        let log = serde_json::json!({
            "stats": {
                "named": stats.named,
                "declined": stats.declined,
                "echoed": stats.echoed,
                "batchesFailed": stats.batches_failed,
            },
            "cache": cache.map(|c| serde_json::json!({"hits": c.hits, "misses": c.misses})),
            "batches": batches,
        });
        std::fs::write(
            path,
            serde_json::to_string_pretty(&log).expect("json") + "\n",
        )
        .map_err(|e| format!("write {path}: {e}"))?;
    }
    Ok(())
}

/// WP4.1's replay gate: print the summary, name the first divergences, exit
/// 1 on any divergence, 2 when the inputs cannot be read.
fn run_llm_replay_gate(requests: &str, ts_replay: &str, cache: &str, dump_keys: Option<&str>) {
    let report = match humanify_llm::replay_gate::run(
        std::path::Path::new(requests),
        dump_keys.map(std::path::Path::new),
        std::path::Path::new(ts_replay),
        std::path::Path::new(cache),
    ) {
        Ok(report) => report,
        Err(e) => {
            eprintln!("ERROR: {e}");
            std::process::exit(2);
        }
    };
    println!("llm-replay-gate: {}", report.summary());
    for seq in report.key_mismatches.iter().take(20) {
        eprintln!("KEY MISMATCH seq {seq}");
    }
    for seq in report.response_mismatches.iter().take(20) {
        eprintln!("RESPONSE MISMATCH seq {seq}");
    }
    for key in report.entry_roundtrip_mismatches.iter().take(20) {
        eprintln!("ENTRY BYTES MISMATCH {key}");
    }
    if !report.identical() {
        std::process::exit(1);
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

/// `humanify emit`'s arguments.
struct EmitArgs {
    ts_dump: String,
    out_dir: String,
    prior_ledger: Option<String>,
    llm_cache: Option<String>,
    model: String,
    reasoning_effort: String,
    disable: String,
}

/// `humanify emit` on a large-stack thread (the emit walks nest as deep as
/// the bundle's expressions do); exit 1 on an error.
fn emit_verb(args: EmitArgs) {
    let params = humanify_model::llm::CacheKeyParams {
        model: args.model.clone(),
        // The TS passes a literal 0 (unified.ts buildProvider).
        temperature: Some(0.0),
        max_tokens: None,
        reasoning_effort: Some(args.reasoning_effort.clone()),
    };
    let run = std::thread::Builder::new()
        .stack_size(1 << 30)
        .spawn(move || {
            run_emit(
                &args.ts_dump,
                &args.out_dir,
                args.prior_ledger.as_deref(),
                args.llm_cache.as_deref(),
                &params,
                &args.disable,
            )
        })
        .expect("spawn the emit thread")
        .join()
        .expect("the emit thread panicked");
    if let Err(e) = run {
        eprintln!("ERROR: {e}");
        std::process::exit(1);
    }
}

/// `humanify emit`: the WP5.3 gate's dump.
fn run_emit(
    ts_dump: &str,
    out_dir: &str,
    prior_ledger: Option<&str>,
    llm_cache: Option<&str>,
    params: &humanify_model::llm::CacheKeyParams,
    disable: &str,
) -> Result<(), String> {
    use humanify_core::emit::align::AlignSwitches;
    use humanify_core::emit::emit_dump::{EmitGate, dump_emit};
    use humanify_core::place::assign::namer::{ProviderSplitNamer, SplitNamer};
    use std::path::Path;

    let mut switches = AlignSwitches::default();
    let mut registrar_exemption_disabled = false;
    for name in disable.split(',').map(str::trim).filter(|n| !n.is_empty()) {
        match name {
            "emit-align" => switches.emit_align_disabled = true,
            "name-align" => switches.name_align_disabled = true,
            "registrar-exemption" => registrar_exemption_disabled = true,
            other => return Err(format!("--disable: not an emit switch: {other:?}")),
        }
    }
    let client =
        llm_cache.map(|dir| humanify_llm::LlmClient::replay_only(Path::new(dir), params.clone()));
    let mut namer = client.as_ref().map(|c| ProviderSplitNamer::new(c));
    let report = dump_emit(
        Path::new(ts_dump),
        Path::new(out_dir),
        EmitGate {
            prior_ledger: prior_ledger.map(Path::new),
            namer: namer.as_mut().map(|n| n as &mut dyn SplitNamer),
            switches,
            registrar_exemption_disabled,
        },
    )?;
    println!(
        "emit: {} ledger file(s), {} tree file(s) -> {out_dir}{}",
        report.files,
        report.tree_files,
        report
            .declined
            .map(|r| format!(" [DECLINED: {r}]"))
            .unwrap_or_default()
    );
    if let Some(stats) = client.as_ref().and_then(|c| c.cache_stats()) {
        println!(
            "llm-cache hits: {} misses: {} writes: {}",
            stats.hits, stats.misses, stats.writes
        );
    }
    Ok(())
}

/// `humanify placement`'s inputs beside the two paths.
struct PlacementInputs {
    prior_ledger: Option<String>,
    prior_text: Option<String>,
    match_map: Option<String>,
    regime: String,
    disable: String,
    inject_ts_hashes: bool,
}

/// The placement kill switches from a `--disable` list (the registry's
/// names; anything else is an error, never a silent no-op).
fn placement_switches(
    disable: &str,
) -> Result<humanify_core::place::tiers::PlacementSwitches, String> {
    let mut s = humanify_core::place::tiers::PlacementSwitches::default();
    for name in disable.split(',').map(str::trim).filter(|n| !n.is_empty()) {
        match name {
            "content-anchor" => s.content_anchor = true,
            "anchor-preempt" => s.anchor_preempt = true,
            "anchor-nearident" => s.anchor_nearident = true,
            "allsame-vote" => s.allsame_vote = true,
            "empty-decl-hash-guard" => s.empty_decl_hash_guard = true,
            other => return Err(format!("--disable: not a placement switch: {other:?}")),
        }
    }
    Ok(s)
}

/// `humanify placement`: run the placement gate's dump; with a cache, the
/// mint namer replays it and every dispatched namer prompt must equal the
/// TS dump's `prompts.jsonl` row (bytes + cache key) — else exit 1.
fn run_placement(
    ts_dump: &str,
    out_dir: &str,
    inputs: PlacementInputs,
    llm_cache: Option<&str>,
    params: &humanify_model::llm::CacheKeyParams,
) -> Result<(), String> {
    use humanify_core::place::assign::namer::{
        ProviderSplitNamer, ProviderTreeReviser, SplitNamer, TreeReviser,
    };
    use humanify_core::place::placement_dump::{
        PlacementGate, Regime, check_dispatched_prompts, dump_placement,
    };
    use std::path::Path;

    let regime = match inputs.regime.as_str() {
        "fossil" => Regime::Fossil,
        "tiers" => Regime::Tiers,
        "cluster" => Regime::Cluster,
        other => {
            return Err(format!(
                "--regime: unknown regime {other:?} (fossil, tiers, cluster)"
            ));
        }
    };
    let switches = placement_switches(&inputs.disable)?;
    let client =
        llm_cache.map(|dir| humanify_llm::LlmClient::replay_only(Path::new(dir), params.clone()));
    let mut namer = client.as_ref().map(|c| ProviderSplitNamer::new(c));
    // The fresh grouping's reviser shares the client (unified.ts wires both
    // from one provider).
    let mut reviser = client
        .as_ref()
        .filter(|_| regime == Regime::Cluster)
        .map(|c| ProviderTreeReviser::new(c));
    let report = dump_placement(
        Path::new(ts_dump),
        Path::new(out_dir),
        PlacementGate {
            regime,
            prior_ledger: inputs.prior_ledger.map(Into::into),
            prior_text: inputs.prior_text.map(Into::into),
            match_map: inputs.match_map.map(Into::into),
            switches,
            namer: namer.as_mut().map(|n| n as &mut dyn SplitNamer),
            reviser: reviser.as_mut().map(|r| r as &mut dyn TreeReviser),
            inject_ts_hashes: inputs.inject_ts_hashes,
        },
    )?;
    println!(
        "placement: {} row(s), {} file(s); {} -> {out_dir}{}",
        report.rows,
        report.files,
        report.summary,
        report
            .injected
            .map(|(n, c)| format!(" [ts-hashes-injected: {n} statements / {c} classes, bijection]"))
            .unwrap_or_default()
    );
    let Some(namer) = namer else {
        return Ok(());
    };
    if let Some(stats) = client.as_ref().and_then(|c| c.cache_stats()) {
        println!(
            "llm-cache hits: {} misses: {} writes: {}; namer batches failed: {}",
            stats.hits, stats.misses, stats.writes, namer.failed_batches
        );
    }
    let prompts = Path::new(ts_dump).join("prompts.jsonl");
    let mut divergences =
        check_dispatched_prompts(&namer.dispatched, &prompts, "split-namer", params)?;
    let revised = reviser.map(|r| r.dispatched).unwrap_or_default();
    if regime == Regime::Cluster {
        divergences.extend(check_dispatched_prompts(
            &revised,
            &prompts,
            "tree-reviser",
            params,
        )?);
    }
    println!(
        "namer prompts: {} split-namer + {} tree-reviser dispatched, {} divergence(s) vs the TS dump",
        namer.dispatched.len(),
        revised.len(),
        divergences.len()
    );
    for d in &divergences {
        eprintln!("DIVERGES {d}");
    }
    if divergences.is_empty() {
        Ok(())
    } else {
        Err("namer prompts diverge from the TS dump".into())
    }
}

/// WP4.2's prompt gate: print the summary, name the first divergences,
/// exit 1 on any divergence, 2 when the inputs cannot be read.
fn run_prompt_gate(dump: &str, capture: Option<&str>) {
    let report = match humanify_core::naming::prompt_gate::run(
        std::path::Path::new(dump),
        capture.map(std::path::Path::new),
    ) {
        Ok(report) => report,
        Err(e) => {
            eprintln!("ERROR: {e}");
            std::process::exit(2);
        }
    };
    println!("prompt-gate: {}", report.summary());
    for d in report.divergences() {
        eprintln!("DIVERGES {d}");
    }
    if !report.identical() {
        std::process::exit(1);
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
    // The replay-only client (humanify-llm): a read-only disk cache, misses
    // fail the batch (counted), nothing can reach a model or be written.
    let client = llm_cache.map(|dir| {
        humanify_llm::LlmClient::replay_only(
            std::path::Path::new(dir),
            humanify_model::llm::CacheKeyParams {
                model: model.to_string(),
                // The TS passes a literal 0 (unified.ts buildProvider).
                temperature: Some(0.0),
                max_tokens,
                reasoning_effort: Some(reasoning_effort.to_string()),
            },
        )
    });
    let mut namer = client
        .as_ref()
        .map(|c| humanify_core::modules::vendor_names::ProviderVendorNamer::new(c));
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
            if let Some(stats) = client.as_ref().and_then(|c| c.cache_stats()) {
                println!(
                    "llm-cache hits: {} misses: {} writes: {}",
                    stats.hits, stats.misses, stats.writes
                );
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

/// A WP3.1 probe verb: read `text`, produce the probe lines, write them to
/// `out`, fail loud on any error.
fn write_probe_lines(text: &str, out: &str, lines_of: fn(&str) -> Result<Vec<String>, String>) {
    let result = std::fs::read_to_string(text)
        .map_err(|e| format!("reading {text}: {e}"))
        .and_then(|t| lines_of(&t))
        .and_then(|lines| {
            std::fs::write(out, format!("{}\n", lines.join("\n")))
                .map(|_| lines.len())
                .map_err(|e| format!("writing {out}: {e}"))
        });
    match result {
        Ok(n) => println!("{n} lines -> {out}"),
        Err(e) => {
            eprintln!("ERROR: {e}");
            std::process::exit(1);
        }
    }
}
