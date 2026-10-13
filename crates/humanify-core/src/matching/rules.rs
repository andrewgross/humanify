//! The decision layer's rule lists, one per PROFILE
//! (docs/matching-architecture.md §3.5, step 4).
//!
//! A matcher is a query against a reference store
//! ([`super::store::ReferenceStore`]) decided by an ordered list of rules.
//! Every rule DECLARES what it reads ([`Evidence`]) — for graph evidence,
//! in which region scope ([`Scope`]: INSIDE, INTO, OUT-OF the region the
//! rule decides for). Two profiles:
//!
//! - **cross-version** ([`cross_version`]): today's cascade, rung for rung
//!   in today's order (`matching::cascade` executes [`RESOLVE`]'s rungs off
//!   this list; the passes around them are listed in the order
//!   `match_functions` / `prior::match_prior_version` run them). The prior
//!   is ONE program, so its rules read every edge ([`Evidence::GraphAll`]):
//!   restricting vendor regions to INSIDE evidence is step 5, with an eval.
//! - **library**: the index lookup (exp095/097) and
//!   discovery's fixpoint. Caller evidence crossing the app→library
//!   boundary describes the APP (Andrew, 2026-10-13), so the library
//!   profile REFUSES any rule that reads INTO edges, or every edge, as
//!   identity evidence ([`Profile::admit`]). Only a [`Stage::Check`] — a
//!   post-claim consistency report that cannot add or drop a claim — may
//!   read INTO.
//!
//! Every decision either profile makes is recorded through ONE constructor,
//! [`RuleList::decide`]: (query piece, reference piece, deciding rule, what
//! that rule reads). A rule that is not on its profile's list cannot
//! decide anything.

use serde::Serialize;

use crate::graph::edges::Scope;

/// What a rule may read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Evidence {
    /// A fingerprint lookup under a named hash form (`hash::forms`).
    Fingerprint(&'static str),
    /// A caller-supplied resolver (cross-version's identity rung).
    Resolver,
    /// The function's member key (`matching::member_key`).
    MemberKey,
    /// The enclosing statement's hash (`matching::statement_context`).
    StatementContext,
    /// Version-stable features: property accesses, external calls, string
    /// literals (`matching::features`).
    Features,
    /// The library literal table (`libindex::literals`).
    Literals,
    /// A piece's size or its module's (floors, specificity).
    Size,
    /// Position among the bundle's pieces (neighbour support, ordinals).
    Position,
    /// The piece's own text (package readers, banners).
    Text,
    /// The package family table (`libindex::families`).
    Families,
    /// Decisions already made (propagation over the profile's own claims or
    /// matches).
    Claims,
    /// Graph edges in one scope, relative to the region the rule decides
    /// for. An OUT-OF edge is read in abstract form only: the callee's
    /// package or "a runtime helper", never a local name.
    Graph(Scope),
    /// Every graph edge, scope unread (cross-version only).
    GraphAll,
}

impl Evidence {
    /// The evidence's spelling in a report (`graph:inside`).
    pub fn spelling(self) -> String {
        match self {
            Evidence::Fingerprint(form) => format!("fingerprint:{form}"),
            Evidence::Resolver => "resolver".into(),
            Evidence::MemberKey => "member-key".into(),
            Evidence::StatementContext => "statement-context".into(),
            Evidence::Features => "features".into(),
            Evidence::Literals => "literals".into(),
            Evidence::Size => "size".into(),
            Evidence::Position => "position".into(),
            Evidence::Text => "text".into(),
            Evidence::Families => "families".into(),
            Evidence::Claims => "claims".into(),
            Evidence::Graph(Scope::Inside) => "graph:inside".into(),
            Evidence::Graph(Scope::Into) => "graph:into".into(),
            Evidence::Graph(Scope::OutOf) => "graph:out-of".into(),
            Evidence::GraphAll => "graph:all".into(),
        }
    }

