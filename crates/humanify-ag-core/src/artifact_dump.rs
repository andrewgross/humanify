//! The artifact dump (`--dump-artifacts <dir>`): each stage's own decision
//! records, serialized as they are, one file per section through one sink
//! ([`DumpSink::section`]). Not a second pipeline pass: every record is
//! what the stage computed, captured while it was live (the naming era's
//! [`crate::naming::driver::era::EraCapture`], the split's
//! [`SplitSections`]) or read from the naming outcome at the end. The one
//! exception is modules: the minified text classified again by the
//! unpack's own classifier ([`crate::modules::classify_site`]).
//!
//! Sections: functions, modules, matches, matches-close, twin-gates,
//! private-renames, transfers, transfers-mechanical, votes, names,
//! regions, placement, emit, partitions (each `<name>.json`), and prompts.jsonl,
//! streamed by [`DispatchLog`] as dispatches commit. A section a run does
//! not reach (no prior: no matches; no split: no placement) is not written.
//!
//! Separately, [`DispatchLog`] is the run's per-dispatch recorder: every
//! dispatch site hands its row here the moment the dispatch is committed.
//! `--dump-artifacts` streams prompts.jsonl rows to part files as
//! dispatches commit (no per-ask string survives its row); `--dump-asks`
//! retains the small ask rows for the end-of-run log; a run with NEITHER
//! flag retains nothing per ask.

use std::collections::HashMap;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use humanify_ag_model::llm::{BatchRenameRequest, CacheKeyParams, LlmCall};
use serde::Serialize;
use serde_json::{Value, json};

use crate::naming::ask_trace::{AskScope, AskSite};
use crate::naming::driver::NamingOutcome;
use crate::naming::passes::sweep::SweepDispatch;
use crate::naming::waves::processor::DispatchRecord;
use crate::trail::Anchor;

/// One dump directory: a section is `<dir>/<name>.json`, its records as
/// serde writes them (pretty, a trailing newline).
pub struct DumpSink {
    dir: PathBuf,
}

impl DumpSink {
    /// The sink over `dir`, created when missing.
    // io: legacy effect site, listed in dev/census/effects-allowlist.json
    #[allow(clippy::disallowed_methods)]
    pub fn create(dir: &Path) -> Result<DumpSink, String> {
        std::fs::create_dir_all(dir).map_err(|e| format!("mkdir {}: {e}", dir.display()))?;
        Ok(DumpSink {
            dir: dir.to_path_buf(),
        })
    }

    /// Write one section: `records` serialized to `<name>.json`.
    // io: legacy effect site, listed in dev/census/effects-allowlist.json
    #[allow(clippy::disallowed_methods)]
    pub fn section<T: Serialize + ?Sized>(&self, name: &str, records: &T) -> Result<(), String> {
        let path = self.dir.join(format!("{name}.json"));
        let file =
            std::fs::File::create(&path).map_err(|e| format!("write {}: {e}", path.display()))?;
        let mut out = BufWriter::new(file);
        serde_json::to_writer_pretty(&mut out, records)
            .map_err(|e| format!("serialize {name}: {e}"))?;
        out.write_all(b"\n")
            .and_then(|()| out.flush())
            .map_err(|e| format!("write {}: {e}", path.display()))
    }
}

/// The split's dump sections, snapshotted when the split ran.
pub struct SplitSections {
    /// partitions' `statementHash` family: each wrapper statement's span
    /// in the shipped text and its hash.
    pub statement_family: Vec<((u32, u32), String)>,
    /// The emitted layout that won: per file its statements' bundle
    /// indexes, and every require alias the runnable tree writes.
    pub emit: Value,
    /// The placement trail's rows.
    pub placement: Value,
    /// The split's input (the shipped text).
    pub shipped: String,
}

