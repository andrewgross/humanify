//! The PRIOR-CARRIED placement regime — TS `assignWithPrior` and the
//! `PLACEMENT_TIERS` registry (src/split/stable-split.ts :404-1067): each
//! statement inherits the file its evidence names, strongest evidence
//! first, and the first tier that claims it wins:
//!
//!   hash → preempt → anchorPreempt → ordinal → name → allsame → fill →
//!   anchor → conflict → novote
//!
//! Every tier abstains rather than guesses (a statement in the WRONG file
//! churns two files plus every importer); the last two place by locality
//! (the preceding statement's file) and the last never abstains. The order
//! and the refusals are the TS's, measured decisions (exp041–043 content
//! anchor / anchor preempt / near-identical, exp058 shapeless refusal) —
//! reproduced, not improved.
//!
//! Decision code is sequential in bundle order: the per-name ordinal
//! cursors advance for EVERY statement (hash-inherited ones included), and
//! the locality fallback reads the previous statement's decision.

use std::collections::HashMap;

use serde_json::Value;

use super::anchor::{PriorStatement, content_anchor_verdicts};
use super::declared::{declared_names, outer_declared_names};
use super::ledger::StableSplitLedger;
use super::stems::{has_minted_number, is_rejected_stem};
use super::trail::{PlacementEvidence, PlacementTrail, TrailEntry};
use crate::hash::statement_hash::STATEMENT_HASH_VERSION;

/// The placement kill switches this regime reads (`--disable <name>`),
/// resolved by the CLI; `true` = the switch is thrown (the tier is off).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlacementSwitches {
    pub content_anchor: bool,
    pub anchor_preempt: bool,
    pub anchor_nearident: bool,
    pub allsame_vote: bool,
    pub empty_decl_hash_guard: bool,
}

/// What the rename matcher carried across (`PriorCarry`): the prior
/// release's top-level statement texts (zip with `prior.order`) and
/// final-name → matched-prior-name (lookups only).
#[derive(Clone, Debug, Default)]
pub struct PriorCarry {
    pub statement_texts: Vec<String>,
    pub match_map: HashMap<String, String>,
}

/// `PlacementTierName`, in the registry's evidence order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tier {
    Hash,
    Preempt,
    AnchorPreempt,
    Ordinal,
    Name,
    AllSame,
    Fill,
    Anchor,
    Conflict,
    NoVote,
}

/// `PLACEMENT_TIERS` order.
pub const PLACEMENT_TIERS: [Tier; 10] = [
    Tier::Hash,
    Tier::Preempt,
    Tier::AnchorPreempt,
    Tier::Ordinal,
    Tier::Name,
    Tier::AllSame,
    Tier::Fill,
    Tier::Anchor,
    Tier::Conflict,
    Tier::NoVote,
];

impl Tier {
    pub fn name(self) -> &'static str {
        match self {
            Tier::Hash => "hash",
            Tier::Preempt => "preempt",
            Tier::AnchorPreempt => "anchorPreempt",
            Tier::Ordinal => "ordinal",
            Tier::Name => "name",
            Tier::AllSame => "allsame",
            Tier::Fill => "fill",
            Tier::Anchor => "anchor",
            Tier::Conflict => "conflict",
            Tier::NoVote => "novote",
        }
    }

    /// How the run log names it ("N via hashes").
    pub fn label(self) -> &'static str {
        match self {
            Tier::Hash => "hashes",
            Tier::Preempt => "identity preempts",
            Tier::AnchorPreempt => "anchor preempts",
            Tier::Ordinal => "ordinals",
            Tier::Name => "name votes",
            Tier::AllSame => "all-same votes",
            Tier::Fill => "identity fills",
            Tier::Anchor => "content anchors",
            Tier::Conflict => "conflicts",
            Tier::NoVote => "no votes",
        }
    }

    /// `LOCALITY_TIERS`: placed by locality rather than evidence.
    pub fn is_locality(self) -> bool {
        matches!(self, Tier::Conflict | Tier::NoVote)
    }
}