    /// Does this evidence read caller edges from outside the region (INTO,
    /// or every edge)?
    pub fn reads_into(self) -> bool {
        matches!(self, Evidence::Graph(Scope::Into) | Evidence::GraphAll)
    }
}

/// What a rule does with what it reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Stage {
    /// Proposes candidates (a hash bucket, an index hit).
    Candidates,
    /// Picks one candidate, or narrows the pool, or parks it ambiguous.
    Resolve,
    /// Takes decisions back (injectivity, crossed containers, the isolation
    /// veto, containment).
    Guard,
    /// Decides from decisions already made.
    Propagate,
    /// A claim from the piece's own evidence (the library profile's seeds).
    Seed,
    /// A post-claim consistency report: reads, never decides.
    Check,
    /// Says something about code it does not claim (library class, the
    /// lookup queue).
    Report,
}

/// One rule: its name (the deciding-rule spelling a trail records), its
/// stage and what it reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RuleSpec {
    pub name: &'static str,
    pub stage: Stage,
    pub reads: &'static [Evidence],
}

impl RuleSpec {
    pub const fn new(name: &'static str, stage: Stage, reads: &'static [Evidence]) -> RuleSpec {
        RuleSpec { name, stage, reads }
    }

    pub fn may_read(&self, e: Evidence) -> bool {
        self.reads.contains(&e)
    }
}

/// Which profile a rule list serves.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Profile {
    CrossVersion,
    Library,
}

/// A rule a profile will not run, and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal {
    pub profile: Profile,
    pub rule: &'static str,
    pub why: String,
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:?} profile refuses rule {}: {}",
            self.profile, self.rule, self.why
        )
    }
}

impl Profile {
    /// May this profile run `rule`? The library profile refuses a rule that
    /// reads INTO edges (or every edge) unless it is a [`Stage::Check`]: the
    /// app's calls into a library are not the library's identity, and the
    /// index cannot hold them.
    pub fn admit(self, rule: &RuleSpec) -> Result<(), Refusal> {
        if self == Profile::Library
            && rule.stage != Stage::Check
            && let Some(e) = rule.reads.iter().find(|e| e.reads_into())
        {
            return Err(Refusal {
                profile: self,
                rule: rule.name,
                why: format!(
                    "it reads {} as identity evidence; caller edges from outside a library \
                     region describe the app (only a post-claim check may read them)",
                    e.spelling()
                ),
            });
        }
        Ok(())
    }
}

/// An ordered rule list for one profile; built only through
/// [`RuleList::push`], which asks [`Profile::admit`].
#[derive(Clone, Debug)]
pub struct RuleList {
    profile: Profile,
    rules: Vec<RuleSpec>,
}

impl RuleList {
    pub fn new(profile: Profile) -> RuleList {
        RuleList {
            profile,
            rules: Vec::new(),
        }
    }

    pub fn profile(&self) -> Profile {
        self.profile
    }

    /// Append `rule`, or refuse it.
    pub fn push(&mut self, rule: RuleSpec) -> Result<(), Refusal> {
        self.profile.admit(&rule)?;
        self.rules.push(rule);
        Ok(())
    }

    pub fn rules(&self) -> &[RuleSpec] {
        &self.rules
    }

    pub fn get(&self, name: &str) -> Option<&RuleSpec> {
        self.rules.iter().find(|r| r.name == name)
    }

    /// The one constructor of a decision: `rule` must be on this list.
    pub fn decide(&self, query: String, reference: String, rule: &str) -> Decision {
        let spec = self
            .get(rule)
            .unwrap_or_else(|| panic!("rule {rule} is not on the {:?} list", self.profile));
        Decision {
            query,
            reference,
            rule: spec.name,
            reads: spec.reads.iter().map(|e| e.spelling()).collect(),
        }
    }