/// What the writer reads.
pub struct DumpInputs<'a> {
    pub dir: &'a Path,
    pub minified: &'a str,
    pub outcome: &'a NamingOutcome,
    pub split: Option<&'a SplitSections>,
    /// The processed file's library comment regions (MINIFIED-text byte
    /// offsets).
    pub comment_regions: &'a [crate::libdetect::CommentRegion],
    /// The post-split reconcile's trail rows, per split file (their
    /// spans index the file's text).
    pub extra_trail: &'a crate::naming::report::diagnostics::ExtraTrail,
    /// The run's bundle layout (the classification site's container).
    pub layout: crate::toolchain::BundleLayout,
    /// The run's module wrapper grammar (the classification site's).
    pub module_wrappers: crate::toolchain::ModuleWrapperGrammar,
}

/// Write every section the run reached.
pub fn write_artifact_dump(inp: &DumpInputs<'_>) -> Result<(), String> {
    let sink = DumpSink::create(inp.dir)?;
    let out = inp.outcome;
    let capture = out.capture.as_ref();
    for (name, records) in capture.iter().flat_map(|c| &c.sections) {
        sink.section(name, records)?;
    }
    if let Some(site) =
        crate::modules::classify_site(inp.minified, inp.layout, inp.module_wrappers)?
    {
        sink.section("modules", &site)?;
    }
    let mut transfers = out.trail.transfer_rows();
    transfers.extend(extra_rows(inp.extra_trail));
    sink.section("transfers", &transfers)?;
    sink.section(
        "names",
        &json!({
            "final": final_names(&transfers),
            "waves": out.waves.names,
            "library": out.library_names,
        }),
    )?;
    sink.section(
        "partitions",
        &json!({
            "structuralHash": capture.map(|c| &c.structural_family),
            "statementHash": inp.split.map(|s| &s.statement_family),
        }),
    )?;
    sink.section(
        "regions",
        &json!({
            "commentRegions": inp.comment_regions,
            "libraryFunctions": out.library_functions,
        }),
    )?;
    if let Some(split) = inp.split {
        sink.section("placement", &split.placement)?;
        sink.section("emit", &split.emit)?;
    }
    Ok(())
}

/// Every renamed binding's final name and the tier that settled it.
fn final_names(transfers: &[humanify_ag_model::dump::TransferRow]) -> Vec<Value> {
    transfers
        .iter()
        .filter_map(|t| {
            let new_name = t.final_name.as_ref()?;
            Some(json!({
                "target": t.target,
                "oldName": t.old_name,
                "newName": new_name,
                "settledBy": t.settled_by,
            }))
        })
        .collect()
}

/// The post-split reconcile's rows keyed in their own split file: the
/// file's tree-relative path and the row's byte span in the text the pass
/// parsed.
fn extra_rows(
    extra: &crate::naming::report::diagnostics::ExtraTrail,
) -> impl Iterator<Item = humanify_ag_model::dump::TransferRow> + '_ {
    extra.iter().flat_map(|x| {
        x.rows.iter().map(|e| {
            e.transfer_row(humanify_ag_model::dump::SpanKey {
                text: x.file.clone(),
                start: i64::from(e.target.decl_span.start),
                end: i64::from(e.target.decl_span.end),
            })
        })
    })
}

/// One LLM dispatch of the run, by site (`recordPromptDump`'s callers).
pub enum Dispatch<'a> {
    /// A naming wave's request (`site: "naming"`).
    Naming(&'a DispatchRecord),
    /// The coverage sweep's (`site: "sweep"`), with its anchored text.
    Sweep(Anchor, &'a SweepDispatch),
    /// A single-call namer: the vendor namer (`vendor`), the split's file
    /// namer and tree reviser (`folders`).
    Plain {
        function_id: &'static str,
        site: &'static str,
        call: &'a LlmCall,
    },
}

/// What each dispatch site does with a committed dispatch (finding #65).
/// The modes are the CLI flags: `Off` is a run with neither dump flag —
/// nothing per ask survives the dispatch; `Asks` is `--dump-asks` — the
/// small ask row is retained for the end-of-run log, never the prompt
/// text; `Full` is `--dump-artifacts` (and the test log, which retains
/// the records) — the prompt rows are written AT COMMIT, so no
/// per-ask string is held past its row either.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RecordMode {
    Off,
    Asks,
    Full,
}