/// `StableSplitStats`' placement counters.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TierStats {
    pub inherited: usize,
    pub residue_locality: usize,
    /// Per tier, in registry order.
    pub by_tier: [usize; 10],
}

impl TierStats {
    fn record(&mut self, tier: Tier) {
        let at = PLACEMENT_TIERS
            .iter()
            .position(|t| *t == tier)
            .expect("a registry tier");
        self.by_tier[at] += 1;
        if tier.is_locality() {
            self.residue_locality += 1;
        } else {
            self.inherited += 1;
        }
    }
}

/// `placementSummary`: every evidence tier that placed anything, in order,
/// then the locality residue.
pub fn placement_summary(stats: &TierStats) -> String {
    let mut parts: Vec<String> = PLACEMENT_TIERS
        .iter()
        .zip(stats.by_tier)
        .filter(|(t, n)| !t.is_locality() && *n > 0)
        .map(|(t, n)| format!("{n} via {}", t.label()))
        .collect();
    parts.push(format!("{} residue by locality", stats.residue_locality));
    parts.join(", ")
}

/// `HashMiss`.
fn hash_miss_name(m: HashMiss) -> &'static str {
    match m {
        HashMiss::NoPriorHashes => "no-prior-hashes",
        HashMiss::Absent => "absent",
        HashMiss::Count => "count",
        HashMiss::Split => "split",
        HashMiss::Shapeless => "shapeless",
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HashMiss {
    NoPriorHashes,
    Absent,
    Count,
    Split,
    Shapeless,
}

/// `carriesNoContent`: a declaration with no initializers masks to a
/// declarator count and nothing else (exp058).
fn carries_no_content(stmt: &Value) -> bool {
    stmt.get("type").and_then(Value::as_str) == Some("VariableDeclaration")
        && stmt
            .get("declarations")
            .and_then(Value::as_array)
            .is_some_and(|d| {
                !d.is_empty() && d.iter().all(|d| d.get("init").is_none_or(Value::is_null))
            })
}

/// All entries equal (`files.every(f => f === files[0])`), non-empty.
fn unanimous(files: &[String]) -> Option<&String> {
    let first = files.first()?;
    files.iter().all(|f| f == first).then_some(first)
}

/// `hashTier`: per statement, the single prior home of its hash, or why
/// the lookup failed.
fn hash_tier(
    body: &[Value],
    hashes: &[String],
    prior: &StableSplitLedger,
    switches: PlacementSwitches,
) -> Vec<Result<String, HashMiss>> {
    let usable = prior.hashes.as_ref().filter(|h| {
        prior.hash_version == Some(STATEMENT_HASH_VERSION) && h.len() == prior.order.len()
    });
    let Some(prior_hashes) = usable else {
        return vec![Err(HashMiss::NoPriorHashes); hashes.len()];
    };
    let mut prior_files: HashMap<&str, Vec<String>> = HashMap::new();
    for (h, f) in prior_hashes.iter().zip(&prior.order) {
        prior_files.entry(h).or_default().push(f.clone());
    }
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for h in hashes {
        *counts.entry(h).or_default() += 1;
    }
    let guard = !switches.empty_decl_hash_guard;
    hashes
        .iter()
        .zip(body)
        .map(|(h, stmt)| {
            if guard && carries_no_content(stmt) {
                return Err(HashMiss::Shapeless);
            }
            let files = prior_files.get(h.as_str()).ok_or(HashMiss::Absent)?;
            if Some(&files.len()) != counts.get(h.as_str()) {
                return Err(HashMiss::Count);
            }
            unanimous(files).cloned().ok_or(HashMiss::Split)
        })
        .collect()
}

/// `identityTier`: the prior file pinned by cross-version BINDING identity
/// — unanimous over the declared names' matched prior counterparts.
fn identity_tier(
    names: &[Vec<String>],
    match_map: &HashMap<String, String>,
    prior_names: &HashMap<String, Vec<String>>,
    skip_generic_new_names: bool,
) -> Vec<Option<String>> {
    if match_map.is_empty() {
        return vec![None; names.len()];
    }
    names
        .iter()
        .map(|declared| {
            let mut votes: Vec<&String> = Vec::new();
            for name in declared {
                if skip_generic_new_names && is_rejected_stem(name) {
                    continue;
                }
                let Some(file) = match_map
                    .get(name)
                    .and_then(|prior_name| prior_names.get(prior_name))
                    .and_then(|files| unanimous(files))
                else {
                    continue;
                };
                if !votes.contains(&file) {
                    votes.push(file);
                }
            }
            (votes.len() == 1).then(|| votes[0].clone())
        })
        .collect()
}

/// The anchor's per-statement output (`AnchorTier`).
struct AnchorTier {
    file: Vec<Option<String>>,
    near_identical: Vec<bool>,
}

/// `contentAnchorTier`: off when switched off, or when the prior texts do
/// not zip with the ledger.
fn content_anchor_tier(
    fresh_texts: &[&str],
    prior: &StableSplitLedger,
    carry: Option<&PriorCarry>,
    switches: PlacementSwitches,
) -> AnchorTier {
    let n = fresh_texts.len();
    let mut tier = AnchorTier {
        file: vec![None; n],
        near_identical: vec![false; n],
    };
    let Some(texts) = carry.map(|c| &c.statement_texts) else {
        return tier;
    };
    if switches.content_anchor || texts.len() != prior.order.len() {
        return tier;
    }
    let prior_statements: Vec<PriorStatement> = texts
        .iter()
        .zip(&prior.order)
        .map(|(text, file)| PriorStatement { text, file })
        .collect();
    let verdicts = content_anchor_verdicts(&prior_statements, fresh_texts);
    for (i, slot) in tier.file.iter_mut().enumerate() {
        if let Some(v) = verdicts.get(&i) {
            *slot = Some(v.file.clone());
            tier.near_identical[i] = v.near_identical;
        }
    }
    tier
}

/// `anchorPreemptTier`: the anchor verdict promoted above the name vote
/// when the twin is near-identical, or every OUTER declared name carries a
/// minted counter.
fn anchor_preempt_tier(
    body: &[Value],
    anchor: &AnchorTier,
    switches: PlacementSwitches,
) -> Vec<Option<String>> {
    if switches.anchor_preempt {
        return vec![None; body.len()];
    }
    let near_ident_enabled = !switches.anchor_nearident;
    body.iter()
        .enumerate()
        .map(|(i, stmt)| {
            let file = anchor.file[i].as_ref()?;
            if near_ident_enabled && anchor.near_identical[i] {
                return Some(file.clone());
            }
            let outer = outer_declared_names(stmt);
            (!outer.is_empty() && outer.iter().all(|n| has_minted_number(n))).then(|| file.clone())
        })
        .collect()
}

/// `PriorTiers`.
struct PriorTiers {
    via_hash: Vec<Result<String, HashMiss>>,
    via_identity: Vec<Option<String>>,
    via_identity_preempt: Vec<Option<String>>,
    via_anchor: Vec<Option<String>>,
    via_anchor_preempt: Vec<Option<String>>,
}

/// `VoteOutcome`.
struct VoteOutcome {
    name_vote: Option<String>,
    all_same_vote: Option<String>,
    used_ordinal: bool,
    votes_size: usize,
}

/// `PlacementContext`.
struct Ctx<'a> {
    i: usize,
    tiers: &'a PriorTiers,
    vote: &'a VoteOutcome,
    fallback: Option<&'a str>,
}