    /// The list as a report (name, stage, reads), in order.
    pub fn describe(&self) -> Vec<RuleRow> {
        self.rules
            .iter()
            .map(|r| RuleRow {
                name: r.name,
                stage: r.stage,
                reads: r.reads.iter().map(|e| e.spelling()).collect(),
            })
            .collect()
    }

    fn of(profile: Profile, specs: &[RuleSpec]) -> RuleList {
        let mut list = RuleList::new(profile);
        for &s in specs {
            list.push(s)
                .unwrap_or_else(|r| panic!("the built-in list is refused: {r}"));
        }
        list
    }
}

/// One rule in a report.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RuleRow {
    pub name: &'static str,
    pub stage: Stage,
    pub reads: Vec<String>,
}

/// One decision's trail: the query piece, the reference piece it was
/// decided against, the deciding rule and what that rule reads.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Decision {
    pub query: String,
    pub reference: String,
    pub rule: &'static str,
    pub reads: Vec<String>,
}

// ---------------------------------------------------------------------------
// The cross-version profile: today's cascade, in today's order (§2.3)
// ---------------------------------------------------------------------------

use Evidence::{
    Claims, Features, Fingerprint, GraphAll, MemberKey, Position, Resolver, StatementContext,
};

const MATCH_KEY: Evidence = Fingerprint("match-key");

/// The cross-version resolve rungs, in the order `matching::cascade`
/// executes them on a bucket of more than one candidate (TS
/// `resolveMatch` + `resolveDeepStages`). THE ORDER IS LOAD-BEARING: each
/// ambiguous park uses the pool the failing rung narrowed to, and the
/// match list's insertion order (the rename order) follows the walk.
pub mod cross {
    use super::*;

    pub const STRUCTURAL_HASH_UNIQUE: RuleSpec =
        RuleSpec::new("structuralHashUnique", Stage::Candidates, &[MATCH_KEY]);
    /// The singleton guard (version-stable signals only).
    pub const SINGLETON_VERDICT: RuleSpec =
        RuleSpec::new("singletonVerdict", Stage::Guard, &[MemberKey, Features]);
    pub const IDENTITY: RuleSpec = RuleSpec::new("identity", Stage::Resolve, &[Resolver]);
    pub const MEMBER_KEY: RuleSpec = RuleSpec::new("memberKey", Stage::Resolve, &[MemberKey]);
    pub const ENCLOSING_STATEMENT: RuleSpec = RuleSpec::new(
        "enclosingStatement",
        Stage::Resolve,
        &[StatementContext, Position, Claims],
    );
    pub const DEPTH_GATE: RuleSpec = RuleSpec::new("depthGate", Stage::Resolve, &[]);
    pub const CALLEE_SHAPES: RuleSpec =
        RuleSpec::new("calleeShapes", Stage::Resolve, &[GraphAll, Features]);
    pub const CALLER_SHAPES: RuleSpec =
        RuleSpec::new("callerShapes", Stage::Resolve, &[GraphAll, Features]);
    /// Exact callee hashes, then two-hop shapes: one rung (a contradiction
    /// at either parks the pool the rung started from).
    pub const CALLEE_HASHES: RuleSpec =
        RuleSpec::new("calleeHashes", Stage::Resolve, &[GraphAll, MATCH_KEY]);
    pub const TWO_HOP_SHAPES: RuleSpec =
        RuleSpec::new("twoHopShapes", Stage::Resolve, &[GraphAll, Features]);
    pub const SHINGLE_SIMILARITY: RuleSpec =
        RuleSpec::new("shingleSimilarity", Stage::Resolve, &[GraphAll, Features]);
    pub const DEMOTE_NON_INJECTIVE: RuleSpec =
        RuleSpec::new("demoteNonInjective", Stage::Guard, &[Claims]);
    pub const REVOKE_CROSSED_CONTAINERS: RuleSpec =
        RuleSpec::new("revokeCrossedContainers", Stage::Guard, &[Claims, GraphAll]);
    pub const MATCHED_CALLEE: RuleSpec =
        RuleSpec::new("matchedCallee", Stage::Propagate, &[Claims, GraphAll]);
    pub const MATCHED_CALLER: RuleSpec =
        RuleSpec::new("matchedCaller", Stage::Propagate, &[Claims, GraphAll]);
    pub const SCOPE_PARENT: RuleSpec =
        RuleSpec::new("scopeParent", Stage::Propagate, &[Claims, GraphAll]);
    pub const EXTERNAL_REFS: RuleSpec =
        RuleSpec::new("externalRefs", Stage::Propagate, &[Claims, GraphAll]);
    pub const SCOPE_ORDINAL: RuleSpec =
        RuleSpec::new("scopeOrdinal", Stage::Propagate, &[Claims, Position]);
    /// The un-attributed propagation tier as the pair rows spell it.
    pub const PROPAGATION: RuleSpec = RuleSpec::new(
        "propagation",
        Stage::Propagate,
        &[Claims, GraphAll, Position],
    );
    pub const BINDING_ALTERNATION: RuleSpec =
        RuleSpec::new("bindingAlternation", Stage::Propagate, &[Claims, GraphAll]);
    /// The tail tiers, spelled as their pair rows spell them.
    pub const ORDINAL_TAIL: RuleSpec = RuleSpec::new("ordinal", Stage::Resolve, &[Position]);
    pub const INTERCHANGEABLE_POOLS: RuleSpec =
        RuleSpec::new("interchangeable", Stage::Resolve, &[Claims, GraphAll]);
    pub const CLOSE_TIER: RuleSpec = RuleSpec::new(
        "closeTier",
        Stage::Resolve,
        &[Features, Fingerprint("align-tokens")],
    );
    pub const SAME_PROGRAM: RuleSpec = RuleSpec::new("sameProgram", Stage::Check, &[Claims]);
}