/// The run's per-dispatch recorder. Every site hands its dispatch here the
/// moment it is committed; row order is commit order, which is the dump's
/// own concatenation order (the vendor namer, the naming sites, the
/// split's namers — the stages never interleave). The waves' pipelined
/// rounds are the one deferral: their records commit at the round's
/// canonical sort, so a round's `Full` records live until then (a round,
/// never the run).
pub struct DispatchLog {
    params: CacheKeyParams,
    /// `--dump-asks`: the ask rows, in commit order (vendor rows first —
    /// [`Self::begin_named_file`] keeps only those when a new file starts).
    asks: Option<Vec<AskRow>>,
    vendor_asks: Option<usize>,
    /// `--dump-artifacts`: the prompts.jsonl part files. None keeps no rows.
    rows: Option<LogRows>,
    /// Tests read the dispatches back out of the sites' own vectors; the
    /// production modes never retain one.
    retain: bool,
    seq: u64,
    rounds: HashMap<String, u64>,
}

impl DispatchLog {
    /// A run with neither dump flag: every [`Self::record`] is a no-op and
    /// no per-ask memory is retained anywhere.
    pub fn off(params: CacheKeyParams) -> Self {
        DispatchLog {
            params,
            asks: None,
            vendor_asks: None,
            rows: None,
            retain: false,
            seq: 0,
            rounds: HashMap::new(),
        }
    }

    /// `--dump-asks`: the ask rows only.
    pub fn asks(params: CacheKeyParams) -> Self {
        let mut log = Self::off(params);
        log.asks = Some(Vec::new());
        log
    }

    /// `--dump-artifacts`: the part files opened in `dir` now (the dump's
    /// other files are still written once, at the end).
    // io: legacy effect site, listed in dev/census/effects-allowlist.json
    #[allow(clippy::disallowed_methods)]
    pub fn dump(params: CacheKeyParams, dir: &Path) -> std::io::Result<Self> {
        std::fs::create_dir_all(dir)?;
        let rows = LogRows::open(dir)?;
        let mut log = Self::off(params);
        log.asks = Some(Vec::new());
        log.rows = Some(rows);
        Ok(log)
    }