/// A preempt tier fires only when it DISAGREES with the name vote.
fn preempting(preempt: Option<&String>, name_vote: Option<&String>) -> Option<String> {
    let (p, v) = (preempt?, name_vote?);
    (p != v).then(|| p.clone())
}

/// One tier's `decide`.
fn decide(tier: Tier, c: &Ctx) -> Option<String> {
    match tier {
        Tier::Hash => c.tiers.via_hash[c.i].as_ref().ok().cloned(),
        Tier::Preempt => preempting(
            c.tiers.via_identity_preempt[c.i].as_ref(),
            c.vote.name_vote.as_ref(),
        ),
        Tier::AnchorPreempt => preempting(
            c.tiers.via_anchor_preempt[c.i].as_ref(),
            c.vote.name_vote.as_ref(),
        ),
        Tier::Ordinal => c
            .vote
            .used_ordinal
            .then(|| c.vote.name_vote.clone())
            .flatten(),
        Tier::Name => (!c.vote.used_ordinal)
            .then(|| c.vote.name_vote.clone())
            .flatten(),
        Tier::AllSame => c.vote.all_same_vote.clone(),
        Tier::Fill => (c.vote.votes_size == 0)
            .then(|| c.tiers.via_identity[c.i].clone())
            .flatten(),
        Tier::Anchor => c.tiers.via_anchor[c.i].clone(),
        Tier::Conflict => (c.vote.votes_size > 1)
            .then(|| c.fallback.map(str::to_string))
            .flatten(),
        Tier::NoVote => c.fallback.map(str::to_string),
    }
}

