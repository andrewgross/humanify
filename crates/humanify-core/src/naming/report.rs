//! The naming stage's reports (WP4.6) — TS originals: the `RenameReport` /
//! `IdentifierOutcome` records the processor and the library-prefix pass
//! build (`src/analysis/types.ts`, `src/rename/processor.ts`),
//! `src/rename/coverage.ts` (the coverage summary + its printed block),
//! `src/rename/name-contention.ts` (the contention events) and
//! `src/rename/diagnostics.ts` (the `--diagnostics` JSON). No decision
//! reads any of it: it is what the run SAYS it did, and the gate compares
//! it to the TS byte for byte (stats.json + diag.json + stdout).

pub mod coverage;
pub mod diagnostics;

/// One round of an identifier's attempt history (`RenameAttempt`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoundAttempt {
    pub round: u64,
    pub proposed: Option<String>,
    pub result: AttemptResult,
}

/// `RenameAttempt.result`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttemptResult {
    Applied,
    Duplicate,
    Invalid,
    Unchanged,
    Missing,
}

impl AttemptResult {
    pub fn as_str(self) -> &'static str {
        match self {
            AttemptResult::Applied => "applied",
            AttemptResult::Duplicate => "duplicate",
            AttemptResult::Invalid => "invalid",
            AttemptResult::Unchanged => "unchanged",
            AttemptResult::Missing => "missing",
        }
    }
}

/// `IdentifierOutcome` (its status union).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    Renamed {
        new_name: String,
        round: u64,
    },
    Unchanged {
        attempts: u64,
        suggestion: Option<String>,
    },
    Missing {
        attempts: u64,
        last_finish_reason: Option<String>,
    },
    Duplicate {
        conflicted_with: String,
        attempts: u64,
        suggestion: Option<String>,
    },
    Invalid {
        attempts: u64,
        suggestion: Option<String>,
    },
}

/// One identifier's outcome with its round trail (`trail` absent when no
/// attempt was recorded).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IdentifierOutcome {
    pub status: Status,
    pub trail: Option<Vec<RoundAttempt>>,
}

impl IdentifierOutcome {
    pub fn renamed(new_name: &str, round: u64, trail: Option<Vec<RoundAttempt>>) -> Self {
        IdentifierOutcome {
            status: Status::Renamed {
                new_name: new_name.to_string(),
                round,
            },
            trail,
        }
    }

    pub fn is_renamed(&self) -> bool {
        matches!(self.status, Status::Renamed { .. })
    }
}

/// `Record<string, IdentifierOutcome>` in JS object order: assigning an
/// existing key keeps its position (identifiers are never index keys).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Outcomes(pub Vec<(String, IdentifierOutcome)>);

impl Outcomes {
    /// `outcomes[name] = outcome`.
    pub fn set(&mut self, name: &str, outcome: IdentifierOutcome) {
        match self.0.iter_mut().find(|(k, _)| k == name) {
            Some(slot) => slot.1 = outcome,
            None => self.0.push((name.to_string(), outcome)),
        }
    }

    pub fn has(&self, name: &str) -> bool {
        self.0.iter().any(|(k, _)| k == name)
    }

    /// `Object.assign(this, other)`.
    pub fn assign(&mut self, other: Outcomes) {
        for (k, v) in other.0 {
            self.set(&k, v);
        }
    }

    /// `mergeOutcomeMaps(this, other)`: a colliding name gets the first
    /// free `name#2`, `name#3`… key (`#` is never in an identifier).
    pub fn merge(&mut self, other: Outcomes) {
        for (name, outcome) in other.0 {
            let mut key = name.clone();
            let mut n = 2;
            while self.has(&key) {
                key = format!("{name}#{n}");
                n += 1;
            }
            self.0.push((key, outcome));
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &(String, IdentifierOutcome)> {
        self.0.iter()
    }
}

/// `RenameReport.type`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReportType {
    Function,
    ModuleBinding,
}

/// `RenameReport.strategy`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReportStrategy {
    Llm,
    LibraryPrefix,
}

impl ReportStrategy {
    pub fn as_str(self) -> &'static str {
        match self {
            ReportStrategy::Llm => "llm",
            ReportStrategy::LibraryPrefix => "library-prefix",
        }
    }
}

/// `RenameReport`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenameReport {
    pub ty: ReportType,
    pub strategy: ReportStrategy,
    pub target_id: String,
    pub total_identifiers: usize,
    pub renamed_count: usize,
    pub outcomes: Outcomes,
    pub total_llm_calls: Option<u64>,
    pub finish_reasons: Vec<Option<String>>,
    pub structural_hash: Option<String>,
}

impl RenameReport {
    /// `mergeRenameReports(self, other)` (main pass + shadowed pass).
    pub fn merge(&mut self, other: RenameReport) {
        self.total_identifiers += other.total_identifiers;
        self.renamed_count += other.renamed_count;
        self.outcomes.merge(other.outcomes);
        self.total_llm_calls =
            Some(self.total_llm_calls.unwrap_or(0) + other.total_llm_calls.unwrap_or(0));
        self.finish_reasons.extend(other.finish_reasons);
    }

    /// `fixupRenamedCount`.
    pub fn fixup_renamed_count(&mut self) {
        self.renamed_count = self.outcomes.iter().filter(|(_, o)| o.is_renamed()).count();
    }

    /// `bumpRetryCallCount`.
    pub fn bump_retry_call(&mut self, finish_reason: Option<String>) {
        self.total_llm_calls = Some(self.total_llm_calls.unwrap_or(0) + 1);
        self.finish_reasons.push(finish_reason);
    }
}

/// One contention event (`NameContentionEvent`): a requested name was
/// already held, and the collision ladder decorated it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContentionEvent {
    pub requested: String,
    pub resolved_to: String,
    pub old_name: String,
    /// "wave" | "remaining".
    pub site: &'static str,
}

/// `RenameProcessor`'s skip counters (`skipReasons`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SkipReasons {
    pub zero_bindings: usize,
    pub all_preserved: usize,
    pub error: usize,
}

/// What the processor reports besides its decisions.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProcessorReport {
    /// `processor.reports`: module reports in settle order, then function
    /// reports in graph node order.
    pub reports: Vec<RenameReport>,
    pub failed: usize,
    pub skipped_by_skip_list: usize,
    pub skip_reasons: SkipReasons,
    pub contention: Vec<ContentionEvent>,
    /// The metrics' `completedCalls`: wave dispatches that resolved.
    pub completed_calls: usize,
}
