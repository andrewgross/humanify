//! The humanify binary.
//!
//! Two surfaces share the binary:
//! - the PIPELINE program (`humanify <input> [options]`, `humanify
//!   env-reads <path>`) — the commander grammar the harness drives
//!   (contract 14 §1), parsed by `humanify_cli::commander` from the
//!   declarations in `humanify_cli::surface`;
//! - the stage VERBS below (`detect`, `unpack`, `libdetect`, `format`,
//!   `format-check`) — one pipeline stage run on its own, for inspection
//!   and for the gate (`format-check` replays the committed formatter
//!   goldens, test/parity/format-goldens.json — the formatter's frozen
//!   spec). Parsed by clap. `argv[1]` naming a verb selects clap; anything
//!   else is the pipeline's (so an input file literally named like a verb
//!   must be passed as `./<name>`).
//!
//! The migration verbs that rebuilt one stage's rows from a TS dump for the
//! parity gates were deleted at the cutover (docs/rust-port/19-cutover.md).

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
    /// detect; prints the verdict as JSON: paths relative to the unpack
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
    /// WP5.6's native formatter (`core::format`): the stage-6 beautify of
    /// one file — Babel's `transform()` with the four stage-6 plugins and
    /// `@babel/generator` (retainLines off, comments off), byte for byte.
    Format {
        /// The file to format (read as UTF-8, invalid bytes replaced).
        input: String,
        /// Write here instead of stdout.
        #[arg(short = 'o', long)]
        out: Option<String>,
        /// Print only (`transformWithPlugins(code, [])`): the G1 leg.
        #[arg(long, default_value_t = false)]
        no_transforms: bool,
        /// Plant a perturbation (gate red runs): `drop:<visitor>` (a
        /// visitor's node type or plugin name), `requeue-reversed`,
        /// `rust-number-format`.
        #[arg(long)]
        plant: Option<String>,
    },
    /// The formatter's golden check: every case of a goldens file (the
    /// committed test/parity/format-goldens.json — the formatter's frozen
    /// spec, captured from the TS beautifier — or a fuzz corpus of the same
    /// `{name, code, none, full}` shape) formatted both ways and compared
    /// with the recorded bytes / errors. Exit 1 on any difference; `--plant`
    /// proves the check can fail. The `rust:format-golden` gate stage.
    FormatCheck {
        goldens: String,
        /// Print at most this many differing cases.
        #[arg(long, default_value_t = 10)]
        show: usize,
        /// Plant a perturbation (as `format --plant`) on the stage-6 legs.
        #[arg(long)]
        plant: Option<String>,
    },
}

/// The stage verbs clap owns (every `Command` variant's kebab name).
fn is_stage_verb(arg: &str) -> bool {
    Cli::command()
        .get_subcommands()
        .any(|c| c.get_name() == arg)
}

fn main() {
    let argv = humanify_cli::env::user_args();
    if !argv.first().is_some_and(|a| is_stage_verb(a)) {
        std::process::exit(humanify_cli::pipeline_main(&argv));
    }
    let cli = Cli::parse();
    match cli.command {
        Some(Command::Detect { input, profile }) => run_detect(&input, profile.as_deref()),
        Some(Command::Unpack {
            input,
            out_dir,
            prior_version,
            llm_cache,
            model,
            reasoning_effort,
            max_tokens,
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
        Some(Command::Format {
            input,
            out,
            no_transforms,
            plant,
        }) => format_verb(&input, out.as_deref(), no_transforms, plant.as_deref()),
        Some(Command::FormatCheck {
            goldens,
            show,
            plant,
        }) => format_check_verb(&goldens, show, plant.as_deref()),
        None => {
            // No subcommand: print help (commander's behavior with a
            // required argument is the same shape).
            Cli::command().print_help().expect("help should print");
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
    let prior = args.prior_version.as_deref().map(Path::new);
    let outcome = bun::unpack_bun(
        &code,
        out,
        bun::BunUnpackOptions {
            namer: recording.as_mut().map(|n| n as &mut dyn VendorNamer),
            prior: prior.and_then(bun::load_prior_vendor),
            manifest_prior_order_disabled: false,
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
        outcome
            .rekey
            .map(|r| format!(
                " rekeyed-by-content={}/{} groups={}",
                r.factories_joined, r.prior_entries, r.groups_joined
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

/// `humanify format`: the native formatter on one file.
fn format_verb(input: &str, out: Option<&str>, no_transforms: bool, plant: Option<&str>) {
    use humanify_core::format::{FormatOptions, Plant, Plugins, format};
    let bytes = match std::fs::read(input) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("ERROR: cannot read {input}: {e}");
            std::process::exit(1);
        }
    };
    let code = String::from_utf8_lossy(&bytes);
    let plant = match plant.map(Plant::parse) {
        None => None,
        Some(Ok(p)) => Some(p),
        Some(Err(e)) => {
            eprintln!("ERROR: {e}");
            std::process::exit(2);
        }
    };
    let opts = FormatOptions {
        plugins: if no_transforms {
            Plugins::NONE
        } else {
            Plugins::STAGE6
        },
        plant,
    };
    let text = match format(&code, &opts) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("ERROR: {input}: {e}");
            std::process::exit(1);
        }
    };
    let written = match out {
        Some(path) => std::fs::write(path, text.as_bytes()),
        None => {
            use std::io::Write;
            std::io::stdout().write_all(text.as_bytes())
        }
    };
    if let Err(e) = written {
        eprintln!("ERROR: write: {e}");
        std::process::exit(1);
    }
}

/// `humanify format-check`: a goldens file against the formatter.
fn format_check_verb(path: &str, show: usize, plant: Option<&str>) {
    use humanify_core::format::{FormatOptions, Plant, Plugins, format};
    let plant = match plant.map(Plant::parse) {
        None => None,
        Some(Ok(p)) => Some(p),
        Some(Err(e)) => {
            eprintln!("ERROR: {e}");
            std::process::exit(2);
        }
    };
    let rows: Vec<serde_json::Value> = match std::fs::read_to_string(path)
        .map_err(|e| e.to_string())
        .and_then(|t| serde_json::from_str(&t).map_err(|e| e.to_string()))
    {
        Ok(r) => r,
        Err(e) => {
            eprintln!("ERROR: {path}: {e}");
            std::process::exit(2);
        }
    };
    let (mut same, mut differ) = (0usize, 0usize);
    for row in &rows {
        let name = row["name"].as_str().unwrap_or("?");
        let code = row["code"].as_str().unwrap_or("");
        for (leg, plugins) in [("none", Plugins::NONE), ("full", Plugins::STAGE6)] {
            let want = &row[leg];
            let plant = if plugins.is_empty() { None } else { plant };
            let got = format(code, &FormatOptions { plugins, plant });
            let ok = match (want.get("text").and_then(serde_json::Value::as_str), &got) {
                (Some(w), Ok(g)) => w == g,
                (None, Err(_)) => true,
                _ => false,
            };
            if ok {
                same += 1;
                continue;
            }
            differ += 1;
            if differ <= show {
                println!(
                    "== {name} [{leg}]\n-- code\n{code}\n-- ts\n{}\n-- rust\n{}",
                    want,
                    match got {
                        Ok(t) => t,
                        Err(e) => format!("ERROR {e}"),
                    }
                );
            }
        }
    }
    println!(
        "format-check: {} legs, identical {same}, differing {differ}",
        same + differ
    );
    if differ > 0 {
        std::process::exit(1);
    }
}

use clap::CommandFactory;