/// `tierVerdicts`: what every non-winning, non-locality tier would claim.
fn tier_verdicts(c: &Ctx, winner: Tier) -> Vec<(String, String)> {
    PLACEMENT_TIERS
        .iter()
        .filter(|t| **t != winner && !t.is_locality())
        .filter_map(|t| decide(*t, c).map(|f| (t.name().to_string(), f)))
        .collect()
}

/// `priorHome`: the strongest IDENTITY evidence of where this lived.
fn prior_home(c: &Ctx) -> Option<(String, &'static str)> {
    if let Ok(f) = &c.tiers.via_hash[c.i] {
        return Some((f.clone(), "hash"));
    }
    if let Some(f) = &c.tiers.via_identity[c.i] {
        return Some((f.clone(), "identity"));
    }
    c.tiers.via_anchor[c.i].clone().map(|f| (f, "anchor"))
}

/// The per-name vote state (`seen` ordinals + the maps it reads).
struct Voter<'a> {
    seen: HashMap<String, usize>,
    prior_names: &'a HashMap<String, Vec<String>>,
    new_counts: HashMap<String, usize>,
}

impl Voter<'_> {
    /// `statementVotes` for one statement's declared names: the vote set,
    /// the all-same subset (names with ONE prior home), whether an ordinal
    /// vote was used. Sets in insertion order.
    fn votes(&mut self, names: &[String]) -> (Vec<String>, Vec<String>, bool) {
        let mut votes: Vec<String> = Vec::new();
        let mut all_same: Vec<String> = Vec::new();
        let mut used_ordinal = false;
        for name in names {
            let ordinal = self.seen.get(name).copied().unwrap_or(0);
            self.seen.insert(name.clone(), ordinal + 1);
            let Some(files) = self.prior_names.get(name).filter(|f| !f.is_empty()) else {
                continue;
            };
            let (file, ordinal_vote) = if let Some(first) = unanimous(files) {
                (first.clone(), false)
            } else if self.new_counts.get(name) == Some(&files.len()) && ordinal < files.len() {
                (files[ordinal].clone(), true)
            } else {
                continue;
            };
            if !votes.contains(&file) {
                votes.push(file.clone());
            }
            if ordinal_vote {
                used_ordinal = true;
            } else if !all_same.contains(&file) {
                all_same.push(file);
            }
        }
        (votes, all_same, used_ordinal)
    }
}