/// The resolve rungs' specs, in execution order (`matching::cascade`'s
/// `RESOLVE_RUNGS` pairs each with its code, and a test holds the two in
/// step).
pub const RESOLVE: [RuleSpec; 8] = [
    cross::IDENTITY,
    cross::MEMBER_KEY,
    cross::ENCLOSING_STATEMENT,
    cross::DEPTH_GATE,
    cross::CALLEE_SHAPES,
    cross::CALLER_SHAPES,
    cross::CALLEE_HASHES,
    cross::SHINGLE_SIMILARITY,
];

/// The cross-version profile, in the order the stage runs (§2.3): the
/// bucket, the singleton guard, the resolve rungs, the two guards, the
/// propagation rungs, the binding alternation, the ordinal and
/// interchangeable tails, the close tier, the same-program check.
pub fn cross_version() -> RuleList {
    RuleList::of(
        Profile::CrossVersion,
        &[
            cross::STRUCTURAL_HASH_UNIQUE,
            cross::SINGLETON_VERDICT,
            cross::IDENTITY,
            cross::MEMBER_KEY,
            cross::ENCLOSING_STATEMENT,
            cross::DEPTH_GATE,
            cross::CALLEE_SHAPES,
            cross::CALLER_SHAPES,
            cross::CALLEE_HASHES,
            cross::TWO_HOP_SHAPES,
            cross::SHINGLE_SIMILARITY,
            cross::DEMOTE_NON_INJECTIVE,
            cross::REVOKE_CROSSED_CONTAINERS,
            cross::MATCHED_CALLEE,
            cross::MATCHED_CALLER,
            cross::SCOPE_PARENT,
            cross::EXTERNAL_REFS,
            cross::SCOPE_ORDINAL,
            cross::PROPAGATION,
            cross::BINDING_ALTERNATION,
            cross::ORDINAL_TAIL,
            cross::INTERCHANGEABLE_POOLS,
            cross::CLOSE_TIER,
            cross::SAME_PROGRAM,
        ],
    )
}

#[cfg(test)]
mod rules_test;