    /// The name of the site a dispatch belongs to (its dump row's `site`).
    fn site_of(d: &Dispatch<'_>) -> &'static str {
        match d {
            Dispatch::Naming(_) => "naming",
            Dispatch::Sweep(_, _) => "sweep",
            Dispatch::Plain { site, .. } => site,
        }
    }

    /// The mode the dispatch sites prepare their records for.
    pub fn mode(&self) -> RecordMode {
        if self.rows.is_some() || self.retain {
            RecordMode::Full
        } else if self.asks.is_some() {
            RecordMode::Asks
        } else {
            RecordMode::Off
        }
    }

    /// Whether the sites retain the dispatches in their own vectors (the
    /// tests' log — production modes never do).
    pub fn retains(&self) -> bool {
        self.retain
    }

    /// The tests' log: full records retained by the sites, rows collected
    /// in memory as the byte-identity oracle.
    pub fn retain_for_tests(params: CacheKeyParams) -> Self {
        let mut log = Self::asks(params);
        log.retain = true;
        log.rows = Some(LogRows::in_memory());
        log
    }

    fn wants_rows(&self) -> bool {
        self.rows.is_some()
    }

    /// Record one committed dispatch: its prompt row (the dump) and/or
    /// its ask row (the ask log), in commit order.
    pub fn record(&mut self, d: &Dispatch<'_>) {
        if self.mode() == RecordMode::Off {
            return;
        }
        let round = round_of(d, &mut self.rounds);
        let seq = self.next_seq();
        if self.asks.is_some() {
            let parts = ask_parts_rounded(d, round);
            if let Some(asks) = &mut self.asks {
                asks.push(ask_row(seq, &parts));
            }
        }
        if self.wants_rows() {
            let prompt = prompt_row(seq, round, d, &self.params);
            let site = Self::site_of(d);
            if let Some(rows) = &mut self.rows {
                rows.write(site, &prompt);
            }
        }
    }

    /// Record one committed NAMING dispatch whose material the waves could
    /// not retain (the `Asks` mode's slim round records — never the prompt
    /// text): its ask row alone.
    pub fn record_ask_parts(&mut self, parts: &AskRowParts) {
        if self.asks.is_some() {
            let seq = self.next_seq();
            if let Some(asks) = &mut self.asks {
                asks.push(ask_row(seq, parts));
            }
        }
    }

    /// The TS dump's LAST-FILE rule: a run that names several files dumps
    /// only the last one's naming rows, so each file's naming section
    /// starts from the vendor rows' seq again. The vendor rows themselves
    /// (written before any file is named) are kept.
    pub fn begin_named_file(&mut self) {
        if self.mode() == RecordMode::Off {
            return;
        }
        if let Some(rows) = &mut self.rows {
            rows.begin_named_file();
        }
        let base = match self.vendor_asks {
            Some(len) => len,
            None => {
                let len = self.asks.as_ref().map_or(0, Vec::len);
                self.vendor_asks = Some(len);
                len
            }
        };
        if let Some(asks) = &mut self.asks {
            asks.truncate(base);
        }
        self.seq = base as u64;
    }

    /// The ask rows so far (the `--dump-asks` log's content).
    pub fn ask_rows(&self) -> &[AskRow] {
        self.asks.as_deref().unwrap_or(&[])
    }

    /// The tests' in-memory prompts.jsonl rows, one per dispatch, in
    /// commit order.
    pub fn memory_rows(&self) -> &[String] {
        self.rows.as_ref().map_or(&[][..], |r| r.memory())
    }

    fn next_seq(&mut self) -> u64 {
        let seq = self.seq;
        self.seq += 1;
        seq
    }

    /// Assemble the dump's prompts.jsonl from its parts (kept even when
    /// the run wrote zero rows: the file then holds one newline). An error here is the same
    /// error `write_artifact_dump` would have reported. Idempotent.
    pub fn close(&mut self) -> Result<(), String> {
        match self.rows.take() {
            Some(rows) => rows.close(),
            None => Ok(()),
        }
    }

    /// Drop the part files without assembling them — the failed-run rule:
    /// a run whose dump was never written leaves no dump files behind
    /// (exactly the TS behavior).
    pub fn discard(&mut self) {
        if let Some(rows) = self.rows.take() {
            rows.discard();
        }
    }
}

/// Where the `Full` mode's prompt rows go: the dump's part files
/// (`--dump-artifacts`) or, for the tests' log, memory.
enum LogRows {
    Files(LogFiles),
    Memory {
        rows: Vec<String>,
        /// Where the vendor rows end (frozen at the first named file) —
        /// the same vendor/naming split the part files make.
        vendor_base: Option<usize>,
    },
}

impl LogRows {
    fn open(dir: &Path) -> std::io::Result<Self> {
        Ok(LogRows::Files(LogFiles::open(dir)?))
    }

    fn in_memory() -> Self {
        LogRows::Memory {
            rows: Vec::new(),
            vendor_base: None,
        }
    }

    fn write(&mut self, site: &str, prompt: &str) {
        match self {
            LogRows::Files(files) => files.write(site, prompt),
            LogRows::Memory { rows, .. } => rows.push(prompt.to_string()),
        }
    }

    fn begin_named_file(&mut self) {
        match self {
            LogRows::Files(files) => files.begin_named_file(),
            LogRows::Memory { rows, vendor_base } => match vendor_base {
                Some(len) => rows.truncate(*len),
                None => *vendor_base = Some(rows.len()),
            },
        }
    }

    fn memory(&self) -> &[String] {
        match self {
            LogRows::Files(_) => &[],
            LogRows::Memory { rows, .. } => rows,
        }
    }