/// What [`assign_with_prior`] reads beside the body.
pub struct TierInput<'a> {
    pub body: &'a [Value],
    pub spans: &'a [(u32, u32)],
    pub hashes: &'a [String],
    /// The shipped text the spans index (the anchor reads statement texts).
    pub code: &'a str,
    pub prior: &'a StableSplitLedger,
    pub carry: Option<&'a PriorCarry>,
    pub switches: PlacementSwitches,
}

/// `assignWithPrior`: inherit prior assignments; residue follows its
/// preceding neighbour.
pub fn assign_with_prior(
    input: &TierInput,
    mut trail: Option<&mut PlacementTrail>,
) -> Result<(Vec<String>, TierStats), String> {
    let body = input.body;
    let names: Vec<Vec<String>> = body.iter().map(declared_names).collect();
    let fresh_texts: Vec<&str> = input
        .spans
        .iter()
        .map(|&(s, e)| &input.code[s as usize..e as usize])
        .collect();
    let empty = HashMap::new();
    let match_map = input.carry.map_or(&empty, |c| &c.match_map);
    let prior_names = &input.prior.name_to_files;
    let anchor = content_anchor_tier(&fresh_texts, input.prior, input.carry, input.switches);
    let tiers = PriorTiers {
        via_hash: hash_tier(body, input.hashes, input.prior, input.switches),
        via_identity: identity_tier(&names, match_map, prior_names, false),
        via_identity_preempt: identity_tier(&names, match_map, prior_names, true),
        via_anchor_preempt: anchor_preempt_tier(body, &anchor, input.switches),
        via_anchor: anchor.file,
    };
    let mut new_counts: HashMap<String, usize> = HashMap::new();
    for n in names.iter().flatten() {
        *new_counts.entry(n.clone()).or_default() += 1;
    }
    let mut voter = Voter {
        seen: HashMap::new(),
        prior_names,
        new_counts,
    };
    let all_same_enabled = !input.switches.allsame_vote;
    let mut assignment: Vec<String> = Vec::with_capacity(body.len());
    let mut stats = TierStats::default();
    for i in 0..body.len() {
        // Always vote: the per-name ordinal cursors must advance for every
        // statement, hash-inherited ones included.
        let (votes, all_same, used_ordinal) = voter.votes(&names[i]);
        let vote = VoteOutcome {
            name_vote: (votes.len() == 1).then(|| votes[0].clone()),
            all_same_vote: (all_same_enabled && all_same.len() == 1).then(|| all_same[0].clone()),
            used_ordinal,
            votes_size: votes.len(),
        };
        let fallback = match i {
            0 => input.prior.files.first().map(String::as_str),
            _ => Some(assignment[i - 1].as_str()),
        };
        let ctx = Ctx {
            i,
            tiers: &tiers,
            vote: &vote,
            fallback,
        };
        let (tier, file) = PLACEMENT_TIERS
            .iter()
            .find_map(|t| decide(*t, &ctx).map(|f| (*t, f)))
            .ok_or("stable split: no placement tier claimed the statement")?;
        stats.record(tier);
        if let Some(trail) = trail.as_deref_mut() {
            let home = prior_home(&ctx);
            trail.record(TrailEntry {
                index: i,
                span: Some(input.spans[i]),
                names: names[i].clone(),
                placed_by: tier.name().to_string(),
                file: file.clone(),
                prior_file: home.as_ref().map(|(f, _)| f.clone()),
                prior_file_from: home.map(|(_, from)| from),
                hash_miss: tiers.via_hash[i].as_ref().err().map(|m| hash_miss_name(*m)),
                alternatives: Some(tier_verdicts(&ctx, tier)),
                evidence: PlacementEvidence {
                    votes: Some(votes),
                    all_same: Some(all_same),
                    anchor: tiers.via_anchor[i].clone(),
                },
            });
        }
        assignment.push(file);
    }
    Ok((assignment, stats))
}