    fn close(self) -> Result<(), String> {
        match self {
            LogRows::Files(files) => files.close(),
            LogRows::Memory { .. } => Ok(()),
        }
    }

    fn discard(self) {
        if let LogRows::Files(files) = self {
            files.discard();
        }
    }
}

const PROMPTS: &str = "prompts.jsonl";
const VENDOR_PART: &str = "prompts.jsonl.vendorpart";
const NAMING_PART: &str = "prompts.jsonl.namingpart";

/// prompts.jsonl as two parts: the vendor rows (every row that precedes
/// the named files) and the current file's naming rows — truncated by
/// [`DispatchLog::begin_named_file`], so a multi-file run dumps the last
/// file's rows. [`LogFiles::close`] concatenates the parts.
struct LogFiles {
    dir: PathBuf,
    vendor: BufWriter<std::fs::File>,
    /// None once a re-create failed (the run keeps going; the error
    /// surfaces at close, like any dump write error).
    naming: Option<BufWriter<std::fs::File>>,
    rows: u64,
    error: Option<String>,
}

// io: legacy effect site, listed in dev/census/effects-allowlist.json
#[allow(clippy::disallowed_methods)]
fn part(dir: &Path, name: &str) -> std::io::Result<BufWriter<std::fs::File>> {
    Ok(BufWriter::new(std::fs::File::create(dir.join(name))?))
}

impl LogFiles {
    fn open(dir: &Path) -> std::io::Result<LogFiles> {
        Ok(LogFiles {
            dir: dir.to_path_buf(),
            vendor: part(dir, VENDOR_PART)?,
            naming: Some(part(dir, NAMING_PART)?),
            rows: 0,
            error: None,
        })
    }

    // default: legacy, untriaged: a Result error becomes a value here
    #[allow(clippy::let_underscore_must_use)]
    fn write(&mut self, site: &str, prompt: &str) {
        let target = match site {
            "vendor" => Some(&mut self.vendor),
            _ => self.naming.as_mut(),
        };
        if let Some(w) = target {
            let _ = w.write_all(prompt.as_bytes());
            let _ = w.write_all(b"\n");
        }
        self.rows += 1;
    }

    fn begin_named_file(&mut self) {
        if self.error.is_some() {
            return;
        }
        self.naming = match part(&self.dir, NAMING_PART) {
            Ok(w) => Some(w),
            Err(e) => {
                self.error = Some(e.to_string());
                None
            }
        };
    }

    /// Remove the part files, no assembly.
    // io: legacy effect site, listed in dev/census/effects-allowlist.json
    // default: legacy, untriaged: a Result error becomes a value here
    #[allow(clippy::disallowed_methods, clippy::let_underscore_must_use)]
    fn discard(self) {
        let _ = std::fs::remove_file(self.dir.join(VENDOR_PART));
        let _ = std::fs::remove_file(self.dir.join(NAMING_PART));
    }

    // io: legacy effect site, listed in dev/census/effects-allowlist.json
    // default: legacy, untriaged: a Result error becomes a value here
    #[allow(clippy::disallowed_methods, clippy::let_underscore_must_use)]
    fn close(self) -> Result<(), String> {
        if let Some(e) = self.error {
            return Err(e);
        }
        let LogFiles {
            dir,
            mut vendor,
            naming,
            rows,
            ..
        } = self;
        let mut naming = naming.ok_or("the naming part writer failed mid-run")?;
        vendor
            .flush()
            .and_then(|()| naming.flush())
            .map_err(|e| format!("write {PROMPTS}: {e}"))?;
        drop(vendor);
        drop(naming);
        let path = dir.join(PROMPTS);
        if rows == 0 {
            std::fs::write(&path, "\n").map_err(|e| format!("write {PROMPTS}: {e}"))?;
        } else {
            let mut out =
                std::fs::File::create(&path).map_err(|e| format!("write {PROMPTS}: {e}"))?;
            for name in [VENDOR_PART, NAMING_PART] {
                let mut r =
                    std::fs::File::open(dir.join(name)).map_err(|e| format!("read {name}: {e}"))?;
                std::io::copy(&mut r, &mut out).map_err(|e| format!("assemble {PROMPTS}: {e}"))?;
            }
        }
        let _ = std::fs::remove_file(dir.join(VENDOR_PART));
        let _ = std::fs::remove_file(dir.join(NAMING_PART));
        Ok(())
    }
}

/// Write the reason-labeled ask log (`--dump-asks`): the ask rows the run
/// recorded, in commit order, one JSON line each. Recording only — no
/// decision reads it, and no row exists unless the flag asks for it.
/// Returns the row count.
// io: legacy effect site, listed in dev/census/effects-allowlist.json
#[allow(clippy::disallowed_methods)]
pub fn write_ask_rows(path: &Path, rows: &[AskRow]) -> Result<usize, String> {
    let mut text = String::new();
    for row in rows {
        text.push_str(&serde_json::to_string(row).map_err(|e| format!("ask row: {e}"))?);
        text.push('\n');
    }
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
    }
    std::fs::write(path, text).map_err(|e| format!("write {}: {e}", path.display()))?;
    Ok(rows.len())
}

/// One ask-log row's material — small enough to retain for a whole run:
/// WHY the ask happened (`naming::ask_trace`'s taxonomy), what it asked,
/// and the context it carried. Never the prompt text, never the model's
/// answer — the `Asks` mode builds this at PREPARE time so an ask is
/// recorded without ever materializing the prompt.
pub struct AskRowParts {
    site: &'static str,
    scope: String,
    scope_kind: &'static str,
    reason: crate::naming::ask_trace::AskReason,
    is_retry: bool,
    /// A recorded cause wins (an applier-rejection seed, or a lane round-2
    /// seeded by the scope check — `AskSite::lane_reask`); otherwise a
    /// lane's round-2 is derived from the request's failure lists.
    retry_cause: Option<crate::naming::ask_trace::RetryCause>,
    retry_cause_detail: Option<String>,
    prior: bool,
    wave: Option<u64>,
    phase: u8,
    round: u64,
    identifiers: Vec<String>,
    used_names_count: usize,
    variant: &'static str,
}

impl AskRowParts {
    /// The row's fields, resolved one dispatch at a time.
    pub fn of(
        site: &'static str,
        kind: crate::naming::ask_trace::AskScope,
        scope: String,
        wave: Option<u64>,
        round: u64,
        ask: &AskSite,
        request: &BatchRenameRequest,
    ) -> AskRowParts {
        use crate::naming::ask_trace::{prior_context_of, reason_of};
        let is_retry = request.is_retry == Some(true);
        let reason = reason_of(ask, kind, request);
        let retry_cause = ask.cause.or_else(|| {
            if is_retry {
                request
                    .failures
                    .as_ref()
                    .and_then(crate::naming::ask_trace::RetryCause::of_failures)
            } else {
                None
            }
        });
        let scope_kind = match kind {
            AskScope::Fn => "fn",
            AskScope::Module => "module",
            AskScope::Sweep => "sweep",
            AskScope::Vendor => "vendor",
            AskScope::Folders => "folders",
            AskScope::Modules => "modules",
        };
        let variant = match (kind, is_retry) {
            (AskScope::Fn, false) => "batch",
            (AskScope::Fn, true) => "batch-retry",
            (AskScope::Module, false) => "module",
            (AskScope::Module, true) => "module-retry",
            (AskScope::Sweep, false) => "batch",
            (AskScope::Sweep, true) => "batch-retry",
            (AskScope::Vendor, _) => "vendor",
            (AskScope::Folders, _) => "folders",
            (AskScope::Modules, _) => "modules",
        };
        AskRowParts {
            site,
            scope,
            scope_kind,
            reason,
            is_retry,
            retry_cause,
            retry_cause_detail: ask.detail.clone(),
            prior: ask.prior || prior_context_of(request),
            wave,
            phase: ask.phase,
            round,
            identifiers: request.identifiers.clone(),
            used_names_count: request.used_names.len(),
            variant,
        }
    }

    /// The scope the round counter counts (the ask row's `scope`).
    pub fn scope(&self) -> &str {
        self.scope.as_str()
    }

    /// The round, assigned at commit (the functionId's call count so far).
    pub fn set_round(&mut self, round: u64) {
        self.round = round;
    }
}

/// One ask-log row (asks.jsonl).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AskRow {
    pub seq: u64,
    pub site: &'static str,
    pub scope: String,
    pub scope_kind: &'static str,
    pub reason: &'static str,
    pub is_retry: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_cause: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_cause_detail: Option<String>,
    pub prior_context: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wave: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<u8>,
    pub round: u64,
    pub identifiers: Vec<String>,
    pub used_names_count: usize,
    pub prompt_variant: &'static str,
}

/// One ask-log row from its parts.
pub fn ask_row(seq: u64, parts: &AskRowParts) -> AskRow {
    AskRow {
        seq,
        site: parts.site,
        scope: parts.scope.clone(),
        scope_kind: parts.scope_kind,
        reason: parts.reason.as_str(),
        is_retry: parts.is_retry,
        retry_cause: parts.retry_cause.map(|c| c.as_str()),
        retry_cause_detail: parts.retry_cause_detail.clone(),
        prior_context: parts.prior,
        wave: parts.wave,
        phase: parts.wave.map(|_| parts.phase),
        round: parts.round,
        identifiers: parts.identifiers.clone(),
        used_names_count: parts.used_names_count,
        prompt_variant: parts.variant,
    }
}

/// One ask-log row per dispatch, `seq` in recording order — the bulk form
/// (the tests' oracle); a run records its rows through
/// [`DispatchLog::record`].
pub fn ask_rows(dispatches: &[Dispatch<'_>]) -> Vec<AskRow> {
    let mut rounds: HashMap<String, u64> = HashMap::new();
    dispatches
        .iter()
        .enumerate()
        .map(|(seq, d)| {
            let round = round_of(d, &mut rounds);
            ask_row(seq as u64, &ask_parts_rounded(d, round))
        })
        .collect()
}

/// [`AskRowParts::of`] from any committed dispatch, `round` already
/// counted by [`round_of`] (the waves carry their own; every other site
/// counts rounds per functionId).
fn ask_parts_rounded(d: &Dispatch<'_>, round: u64) -> AskRowParts {
    match d {
        Dispatch::Naming(r) => {
            let kind = if r.function_id.starts_with("module-binding-batch:") {
                AskScope::Module
            } else {
                AskScope::Fn
            };
            AskRowParts::of(
                "naming",
                kind,
                r.function_id.clone(),
                Some(r.wave),
                r.round,
                &r.ask,
                &r.request,
            )
        }
        Dispatch::Sweep(_, s) => AskRowParts::of(
            "sweep",
            AskScope::Sweep,
            "coverage-sweep".to_string(),
            None,
            round,
            &s.ask,
            &s.request,
        ),
        Dispatch::Plain {
            function_id,
            site,
            call,
        } => AskRowParts::of(
            site,
            match *site {
                "folders" => AskScope::Folders,
                crate::place::assign::namer::MODULE_NAMER_SITE => AskScope::Modules,
                _ => AskScope::Vendor,
            },
            function_id.to_string(),
            None,
            round,
            &AskSite::fresh(0),
            &call.request,
        ),
    }
}

/// The dump's per-functionId round counter (a sweep's / a namer's rounds
/// are their call count; the waves carry their own).
fn rounds_of(rounds: &mut HashMap<String, u64>, function_id: &str) -> u64 {
    let counted = rounds.entry(function_id.to_string()).or_insert(0);
    *counted += 1;
    *counted
}

/// One dispatch's dump round: the waves carry their own (the processor's
/// counter); every other site counts per functionId.
fn round_of(d: &Dispatch<'_>, rounds: &mut HashMap<String, u64>) -> u64 {
    match d {
        Dispatch::Naming(r) => r.round,
        Dispatch::Sweep(..) => rounds_of(rounds, "coverage-sweep"),
        Dispatch::Plain { function_id, .. } => rounds_of(rounds, function_id),
    }
}

/// One prompt's target: the identifier's session id (or name) and span in
/// the text the prompt was built over.
#[derive(Serialize)]
struct PromptTarget<'a> {
    #[serde(rename = "sessionId")]
    session_id: &'a str,
    start: u32,
    end: u32,
    text: &'a str,
}

/// One prompts.jsonl row.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PromptRow<'a> {
    seq: u64,
    function_id: &'a str,
    site: &'a str,
    round: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    wave: Option<u64>,
    is_retry: bool,
    cache_key: String,
    system_prompt: &'a str,
    user_prompt: &'a str,
    identifiers: &'a [String],
    targets: Vec<PromptTarget<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    targets_text: Option<&'a str>,
}

/// One dispatch's prompts.jsonl row: the row the `Full` mode streams at
/// commit ([`DispatchLog::record`]) — `seq` is its recording index,
/// `round` counted per functionId across every site.
pub fn prompt_row(seq: u64, round: u64, d: &Dispatch<'_>, params: &CacheKeyParams) -> String {
    let (function_id, request, system, user, cache_key) = match d {
        Dispatch::Naming(r) => (
            r.function_id.as_str(),
            &r.request,
            r.system_prompt.as_str(),
            r.user_prompt.as_str(),
            r.cache_key.clone(),
        ),
        Dispatch::Sweep(_, s) => (
            "coverage-sweep",
            &s.request,
            s.system_prompt.as_str(),
            s.user_prompt.as_str(),
            s.cache_key.clone(),
        ),
        Dispatch::Plain {
            function_id, call, ..
        } => (
            *function_id,
            &call.request,
            call.system_prompt.as_str(),
            call.user_prompt.as_str(),
            call.cache_key(params),
        ),
    };
    let targets = match d {
        Dispatch::Naming(r) => r
            .targets
            .iter()
            .map(|(sid, s)| PromptTarget {
                session_id: sid,
                start: s.start,
                end: s.end,
                text: "fresh",
            })
            .collect(),
        Dispatch::Sweep(a, s) => s
            .targets
            .iter()
            .map(|(name, sp)| PromptTarget {
                session_id: name,
                start: sp.start,
                end: sp.end,
                text: a.as_str(),
            })
            .collect(),
        Dispatch::Plain { .. } => Vec::new(),
    };
    let row = PromptRow {
        seq,
        function_id,
        site: DispatchLog::site_of(d),
        round,
        wave: match d {
            Dispatch::Naming(r) => Some(r.wave),
            _ => None,
        },
        is_retry: request.is_retry == Some(true),
        cache_key,
        system_prompt: system,
        user_prompt: user,
        identifiers: &request.identifiers,
        targets,
        targets_text: match d {
            Dispatch::Sweep(a, _) => Some(a.as_str()),
            _ => None,
        },
    };
    serde_json::to_string(&row).expect("a prompt row serializes")
}

/// prompts.jsonl, every row in recording order — the bulk form (the
/// tests' oracle; a run streams its rows through [`DispatchLog::record`]).
/// No rows is one newline.
pub fn dispatch_rows(dispatches: &[Dispatch<'_>], params: &CacheKeyParams) -> String {
    if dispatches.is_empty() {
        return "\n".to_string();
    }
    let mut prompts = String::new();
    let mut rounds: HashMap<String, u64> = HashMap::new();
    for (seq, d) in dispatches.iter().enumerate() {
        prompts.push_str(&prompt_row(seq as u64, round_of(d, &mut rounds), d, params));
        prompts.push('\n');
    }
    prompts
}

#[cfg(test)]
mod artifact_dump_test;
