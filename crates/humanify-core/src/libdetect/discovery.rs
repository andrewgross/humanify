//! Library discovery, report-only (exp095; docs/design/library-discovery.md
//! §5–§7): which app statements are library code the bundler hoisted out of
//! every module boundary. It reads the statement reference graph
//! ([`crate::graph::references`]) and claims; it names, moves and renames
//! NOTHING — the CLI writes what it found to `--diagnostics`
//! (`libraryDiscovery`).
//!
//! - **Seeds** (round 0): declarations the package readers found in a
//!   unit's own text, kept `/*!` banners, and index claims when an index is
//!   supplied.
//! - **The leaf-inward fixpoint**: each round claims, for every
//!   unidentified unit, by three rules — DOWN (every caller is claimed as
//!   one package P: P's private code), MODULE (a unit of a module whose
//!   claims are all P, whose closure stays inside the module and which no
//!   unidentified code outside the module calls), GLUE (a lazy wrapper
//!   whose body is all P). A unit claimed in round k is external in round
//!   k+1. The runtime and bundled CommonJS modules are external from the
//!   start.
//! - **The isolation veto**, repeated until stable: an INDEX seed with no
//!   graph neighbour (runtime aside) claimed as its own package family —
//!   after the fixpoint's DOWN and GLUE rules, not MODULE's fills
//!   ([`VetoSupport`]) — is dropped: a fork keeps a library's leaf tables
//!   and rewrites everything around them. Declarations and banners are
//!   exempt: they are their own evidence.
//! - **DOWN-CLASS**: everything reachable from a claim of at least
//!   `closure_floor` bytes over unidentified code is library CLASS, package
//!   unknown. The floor keeps a content-free claim (`var N = {}`) from
//!   owning the app module that fills it.
//! - **The candidate queue**: unidentified units that reference claimed
//!   code and whose closed region is at most `queue_cap` bytes — the next
//!   layer out, for a targeted index lookup. Closure is not evidence: the
//!   queue names nothing.
//!
//! Module groups: the units between two module boundaries (a CommonJS
//! factory or a lazy wrapper) belong to the module that ends there — a Bun
//! lazy module's hoisted functions and `var a, b;` precede its wrapper —
//! plus the wrapper's own body. A statement holding two or more modules
//! (webpack's module table) is their container and in no group.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde::Serialize;

use crate::cut::units::UnitKind;
use crate::graph::edges::Scope;
use crate::graph::references::ReferenceGraph;
use crate::libindex::families::PackageFamilies;
use crate::matching::rules::{
    self, Evidence as Reads, Profile, Refusal, RuleList, RuleSpec, Stage, lib,
};

/// What a seed claim stands on.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum Evidence {
    /// A package reader read it in the unit's own text.
    Declared { reader: String },
    /// A kept `/*!` banner just before (or inside) the unit.
    Banner,
    /// An index claim (structural/literal agreement) — not exempt from the
    /// isolation veto.
    Index,
    /// An index claim from a targeted lookup of a queued region: a lower
    /// floor, accepted only for a package family the region already calls.
    /// Vetoable like any index claim.
    QueueIndex,
}

impl Evidence {
    /// The kind's recorded spelling.
    pub fn kind(&self) -> &'static str {
        match self {
            Evidence::Declared { .. } => "declared",
            Evidence::Banner => "banner",
            Evidence::Index => "index",
            Evidence::QueueIndex => "queue-index",
        }
    }

    /// An index claim of either kind (vetoable; never a declaration).
    pub fn is_index(&self) -> bool {
        matches!(self, Evidence::Index | Evidence::QueueIndex)
    }
}

/// "Unit `unit` is package `package`", before any graph rule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Seed {
    pub unit: u32,
    pub package: String,
    pub evidence: Evidence,
}

/// The rule that made a claim.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Rule {
    Seed,
    Down,
    Module,
    Glue,
}

/// One claim after the fixpoint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Claim {
    pub package: String,
    pub rule: Rule,
    pub round: u32,
    pub evidence: Option<Evidence>,
    /// The profile rule that made a graph claim (`matching::rules`); empty
    /// for a seed (its evidence says what made it).
    pub by: &'static str,
}

/// Which claims may support an index seed against the isolation veto.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VetoSupport {
    /// Claims after a fixpoint WITHOUT the MODULE rule (the default). A
    /// module whose only claims are a fork's kept tables would otherwise
    /// fill the app code around them and count the fill as the tables'
    /// support: on Claude Code 2.1.86 that kept 4.5 KB of the app's own
    /// keypress parser claimed as `ink`. Costs ~2.5 KB of real library
    /// claims per release (lab, 2.1.86–2.1.216).
    WithoutModule,
    /// Every claim after the full fixpoint: the lab prototype's rule, kept
    /// for the lab's reproduction of its numbers.
    AllRules,
}

/// Which names a kept `/*!` banner may seed (exp106).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BannerNames {
    /// Any name the banner reader reads (the default).
    Any,
    /// With an index at hand, only a package the index knows, in npm's
    /// spelling ([`crate::libindex::lookup::LibraryIndex::resolve_known`]):
    /// a banner names a project (`noble-curves`, `Runge-Kutta`, `@license`),
    /// not always a package.
    Known,
}

/// The tunables (design §5): the content floor a DOWN-CLASS start needs,
/// the largest region queued for an index lookup, and what may support a
/// seed against the isolation veto.
#[derive(Clone, Copy, Debug)]
pub struct DiscoveryOptions {
    pub closure_floor: u32,
    pub queue_cap: u32,
    pub max_rounds: u32,
    pub veto_support: VetoSupport,
    /// The evidence-conflict guard (exp102; default off): an index seed or
    /// a propagated unit another family's index prints cover more of than
    /// the claimed family's is refused ([`Containment::outweighed`]).
    pub evidence_conflict: bool,
    /// The literal-conflict guard (exp102; default off): a propagated
    /// unit carrying a long literal the index's literal table ties only to
    /// other families is refused ([`Containment::literal_contradicts`]).
    pub literal_conflict: bool,
    /// The contested-cover floor (exp102; 0 = off): a propagated unit
    /// another family's prints also speak for, of which the claimed
    /// family's prints cover less than this share, is refused
    /// ([`Containment::contested_thin`]).
    pub contested_cover: f64,
    /// Which banner names seed (exp106; default [`BannerNames::Any`]).
    pub banner_names: BannerNames,
    /// The helper floor (exp106; 0 = off): an index print held by at least
    /// this many package families is emitted helper code (a tsc / Babel /
    /// bundler helper every package that needs it inlines), no package's
    /// identity. A unit such prints cover at least half of is no seed and
    /// is refused to propagation ([`Containment::helper_dominated`]).
    pub helper_families: u32,
    /// The payload floor (exp106; 0 = off): a propagated unit of at least
    /// this many OWN bytes (exp113: a module wrapper is not its body), nine
    /// tenths of them inside string literals (a base64 wasm blob, an
    /// embedded data table), is refused unless the claimed family's own
    /// index evidence holds it (a print of it, or its long literal in the
    /// family's files) or the payload names it (exp113: a base64 script
    /// opening with the package's banner): the call that reaches a payload
    /// says nothing about whose payload it is
    /// ([`Containment::unbacked_payload`]).
    pub payload_min_bytes: u32,
    /// Read another family's evidence in the conflict guards
    /// (`evidence_conflict`, `contested_cover`) under the lookup's
    /// specificity rule (exp106; default off): a print another family holds
    /// only in a module far bigger than the query's (a sponge package
    /// bundling the claimed package's code) is no rival.
    pub specific_rivals: bool,
    /// Join the package families by the metadata-derived links
    /// (`libindex/families-derived.json`, restricted to the index's
    /// packages) wherever discovery asks "one family?" (exp106).
    pub derived_families: bool,
    /// The context-presence factor (exp114; 0 = off): a package every one
    /// of whose index seeds sits in a module group the package's index
    /// holds that unit's prints only in modules at least `factor` x the
    /// group's bytes + [`Self::context_presence_slack`] is no seed. A
    /// package's dist that INLINES a dependency holds the dependency's code
    /// in a bigger module than the dependency's own; when the bundle shows
    /// the code in the smaller module, and nowhere does the bundle show the
    /// package at its own size, the code came from the dependency, not the
    /// package ([`presence`]). Units in no module group are not judged.
    pub context_presence: f64,
    /// The context-presence slack in bytes (exp114).
    pub context_presence_slack: u64,
    /// Shadow ties (exp114; default off): a package whose every index seed
    /// is a TIE the structural rule would accept for another family too,
    /// while some such rival has a seed of its own (one only its family is
    /// accepted for) in the bundle, is the rival's shadow — a package
    /// carrying the rival's code: its tied seeds become the rival's (the one
    /// with the most own-seed bytes), the rest are dropped ([`presence`]).
    /// A rival counts only where it holds the tied code in a module no
    /// bigger than the package's own (the origin is the smaller context: a
    /// sponge with its own seeds must not absorb a real origin's ties).
    pub shadow_ties: bool,
    /// Fence what the presence rules drop (exp114; default off): a dropped
    /// seed's unit is claimed by no one — neither the package nor a rival
    /// propagating into it — instead of left open (the code is ambiguous
    /// between a shipper and its origin; name neither).
    pub presence_fence: bool,
    /// The declared-seed cap (exp112; 0 = off): a declared seed (a package
    /// reader's text: an inlined `package.json`, a scoped error prefix)
    /// whose statement is bigger than this many bytes is no seed. A
    /// manifest literal inside a huge statement (a nested bundle's one
    /// factory holding many packages) says that package is somewhere in
    /// it, not that the statement is that package's.
    pub declared_max_bytes: u32,
    /// The copy-presence share (exp121; 0 = off): an index seed of P is a
    /// COPY seed when another family Q holds at least this share of the
    /// nodes P's own prints cover there, each in an index module at most
    /// 1/[`Self::copy_presence_factor`] of P's module for the same print —
    /// Q is the smaller context, the origin P's dist inlined (a sponge: a
    /// package whose published build bundles its dependencies). A package
    /// whose EVERY index seed is a copy seed, and whose family has no
    /// declared or banner seed, is no seed: the bundle never shows P's own
    /// code ([`presence`]; fenced with [`Self::presence_fence`]). Judged per
    /// package: a scope sibling's own seeds do not vouch for P's copies.
    pub copy_presence: f64,
    /// The copy-presence factor (exp121): how much smaller the origin's
    /// module must be than the sponge's.
    pub copy_presence_factor: u64,
    /// Prefer the origin (exp121): what [`Self::copy_presence`] drops
    /// becomes the seed of the owner co-holding the most of it in the
    /// smaller context (the code's origin), instead of no one's.
    pub copy_presence_origin: bool,
    /// The copy-presence own share (exp121; 0 = any): a package shows in
    /// the bundle only when its NON-copy index seeds hold at least this
    /// share of its index seed bytes there; otherwise its copy seeds go
    /// (its own seeds stay). A sponge's one stray seed on its own code
    /// does not vouch for megabytes of inlined dependencies.
    pub copy_presence_own: f64,
    /// Cut bundled CommonJS modules into their body statements (exp128;
    /// default off): each statement of a factory body is a unit with its
    /// own prints, judged and propagated as a lazy module's body is, the
    /// factory a wrapper (the index side has always cut them so: its rows
    /// hold factory bodies). Off, a factory is one external unit the index
    /// cannot speak for.
    pub factory_bodies: bool,
    /// The factory tie fence (exp132; default off; reads cut factories,
    /// [`Self::factory_bodies`]): a bundled CommonJS module is ONE file of
    /// one package. When every index seed on its body statements is a TIE
    /// (the structural rule accepts another family for it too: a fork, a
    /// sponge carrying the module verbatim, generated code many packages
    /// ship) and none of them is evidence only one family gives, the
    /// module's package is unknowable from its code: its seeds are dropped
    /// and its body statements are claimed by no one ([`factory_ties`]).
    /// One untied seed, or a declared or banner seed, keeps the module as it
    /// is.
    pub factory_tie_fence: bool,
    /// The factory whole-print floor (exp132; default off; reads cut
    /// factories): a package names a bundled CommonJS module's statements
    /// only when at least one of its index seeds in that module is a WHOLE
    /// statement its family's index holds (or it has a declared or banner
    /// seed there). Seeds from function prints alone (a shared helper, an
    /// inlined copy's inner functions, version drift) are no evidence of the
    /// module's file: they are dropped, and a module left with no seed is
    /// claimed by no one ([`factory_whole`]).
    pub factory_whole_seed: bool,
    /// The factory banner check (exp132; default off; reads cut factories):
    /// a `/*!` banner seed on a body statement of a bundled CommonJS module
    /// is refused when another family's index prints cover more of that
    /// statement than the banner family's ([`Containment::outweighed`]: a
    /// banner kept from another file, or one ahead of the next module's
    /// wrapper): the statement's own code outweighs a comment next to it
    /// ([`factory_banners`]).
    pub factory_banner_index: bool,
}

impl Default for DiscoveryOptions {
    fn default() -> Self {
        DiscoveryOptions {
            closure_floor: 30,
            queue_cap: 256_000,
            max_rounds: 50,
            veto_support: VetoSupport::WithoutModule,
            evidence_conflict: false,
            literal_conflict: false,
            contested_cover: 0.0,
            banner_names: BannerNames::Any,
            helper_families: 0,
            payload_min_bytes: 0,
            specific_rivals: false,
            derived_families: false,
            context_presence: 0.0,
            context_presence_slack: 0,
            shadow_ties: false,
            presence_fence: false,
            declared_max_bytes: 0,
            copy_presence: 0.0,
            copy_presence_factor: 0,
            copy_presence_origin: false,
            copy_presence_own: 0.0,
            factory_bodies: false,
            factory_tie_fence: false,
            factory_whole_seed: false,
            factory_banner_index: false,
        }
    }
}

impl DiscoveryOptions {
    /// The SHIPPED options: what `[library.discovery]` defaults to (exp109,
    /// from experiments/106-matcher-hygiene's measured recommendation) —
    /// exp102's three guards, read under `specific_rivals`, banner names the
    /// index knows, metadata-derived families and the payload guard; since
    /// exp117 (experiments/117-default-flip) also exp114's context presence
    /// with its fence and exp112's declared cap, measured together on every
    /// corpus on k2-all; and since 2026-10-15 (Andrew's call) `shadow_ties`,
    /// which removes fork claims on every webpack/rollup set for 1,035 wrong
    /// bytes on test2-esbuild; and since 2026-10-16 exp121's copy presence at
    /// its measured "v2" setting (0.1 / factor 4 / origin / own 0.1), clean
    /// on all 19 sets on k2-all. `helper_families` stays off.
    /// `Default` stays the engine's baseline (every guard off), which the
    /// unit tests compare each guard against.
    pub fn recommended() -> Self {
        DiscoveryOptions {
            evidence_conflict: true,
            literal_conflict: true,
            contested_cover: 0.05,
            banner_names: BannerNames::Known,
            payload_min_bytes: 4096,
            specific_rivals: true,
            derived_families: true,
            context_presence: 1.5,
            context_presence_slack: 16384,
            presence_fence: true,
            declared_max_bytes: 65536,
            shadow_ties: true,
            copy_presence: 0.1,
            copy_presence_factor: 4,
            copy_presence_origin: true,
            copy_presence_own: 0.1,
            ..DiscoveryOptions::default()
        }
    }
}

/// One fixpoint round.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoundStats {
    pub round: u32,
    pub units: u32,
    /// Bytes of the statements claimed (lazy wrappers' overhead excluded).
    pub statement_bytes: u64,
    pub by_rule: BTreeMap<Rule, u32>,
}

/// A closed region that calls identified library code: queued for a
/// targeted index lookup, never named by the graph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueueEntry {
    pub unit: u32,
    /// The region's units (the head first).
    pub region: Vec<u32>,
    pub region_units: u32,
    pub region_bytes: u64,
    pub uses: Vec<String>,
}

/// What discovery found.
#[derive(Clone, Debug, Default)]
pub struct Discovery {
    pub claims: BTreeMap<u32, Claim>,
    pub rounds: Vec<RoundStats>,
    /// Index seeds the isolation veto dropped.
    pub vetoed: Vec<Seed>,
    /// DOWN-CLASS: unit → the package whose claim's closure reached it.
    pub library_class: BTreeMap<u32, String>,
    pub queue: Vec<QueueEntry>,
    /// The boundary check, after the claims: per package, the INTO edges
    /// landing on its claims and how many of its units they land on (a
    /// package's callers should land on its entry points). Read, never
    /// used to claim.
    pub boundary: BTreeMap<String, Boundary>,
}

impl Discovery {
    /// Every claim as a decision trail (`matching::rules::Decision`): the
    /// claimed unit, what it was decided against (the index's package, the
    /// unit's own text, or the claims it hangs off) and the deciding rule
    /// of `profile`, with what that rule reads.
    pub fn decisions(
        &self,
        graph: &ReferenceGraph,
        profile: &LibraryProfile,
    ) -> Vec<crate::matching::rules::Decision> {
        self.claims
            .iter()
            .map(|(&u, c)| {
                let span = graph.units[u as usize].span;
                let (rule, against) = match (c.rule, &c.evidence) {
                    (Rule::Seed, Some(Evidence::Declared { .. })) => (lib::DECLARED.name, "text"),
                    (Rule::Seed, Some(Evidence::Banner)) => (lib::BANNER.name, "text"),
                    (Rule::Seed, Some(Evidence::QueueIndex)) => (lib::TARGETED.name, "index"),
                    (Rule::Seed, _) => (lib::STRUCTURAL.name, "index"),
                    _ => (c.by, "claims"),
                };
                profile.rules().decide(
                    format!("unit@{}-{}", span.start, span.end),
                    format!("{against}:{}", c.package),
                    rule,
                )
            })
            .collect()
    }
}

/// One package's INTO edges (the boundary check).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Boundary {
    pub into_edges: u32,
    pub into_units: u32,
}

/// The boundary check (`matching::rules::lib::BOUNDARY`, the one library
/// rule that may read INTO edges: it runs after every claim and decides
/// nothing).
fn boundary(st: &State<'_>, groups: &Groups) -> BTreeMap<String, Boundary> {
    let view = RoundView {
        st,
        groups,
        spec: &lib::BOUNDARY,
    };
    let mut out: BTreeMap<String, Boundary> = BTreeMap::new();
    for u in 0..view.units() {
        let Some(p) = view.package_of(u) else {
            continue;
        };
        let into = view.into_callers(u, p).len() as u32;
        if into > 0 {
            let b = out.entry(st.packages[p as usize].clone()).or_default();
            b.into_edges += into;
            b.into_units += 1;
        }
    }
    out
}

/// Is `u` a statement (top-level or of an init body) — the population the
/// rules claim from (wrappers are GLUE's; factories are external).
fn is_statement(graph: &ReferenceGraph, u: u32) -> bool {
    matches!(
        graph.units[u as usize].kind,
        UnitKind::Statement | UnitKind::LazyBody { .. } | UnitKind::FactoryBody { .. }
    )
}

/// Module groups (module doc): per unit, its group; per group, its
/// statements.
struct Groups {
    of: Vec<Option<u32>>,
    statements: Vec<Vec<u32>>,
    /// Per lazy wrapper, its body statements.
    bodies: BTreeMap<u32, Vec<u32>>,
}

fn module_groups(graph: &ReferenceGraph) -> Groups {
    let n = graph.units.len();
    let mut groups = Groups {
        of: vec![None; n],
        statements: Vec::new(),
        bodies: BTreeMap::new(),
    };
    // module heads in source order: a statement holding two or more is
    // their CONTAINER (webpack's module table), not a hoisted declaration
    // of the next one, and joins no group
    let heads: Vec<u32> = graph
        .units
        .iter()
        .filter(|x| matches!(x.kind, UnitKind::Factory | UnitKind::LazyModule))
        .map(|x| x.span.start)
        .collect();
    let holds = |u: u32| {
        let s = graph.units[u as usize].span;
        heads.partition_point(|&h| h < s.end) - heads.partition_point(|&h| h < s.start)
    };
    let mut pending: Vec<u32> = Vec::new();
    for u in 0..n as u32 {
        match graph.units[u as usize].kind {
            UnitKind::Statement if holds(u) >= 2 => {}
            UnitKind::Statement => pending.push(u),
            UnitKind::Factory | UnitKind::LazyModule => {
                let g = groups.statements.len() as u32;
                for &p in &pending {
                    groups.of[p as usize] = Some(g);
                }
                groups.of[u as usize] = Some(g);
                groups.statements.push(std::mem::take(&mut pending));
            }
            UnitKind::LazyBody { module } | UnitKind::FactoryBody { module } => {
                let g = groups.of[module as usize];
                groups.of[u as usize] = g;
                if let Some(g) = g {
                    groups.statements[g as usize].push(u);
                }
                groups.bodies.entry(module).or_default().push(u);
            }
        }
    }
    groups
}

/// Per unit, the bytes of the module it sits in (its group's statements
/// and container), else its own: the query side of the index's
/// specificity rule.
fn module_bytes(graph: &ReferenceGraph) -> Vec<u64> {
    let groups = module_groups(graph);
    let mut size = vec![0u64; groups.statements.len()];
    for (u, g) in groups.of.iter().enumerate() {
        if let Some(g) = g {
            size[*g as usize] += u64::from(graph.units[u].bytes);
        }
    }
    groups
        .of
        .iter()
        .enumerate()
        .map(|(u, g)| g.map_or(u64::from(graph.units[u].bytes), |g| size[g as usize]))
        .collect()
}

/// The claim state of one fixpoint run; packages interned.
struct State<'g> {
    graph: &'g ReferenceGraph,
    packages: Vec<String>,
    claimed: Vec<Option<(u32, Rule, u32, &'static str)>>,
    /// Per claimed unit, the one seed its claim derives from, or [`MANY`]
    /// (more than one seed, or a positional fill): what the isolation veto
    /// asks so a seed cannot support itself through its own propagation.
    origin: Vec<u32>,
}

/// A claim deriving from more than one seed (or none).
const MANY: u32 = u32::MAX;

impl State<'_> {
    fn ext(&self, u: u32) -> bool {
        self.graph.is_base_external(u) || self.claimed[u as usize].is_some()
    }

    fn package_of(&self, u: u32) -> Option<u32> {
        self.claimed[u as usize].map(|c| c.0)
    }

    fn intern(&mut self, package: &str) -> u32 {
        match self.packages.iter().position(|p| p == package) {
            Some(i) => i as u32,
            None => {
                self.packages.push(package.to_string());
                self.packages.len() as u32 - 1
            }
        }
    }

    /// The one package every unit in `units` is claimed as, if any.
    fn one_package(&self, units: impl Iterator<Item = u32>) -> Option<u32> {
        let mut found = None;
        for u in units {
            let p = self.package_of(u)?;
            if found.is_some_and(|f| f != p) {
                return None;
            }
            found = Some(p);
        }
        found
    }

    /// The seed a new claim of `u` as `p` derives from: the one origin of
    /// the `p` claims it hangs off (its `p` callers, or a wrapper's body),
    /// else [`MANY`].
    fn derive_origin(&self, groups: &Groups, u: u32, p: u32) -> u32 {
        let body = groups.bodies.get(&u).map_or(&[][..], Vec::as_slice);
        let mut found = None;
        for &c in self.graph.inn[u as usize].iter().chain(body) {
            if self.package_of(c) != Some(p) {
                continue;
            }
            let o = self.origin[c as usize];
            match found {
                None => found = Some(o),
                Some(f) if f != o => return MANY,
                Some(_) => {}
            }
        }
        found.unwrap_or(MANY)
    }
}

/// What a library rule sees: the claims so far, the module groups, and
/// the graph's edges — ONLY in the scopes the rule declared
/// (`matching::rules`, docs/matching-architecture.md §4). Scope is read
/// relative to the region the rule decides for: when a rule asks whether
/// unit `u` is package P's, an edge from a P claim to `u` is INSIDE P's
/// region, an edge from anything else (app code, another package) to `u`
/// is INTO it, and `u`'s own edges out are OUT OF it. Reading a scope the
/// rule did not declare is a bug, and panics.
pub struct RoundView<'v, 'g> {
    st: &'v State<'g>,
    groups: &'v Groups,
    spec: &'v RuleSpec,
}

impl RoundView<'_, '_> {
    fn need(&self, e: Reads) {
        assert!(
            self.spec.may_read(e),
            "library rule {} reads {} without declaring it",
            self.spec.name,
            e.spelling()
        );
    }

    /// How many units the graph has.
    pub fn units(&self) -> u32 {
        self.st.graph.units.len() as u32
    }

    /// A statement (top-level or of an init body) — the population the
    /// rules claim from.
    pub fn is_statement(&self, u: u32) -> bool {
        is_statement(self.st.graph, u)
    }

    /// Already identified: claimed, or a runtime helper / bundled module.
    pub fn is_external(&self, u: u32) -> bool {
        self.need(Reads::Claims);
        self.st.ext(u)
    }

    pub fn package_of(&self, u: u32) -> Option<u32> {
        self.need(Reads::Claims);
        self.st.package_of(u)
    }

    /// The one package every unit in `units` is claimed as.
    pub fn one_package(&self, units: impl Iterator<Item = u32>) -> Option<u32> {
        self.need(Reads::Claims);
        self.st.one_package(units)
    }

    /// The packages P with a claim of P calling `u`: for each, an INSIDE
    /// edge of P's region were `u` P's. Unclaimed callers (the app, or code
    /// nobody has identified) are not read.
    pub fn inside_caller_packages(&self, u: u32) -> BTreeSet<u32> {
        self.need(Reads::Graph(Scope::Inside));
        self.st.graph.inn[u as usize]
            .iter()
            .filter_map(|&c| self.st.package_of(c))
            .collect()
    }

    /// The callers of `u` that are NOT claimed as `p`: the INTO edges of
    /// P's region were `u` P's — how the app (or another package) calls it.
    pub fn into_callers(&self, u: u32, p: u32) -> Vec<u32> {
        self.need(Reads::Graph(Scope::Into));
        self.st.graph.inn[u as usize]
            .iter()
            .copied()
            .filter(|&c| self.st.package_of(c) != Some(p))
            .collect()
    }

    /// `u`'s own edges out: OUT OF any region `u` would join (or INSIDE
    /// it, where the callee joins too).
    pub fn callees(&self, u: u32) -> &[u32] {
        self.need(Reads::Graph(Scope::OutOf));
        &self.st.graph.out[u as usize]
    }

    /// Is a neighbour of `u` (either direction, runtime aside) claimed as a
    /// package `same` accepts: an INSIDE edge of that family's region.
    pub fn has_inside_neighbour(&self, u: u32, mut same: impl FnMut(u32, u32) -> bool) -> bool {
        self.need(Reads::Graph(Scope::Inside));
        let g = self.st.graph;
        g.out[u as usize]
            .iter()
            .chain(&g.inn[u as usize])
            .filter(|&&w| !g.units[w as usize].runtime)
            .filter_map(|&w| Some((w, self.st.package_of(w)?)))
            .any(|(w, q)| same(w, q))
    }

    /// Was `w`'s claim made only through seed `seed`'s own propagation?
    pub fn derives_only_from(&self, w: u32, seed: u32) -> bool {
        self.need(Reads::Claims);
        self.st.origin[w as usize] == seed
    }

    /// The module groups (positional: the units between two module
    /// boundaries, module doc).
    pub fn group_count(&self) -> u32 {
        self.need(Reads::Position);
        self.groups.statements.len() as u32
    }

    pub fn group_of(&self, u: u32) -> Option<u32> {
        self.need(Reads::Position);
        self.groups.of[u as usize]
    }

    pub fn group_statements(&self, g: u32) -> &[u32] {
        self.need(Reads::Position);
        &self.groups.statements[g as usize]
    }

    /// Each lazy wrapper with its body statements (its LazyWrapper edges:
    /// INSIDE the module's region).
    pub fn wrapper_bodies(&self) -> impl Iterator<Item = (u32, &[u32])> {
        self.need(Reads::Graph(Scope::Inside));
        self.groups.bodies.iter().map(|(&w, b)| (w, b.as_slice()))
    }
}

/// A propagation rule's proposals for one round: (unit, package id).
pub type ProposeFn = fn(&RoundView<'_, '_>, &mut Vec<(u32, u32)>);

/// One propagation rule of the library profile: its spec (what it
/// reads), the claim kind it makes, and its code.
#[derive(Clone, Copy)]
pub struct LibraryRule {
    pub spec: RuleSpec,
    pub rule: Rule,
    pub propose: ProposeFn,
}

/// DOWN: a unit whose library callers are all one package P is P's
/// private code. Only INSIDE edges: the app's calls into the library are
/// not read (two apps calling one library differently must identify it
/// the same).
fn propose_down(v: &RoundView<'_, '_>, out: &mut Vec<(u32, u32)>) {
    for u in 0..v.units() {
        if !v.is_statement(u) || v.is_external(u) {
            continue;
        }
        let packages = v.inside_caller_packages(u);
        if packages.len() == 1 {
            out.extend(packages.into_iter().map(|p| (u, p)));
        }
    }
}

/// Per member of group `g` (in `members` order): does its closure over
/// unidentified units stay inside the group's statements? A member is
/// LEAKY when it calls an unidentified unit outside the group, or an
/// unidentified member that is leaky; one reverse pass over the group's
/// edges answers every member (exp128: a per-member search was quadratic
/// in a large CommonJS module cut into its body statements).
fn closed_members(v: &RoundView<'_, '_>, g: u32, members: &[u32]) -> Vec<bool> {
    let inside = |w: u32| v.group_of(w) == Some(g) && v.is_statement(w);
    let index: HashMap<u32, usize> = members.iter().enumerate().map(|(i, &u)| (u, i)).collect();
    let mut callers: Vec<Vec<usize>> = vec![Vec::new(); members.len()];
    let mut leaky = vec![false; members.len()];
    let mut stack = Vec::new();
    for (i, &x) in members.iter().enumerate() {
        for &w in v.callees(x) {
            if w == x || v.is_external(w) {
                continue;
            }
            match index.get(&w) {
                Some(&j) if inside(w) => callers[j].push(i),
                _ => {
                    if !leaky[i] {
                        leaky[i] = true;
                        stack.push(i);
                    }
                }
            }
        }
    }
    while let Some(j) = stack.pop() {
        for &i in &callers[j] {
            if !leaky[i] {
                leaky[i] = true;
                stack.push(i);
            }
        }
    }
    leaky.into_iter().map(|l| !l).collect()
}

/// MODULE: a unit of a module whose claims are all one package P, whose
/// closure stays inside the module. Who calls the unit from outside is
/// not read (INTO).
fn propose_module(v: &RoundView<'_, '_>, out: &mut Vec<(u32, u32)>) {
    for g in 0..v.group_count() {
        let members = v.group_statements(g);
        let Some(p) = v.one_package(
            members
                .iter()
                .copied()
                .filter(|&u| v.package_of(u).is_some()),
        ) else {
            continue;
        };
        let closed = closed_members(v, g, members);
        for (&u, closed) in members.iter().zip(closed) {
            if !v.is_external(u) && closed {
                out.push((u, p));
            }
        }
    }
}

/// GLUE: a lazy wrapper whose body is all P.
fn propose_glue(v: &RoundView<'_, '_>, out: &mut Vec<(u32, u32)>) {
    for (wrapper, body) in v.wrapper_bodies() {
        if !v.is_external(wrapper)
            && let Some(p) = v.one_package(body.iter().copied())
        {
            out.push((wrapper, p));
        }
    }
}

/// The library profile's propagation rules, in round order (first wins),
/// and the rule list they belong to (every one admitted by it).
#[derive(Clone)]
pub struct LibraryProfile {
    list: RuleList,
    propagation: Vec<LibraryRule>,
}

impl LibraryProfile {
    /// The library profile (`matching::rules::library`) with its
    /// fixpoint's rules DOWN, MODULE, GLUE.
    pub fn standard() -> LibraryProfile {
        Self::with_propagation(Self::standard_rules()).expect("the built-in rules are admitted")
    }

    /// The fixpoint's built-in rules, in round order.
    pub fn standard_rules() -> Vec<LibraryRule> {
        vec![
            LibraryRule {
                spec: lib::DOWN,
                rule: Rule::Down,
                propose: propose_down,
            },
            LibraryRule {
                spec: lib::MODULE,
                rule: Rule::Module,
                propose: propose_module,
            },
            LibraryRule {
                spec: lib::GLUE,
                rule: Rule::Glue,
                propose: propose_glue,
            },
        ]
    }

    /// The profile with `rules` as its fixpoint's rules, each admitted by
    /// the library profile ([`crate::matching::rules::Profile::admit`]):
    /// a rule that reads INTO edges is REFUSED.
    pub fn with_propagation(rules: Vec<LibraryRule>) -> Result<LibraryProfile, Refusal> {
        // The fixpoint's rules stand where the built-in list has its
        // propagation rules.
        let mut list = RuleList::new(Profile::Library);
        let mut placed = false;
        for spec in rules::library().rules() {
            if spec.stage != Stage::Propagate {
                list.push(*spec)?;
            } else if !placed {
                placed = true;
                for r in &rules {
                    list.push(r.spec)?;
                }
            }
        }
        Ok(LibraryProfile {
            list,
            propagation: rules,
        })
    }

    pub fn rules(&self) -> &RuleList {
        &self.list
    }
}

/// Containment (docs/matching-architecture.md §4) with the index's node
/// table. A bundled library is a SUBSET of its package's build
/// (tree-shaking only removes), so a piece a propagation rule would add to
/// package P's region must be in P's index entry wherever the index can
/// say: a unit whose print is at or over the index's row floor
/// ([`crate::libindex::units::UNIT_ROW_MIN_NODES`]) and that no print of
/// P's family holds is not P's — the INSIDE edge that would bring it in is
/// one P's entry cannot have. A smaller unit counts for nothing (the index
/// holds no row for it), and so does what P's entry has that the bundle
/// lacks (tree-shaken).
pub struct Containment<'c> {
    index: &'c crate::libindex::lookup::LibraryIndex,
    /// The bundle and each graph unit's span (the literal guard reads a
    /// unit's text).
    code: &'c str,
    spans: Vec<oxc_span::Span>,
    /// Each graph unit's OWN bytes: a module wrapper's span holds its body,
    /// but the body is a unit of its own.
    own_bytes: Vec<u32>,
    /// Per unit, the owners holding each of its long literals that at most
    /// [`LITERAL_IDENTITY_FAMILIES`] families hold (read once).
    long_literals: std::cell::RefCell<HashMap<u32, Vec<BTreeSet<u32>>>>,
    prints: &'c [Option<crate::libindex::units::StatementPrints>],
    module_bytes: &'c [u64],
    rules: crate::libindex::lookup::LookupRules,
    /// The family judgement (`same`) every guard asks.
    families: &'c PackageFamilies,
    /// Per index owner, its base family (read once): a key for counting
    /// and covering, joined by `families.same` where links apply.
    owner_family: Vec<String>,
    /// Rivals' prints count only under the specificity rule
    /// ([`DiscoveryOptions::specific_rivals`]).
    specific_rivals: bool,
}

/// The cover key of the claimed package's own family (see
/// [`Containment::covers`]).
const OWN: &str = "\u{0}own";

impl<'c> Containment<'c> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        index: &'c crate::libindex::lookup::LibraryIndex,
        code: &'c str,
        graph: &ReferenceGraph,
        prints: &'c [Option<crate::libindex::units::StatementPrints>],
        module_bytes: &'c [u64],
        rules: crate::libindex::lookup::LookupRules,
        families: &'c PackageFamilies,
    ) -> Containment<'c> {
        Containment {
            index,
            code,
            spans: graph.units.iter().map(|u| u.span).collect(),
            own_bytes: graph.units.iter().map(|u| u.bytes).collect(),
            long_literals: std::cell::RefCell::default(),
            prints,
            module_bytes,
            rules,
            families,
            owner_family: index
                .package_names()
                .iter()
                .map(|p| families.family(p))
                .collect(),
            specific_rivals: false,
        }
    }

    /// Read rivals under the specificity rule ([`DiscoveryOptions::specific_rivals`]).
    pub fn with_specific_rivals(mut self, on: bool) -> Self {
        self.specific_rivals = on;
        self
    }

    fn name(&self, owner: u32) -> &str {
        self.index.package_name(owner)
    }

    /// Is index owner `owner` of `package`'s family?
    fn owner_is(&self, owner: u32, package: &str) -> bool {
        self.families.same(self.name(owner), package)
    }

    /// How many families `owners` span: owners one family judgement joins
    /// count once.
    fn family_count(&self, owners: impl IntoIterator<Item = u32>) -> usize {
        let mut reps: Vec<u32> = Vec::new();
        let mut keys: BTreeSet<&str> = BTreeSet::new();
        for o in owners {
            if !keys.insert(&self.owner_family[o as usize]) {
                continue;
            }
            if !reps
                .iter()
                .any(|&r| self.families.same(self.name(r), self.name(o)))
            {
                reps.push(o);
            }
        }
        reps.len()
    }

    /// How many families the structural rule would accept for `u` on its
    /// own, tie or not: the index's ambiguity about the unit (a sponge
    /// package, a fork, helpers every build emits).
    fn families_accepting(&self, u: u32) -> usize {
        let Some(Some(prints)) = self.prints.get(u as usize) else {
            return 0;
        };
        let q = crate::libindex::lookup::UnitQuery {
            unit: u,
            prints,
            module_bytes: self.module_bytes[u as usize],
            text: "",
        };
        self.family_count(
            self.index
                .structural_owners(&q, &self.rules, &|a, b| self.families.same(a, b)),
        )
    }

    /// The owners the structural rule would accept for `u` on its own, tie
    /// or not (empty for a unit with no prints).
    fn accepting_owners(&self, u: u32) -> Vec<u32> {
        let Some(Some(prints)) = self.prints.get(u as usize) else {
            return Vec::new();
        };
        let q = crate::libindex::lookup::UnitQuery {
            unit: u,
            prints,
            module_bytes: self.module_bytes[u as usize],
            text: "",
        };
        self.index
            .structural_owners(&q, &self.rules, &|a, b| self.families.same(a, b))
    }

    /// The smallest index module in which `package`'s family holds any of
    /// unit `u`'s prints at or over the row floor: the package's tightest
    /// context for this code. None when it holds none.
    fn tightest_context(&self, u: u32, package: &str) -> Option<u64> {
        use crate::libindex::units::UNIT_ROW_MIN_NODES;
        use crate::matching::store::ReferenceStore;
        let Some(Some(prints)) = self.prints.get(u as usize) else {
            return None;
        };
        std::iter::once(&prints.whole)
            .chain(&prints.functions)
            .filter(|p| p.nodes >= UNIT_ROW_MIN_NODES)
            .filter_map(|p| {
                self.index
                    .candidates(&p.hash)
                    .iter()
                    .filter(|hit| self.owner_is(hit.owner, package))
                    .map(|hit| hit.module_bytes)
                    .min()
            })
            .min()
    }

    /// The copy share of unit `u` as `package`'s (exp121,
    /// [`DiscoveryOptions::copy_presence`]): over the prints `package`'s
    /// family covers `u` with (the whole print when the lookup reads it
    /// for the package, else its largest non-overlapping such function
    /// prints, at the row floor), the largest share of those nodes ONE other
    /// family holds in an index module at most 1/`factor` of the package's
    /// smallest module for the same print, with that family's owner holding
    /// the code in the smallest module (the origin). None when the package
    /// covers nothing.
    fn copy_share(&self, u: u32, package: &str, factor: u64) -> Option<(f64, Option<u32>)> {
        use crate::libindex::units::UNIT_ROW_MIN_NODES;
        use crate::matching::store::ReferenceStore;
        let Some(Some(prints)) = self.prints.get(u as usize) else {
            return None;
        };
        let query = self.module_bytes[u as usize];
        let specific = |m: u64| {
            self.rules
                .specificity
                .is_none_or(|(f, slack)| m <= f * query + slack)
        };
        // The package's smallest module holding `p`, when `p` is a print
        // the lookup reads (held within the specificity bound by at most
        // `max_packages` owners) and the package is one of its owners: a
        // print too common to seed anyone says nothing about origins either.
        let own = |p: &crate::libindex::units::UnitPrint| -> Option<u64> {
            let owners = self.index.look(&p.hash, query, &self.rules, &|a, b| {
                self.families.same(a, b)
            })?;
            if !owners.iter().any(|&o| self.owner_is(o, package)) {
                return None;
            }
            self.index
                .candidates(&p.hash)
                .iter()
                .filter(|h| self.owner_is(h.owner, package) && specific(h.module_bytes))
                .map(|h| h.module_bytes)
                .min()
        };
        let mut covering: Vec<(&crate::libindex::units::UnitPrint, u64)> = Vec::new();
        if prints.whole.nodes >= UNIT_ROW_MIN_NODES
            && let Some(m) = own(&prints.whole)
        {
            covering.push((&prints.whole, m));
        } else {
            let mut held: Vec<(&crate::libindex::units::UnitPrint, u64)> = prints
                .functions
                .iter()
                .filter(|p| p.nodes >= UNIT_ROW_MIN_NODES)
                .filter_map(|p| own(p).map(|m| (p, m)))
                .collect();
            held.sort_by_key(|(p, _)| std::cmp::Reverse(p.nodes));
            for (p, m) in held {
                let (s, e) = (p.span.start, p.span.end);
                if covering
                    .iter()
                    .any(|(q, _)| !(e <= q.span.start || s >= q.span.end))
                {
                    continue;
                }
                covering.push((p, m));
            }
        }
        let cover: u64 = covering.iter().map(|(p, _)| u64::from(p.nodes)).sum();
        if cover == 0 {
            return None;
        }
        // Per other family: co-held nodes, and its owner in the smallest
        // module seen (module bytes, owner).
        let mut co: BTreeMap<&str, (u64, (u64, u32))> = BTreeMap::new();
        for (p, mine) in &covering {
            let mut smallest: BTreeMap<&str, (u64, u32)> = BTreeMap::new();
            for h in self.index.candidates(&p.hash).iter() {
                if self.owner_is(h.owner, package) {
                    continue;
                }
                let m = smallest
                    .entry(self.owner_family[h.owner as usize].as_str())
                    .or_insert((u64::MAX, h.owner));
                *m = (*m).min((h.module_bytes, h.owner));
            }
            for (f, m) in smallest {
                if m.0.saturating_mul(factor) <= *mine {
                    let e = co.entry(f).or_insert((0, m));
                    e.0 += u64::from(p.nodes);
                    e.1 = e.1.min(m);
                }
            }
        }
        // The best family: most co-held nodes, then the smaller module.
        let best = co
            .values()
            .max_by_key(|(n, m)| (*n, std::cmp::Reverse(*m)))
            .copied();
        Some(match best {
            Some((n, (_, owner))) => (n as f64 / cover as f64, Some(owner)),
            None => (0.0, None),
        })
    }

    /// Does `package`'s family hold unit `u`'s WHOLE print (at or over the
    /// row floor, in a module the lookup's specificity rule reads; however
    /// many other packages hold it too)?
    fn whole_held(&self, u: u32, package: &str) -> bool {
        use crate::libindex::units::UNIT_ROW_MIN_NODES;
        let Some(Some(prints)) = self.prints.get(u as usize) else {
            return false;
        };
        use crate::matching::store::ReferenceStore;
        let query = self.module_bytes[u as usize];
        prints.whole.nodes >= UNIT_ROW_MIN_NODES
            && self.index.candidates(&prints.whole.hash).iter().any(|h| {
                self.owner_is(h.owner, package)
                    && self
                        .rules
                        .specificity
                        .is_none_or(|(f, slack)| h.module_bytes <= f * query + slack)
            })
    }

    /// Does the index contradict unit `u` being `package`'s? (type doc)
    fn contradicts(&self, u: u32, package: &str) -> bool {
        use crate::libindex::units::UNIT_ROW_MIN_NODES;
        use crate::matching::store::ReferenceStore;
        let Some(Some(prints)) = self.prints.get(u as usize) else {
            return false;
        };
        if prints.whole.nodes < UNIT_ROW_MIN_NODES {
            return false;
        }
        let held_by_family = std::iter::once(&prints.whole)
            .chain(&prints.functions)
            .filter(|p| p.nodes >= UNIT_ROW_MIN_NODES)
            .any(|p| {
                self.index
                    .candidates(&p.hash)
                    .iter()
                    .any(|hit| self.owner_is(hit.owner, package))
            });
        !held_by_family
    }

    /// Does the literal table contradict unit `u` being `package`'s? Its
    /// long literals (identity evidence: a generated table, a long
    /// message — see `LibraryIndex::long_literal_owners`) that the table
    /// ties to at most [`LITERAL_IDENTITY_FAMILIES`] families are read; when
    /// there are some and none is held by `package`'s family, the unit
    /// carries another package's identity. The unit print cannot say this
    /// for a table: one huge string is a handful of nodes, under the row
    /// floor.
    fn literal_contradicts(&self, u: u32, package: &str) -> bool {
        let mut cache = self.long_literals.borrow_mut();
        let held = cache.entry(u).or_insert_with(|| {
            let span = self.spans[u as usize];
            let text = &self.code[span.start as usize..span.end as usize];
            self.index
                .long_literal_owners(text)
                .unwrap_or_default()
                .into_iter()
                .filter(|owners| {
                    self.family_count(owners.iter().copied()) <= LITERAL_IDENTITY_FAMILIES
                })
                .collect()
        });
        !held.is_empty()
            && !held
                .iter()
                .any(|owners| owners.iter().any(|&o| self.owner_is(o, package)))
    }

    /// Is unit `u` a data payload (at least `min_bytes` of its own, nine
    /// tenths string literal bytes) that `package`'s family holds no
    /// evidence of: no print of it (any size), none of its long literals,
    /// and no opaque run naming `package` ([`payload_names`])?
    fn unbacked_payload(&self, u: u32, package: &str, min_bytes: u32) -> bool {
        use crate::matching::store::ReferenceStore;
        // judged by its own bytes: a wrapper's body is judged as itself
        if self.own_bytes[u as usize] < min_bytes {
            return false;
        }
        let span = self.spans[u as usize];
        let text = &self.code[span.start as usize..span.end as usize];
        if 10 * string_literal_bytes(text) < 9 * text.len() || payload_names(text, package) {
            return false;
        }
        let printed = match self.prints.get(u as usize) {
            Some(Some(p)) => std::iter::once(&p.whole).chain(&p.functions).any(|p| {
                self.index
                    .candidates(&p.hash)
                    .iter()
                    .any(|hit| self.owner_is(hit.owner, package))
            }),
            _ => false,
        };
        let literal = self
            .index
            .long_literal_owners(text)
            .unwrap_or_default()
            .iter()
            .any(|owners| owners.iter().any(|&o| self.owner_is(o, package)));
        !printed && !literal
    }

    /// Per family, the nodes of unit `u` its index prints cover
    /// ([`family_cover`] at the row floor), and `u`'s nodes; None for a
    /// unit with no prints. `package`'s own family is keyed [`OWN`]; every
    /// other owner by its base family.
    fn covers(&self, u: u32, package: &str) -> Option<(BTreeMap<&str, u32>, u32)> {
        use crate::libindex::units::UNIT_ROW_MIN_NODES;
        let Some(Some(prints)) = self.prints.get(u as usize) else {
            return None;
        };
        let cover = family_cover(
            &prints.whole,
            &prints.functions,
            UNIT_ROW_MIN_NODES,
            |hash| {
                let query = self.module_bytes[u as usize];
                crate::matching::store::ReferenceStore::candidates(self.index, hash)
                    .iter()
                    .filter_map(|hit| {
                        if self.owner_is(hit.owner, package) {
                            Some(OWN)
                        } else if self.specific_rivals
                            && self.rules.specificity.is_some_and(|(factor, slack)| {
                                hit.module_bytes > factor * query + slack
                            })
                        {
                            None
                        } else {
                            Some(self.owner_family[hit.owner as usize].as_str())
                        }
                    })
                    .collect()
            },
        );
        Some((cover, prints.whole.nodes))
    }

    /// Does another family's evidence outweigh `package`'s family's on unit
    /// `u`? Per family, the nodes of `u` its index prints cover — the whole
    /// print when the family holds it, else its largest non-overlapping
    /// function prints (the structural rule's scoring, every print at or
    /// over the row floor, no specificity or package-count filter): a
    /// propagated claim is refused when some other family covers strictly
    /// more. Containment asks "can P's build hold this?"; this asks "does
    /// the unit's own evidence point elsewhere?" — a module of package Q
    /// that one nested helper shared with P would otherwise let DOWN take
    /// as P's. A tie (a fork holding the same prints) is no conflict.
    fn outweighed(&self, u: u32, package: &str) -> bool {
        let Some((cover, _)) = self.covers(u, package) else {
            return false;
        };
        let own = cover.get(OWN).copied().unwrap_or(0);
        cover.iter().any(|(f, &n)| *f != OWN && n > own)
    }

    /// Is unit `u` contested and `package`'s family's claim on it thin?
    /// Another family's prints also cover part of it, and the claimed
    /// family's cover less than `floor` of its nodes: a big unit (a whole
    /// module of Q) taken through one small helper P shares with it.
    /// Uncontested, a thin cover is version drift and DOWN may keep it.
    fn contested_thin(&self, u: u32, package: &str, floor: f64) -> bool {
        let Some((cover, nodes)) = self.covers(u, package) else {
            return false;
        };
        let own = cover.get(OWN).copied().unwrap_or(0);
        let contested = cover.iter().any(|(f, &n)| *f != OWN && n > 0);
        contested && f64::from(own) < floor * f64::from(nodes)
    }

    /// Is unit `u` emitted helper code? Its prints at or over the row floor
    /// that at least `floor` families hold (the whole print, else the
    /// largest non-overlapping function prints) cover at least half of its
    /// nodes. Commonness is read from the index alone: no list of helper
    /// names.
    fn helper_dominated(&self, u: u32, floor: u32) -> bool {
        use crate::libindex::units::UNIT_ROW_MIN_NODES;
        use crate::matching::store::ReferenceStore;
        let Some(Some(prints)) = self.prints.get(u as usize) else {
            return false;
        };
        let shared = |hash: &str| {
            self.family_count(self.index.candidates(hash).iter().map(|h| h.owner)) >= floor as usize
        };
        let whole = &prints.whole;
        if whole.nodes >= UNIT_ROW_MIN_NODES && shared(&whole.hash) {
            return true;
        }
        let mut fns: Vec<&crate::libindex::units::UnitPrint> = prints
            .functions
            .iter()
            .filter(|p| p.nodes >= UNIT_ROW_MIN_NODES)
            .collect();
        fns.sort_by_key(|p| std::cmp::Reverse(p.nodes));
        let mut taken: Vec<(u32, u32)> = Vec::new();
        let mut covered = 0u32;
        for p in fns {
            let (s, e) = (p.span.start, p.span.end);
            if taken.iter().any(|&(a, b)| !(e <= a || s >= b)) || !shared(&p.hash) {
                continue;
            }
            taken.push((s, e));
            covered += p.nodes;
        }
        whole.nodes > 0 && 2 * covered >= whole.nodes
    }
}

/// A run of the base64 / base64url alphabet at least this long is an
/// opaque payload (a wasm module, an embedded script, a packed trie): what
/// [`payload_names`] decodes.
const OPAQUE_RUN_MIN: usize = 256;

/// The bytes of `text` inside string and template literals (quotes
/// included; a lexical scan, comments and regexes not told apart).
fn string_literal_bytes(text: &str) -> usize {
    let b = text.as_bytes();
    let (mut i, mut inside) = (0, 0);
    while i < b.len() {
        let q = b[i];
        if q == b'"' || q == b'\'' || q == b'`' {
            let start = i;
            i += 1;
            while i < b.len() && b[i] != q {
                i += if b[i] == b'\\' { 2 } else { 1 };
            }
            i = (i + 1).min(b.len());
            inside += i - start;
        } else {
            i += 1;
        }
    }
    inside
}

/// The opaque runs of `text` ([`OPAQUE_RUN_MIN`]).
fn opaque_runs(text: &str) -> impl Iterator<Item = &[u8]> {
    let opaque =
        |c: &u8| c.is_ascii_alphanumeric() || matches!(c, b'+' | b'/' | b'=' | b'_' | b'-');
    text.as_bytes()
        .split(move |c| !opaque(c))
        .filter(|run| run.len() >= OPAQUE_RUN_MIN)
}

/// How much of an opaque run's head is decoded to look for a banner.
const PAYLOAD_HEAD: usize = 1024;

/// Does an opaque run of `text`, read as base64, open with text naming
/// `package` (a worker script embedded with its banner, `/*! <package>
/// 1.2.3 */`)? A payload that says whose it is is its package's own
/// evidence. The name must stand alone: not inside a longer name.
fn payload_names(text: &str, package: &str) -> bool {
    let name = package.as_bytes();
    let part = |c: u8| c.is_ascii_alphanumeric() || matches!(c, b'@' | b'/' | b'_' | b'-' | b'.');
    opaque_runs(text).any(|run| {
        let head = base64_decode(&run[..run.len().min(PAYLOAD_HEAD)]);
        head.windows(name.len()).enumerate().any(|(i, w)| {
            w == name
                && (i == 0 || !part(head[i - 1]))
                && head
                    .get(i + name.len())
                    .is_none_or(|&c| !part(c) || c == b'@')
        })
    })
}

/// Standard or URL-safe base64 to bytes, up to the first byte outside the
/// alphabet (padding included); a trailing partial group is dropped.
fn base64_decode(text: &[u8]) -> Vec<u8> {
    let value = |c: u8| match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' | b'-' => Some(62),
        b'/' | b'_' => Some(63),
        _ => None,
    };
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let (mut acc, mut bits) = (0u32, 0u32);
    for v in text.iter().map_while(|&c| value(c)) {
        acc = (acc << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    out
}

/// A long literal held by more families than this is no package's
/// identity (a licence line, a shared message).
const LITERAL_IDENTITY_FAMILIES: usize = 3;

/// Per family, the nodes of a unit its held prints cover: the whole
/// print's nodes when the family holds it, else the sum over its largest
/// non-overlapping held function prints (prints under `floor` nodes are
/// not in the index and count for nothing). `holders` names the families
/// holding a hash.
fn family_cover<'f>(
    whole: &crate::libindex::units::UnitPrint,
    functions: &[crate::libindex::units::UnitPrint],
    floor: u32,
    holders: impl Fn(&str) -> BTreeSet<&'f str>,
) -> BTreeMap<&'f str, u32> {
    let mut cover: BTreeMap<&str, u32> = BTreeMap::new();
    let whole_held = if whole.nodes >= floor {
        holders(&whole.hash)
    } else {
        BTreeSet::new()
    };
    for f in &whole_held {
        cover.insert(f, whole.nodes);
    }
    let mut hits: Vec<(&crate::libindex::units::UnitPrint, BTreeSet<&str>)> = functions
        .iter()
        .filter(|p| p.nodes >= floor)
        .map(|p| (p, holders(&p.hash)))
        .filter(|(_, h)| !h.is_empty())
        .collect();
    hits.sort_by_key(|(p, _)| std::cmp::Reverse(p.nodes));
    let mut taken: BTreeMap<&str, Vec<(u32, u32)>> = BTreeMap::new();
    for (p, families) in hits {
        let (s, e) = (p.span.start, p.span.end);
        for f in families {
            if whole_held.contains(f) {
                continue;
            }
            let spans = taken.entry(f).or_default();
            if spans.iter().any(|&(a, b)| !(e <= a || s >= b)) {
                continue;
            }
            spans.push((s, e));
            *cover.entry(f).or_default() += p.nodes;
        }
    }
    cover
}

/// The guards on every propagated claim when an index is at hand:
/// containment, and the evidence-conflict guard when the run asks for it.
#[derive(Clone, Copy)]
struct Guard<'c, 'x> {
    containment: &'c Containment<'x>,
    evidence_conflict: bool,
    literal_conflict: bool,
    contested_cover: f64,
    helper_families: u32,
    payload_min_bytes: u32,
}

impl Guard<'_, '_> {
    /// Does a guard refuse unit `u` as `package`'s?
    fn refuses(&self, u: u32, package: &str) -> bool {
        let c = self.containment;
        c.contradicts(u, package)
            || (self.evidence_conflict && c.outweighed(u, package))
            || (self.literal_conflict && c.literal_contradicts(u, package))
            || (self.contested_cover > 0.0 && c.contested_thin(u, package, self.contested_cover))
            || (self.helper_families > 0 && c.helper_dominated(u, self.helper_families))
            || (self.payload_min_bytes > 0
                && c.unbacked_payload(u, package, self.payload_min_bytes))
    }
}

/// One fixpoint round: every rule's proposals in rule order (first wins
/// per unit), each checked by the guards when an index is at hand.
fn round(
    st: &State<'_>,
    groups: &Groups,
    rules: &[LibraryRule],
    guard: Option<Guard<'_, '_>>,
) -> Vec<(u32, u32, Rule, &'static str)> {
    let mut found = Vec::new();
    for r in rules {
        let view = RoundView {
            st,
            groups,
            spec: &r.spec,
        };
        let mut proposed = Vec::new();
        (r.propose)(&view, &mut proposed);
        for (u, p) in proposed {
            if guard.is_some_and(|g| g.refuses(u, &st.packages[p as usize])) {
                continue;
            }
            found.push((u, p, r.rule, r.spec.name));
        }
    }
    let mut seen = BTreeSet::new();
    found.retain(|&(u, _, _, _)| seen.insert(u));
    found
}

/// Run the fixpoint from `seeds` with `rules`; the rounds' tally.
fn fixpoint<'g>(
    graph: &'g ReferenceGraph,
    groups: &Groups,
    seeds: &[Seed],
    max_rounds: u32,
    rules: &[LibraryRule],
    guard: Option<Guard<'_, '_>>,
) -> (State<'g>, Vec<RoundStats>) {
    let mut st = State {
        graph,
        packages: Vec::new(),
        claimed: vec![None; graph.units.len()],
        origin: vec![MANY; graph.units.len()],
    };
    for seed in seeds {
        let p = st.intern(&seed.package);
        st.claimed[seed.unit as usize] = Some((p, Rule::Seed, 0, ""));
        st.origin[seed.unit as usize] = seed.unit;
    }
    let mut rounds = Vec::new();
    for k in 1..=max_rounds {
        let added = round(&st, groups, rules, guard);
        if added.is_empty() {
            break;
        }
        let mut stats = RoundStats {
            round: k,
            units: added.len() as u32,
            ..RoundStats::default()
        };
        // Origins from the round's starting state, like the claims.
        let origins: Vec<u32> = added
            .iter()
            .map(|&(u, p, _, _)| st.derive_origin(groups, u, p))
            .collect();
        for (&(u, p, rule, by), o) in added.iter().zip(origins) {
            st.claimed[u as usize] = Some((p, rule, k, by));
            st.origin[u as usize] = o;
            *stats.by_rule.entry(rule).or_default() += 1;
            if is_statement(graph, u) {
                stats.statement_bytes += u64::from(graph.units[u as usize].bytes);
            }
        }
        rounds.push(stats);
    }
    (st, rounds)
}

/// The isolation veto: the index seeds with no neighbour (runtime aside)
/// claimed as their own family — INSIDE edges of the family's region only.
/// A seed the index is AMBIGUOUS about (the structural rule would accept
/// more than one family for it: a sponge package's copy, a fork, shared
/// emitted helpers) cannot support itself: a neighbour claimed only
/// through this seed's own propagation is not independent evidence. (The
/// old DOWN read the app's callers, which kept such a seed's neighbours
/// unclaimed by accident; without INTO edges, its own declarations and
/// helpers would otherwise vouch for it.)
fn isolated(
    st: &State<'_>,
    groups: &Groups,
    seeds: &[Seed],
    families: &PackageFamilies,
    containment: Option<&Containment<'_>>,
) -> BTreeSet<u32> {
    let view = RoundView {
        st,
        groups,
        spec: &lib::ISOLATION_VETO,
    };
    let mut same_cache: HashMap<(u32, u32), bool> = HashMap::new();
    let mut same = |p: u32, q: u32| {
        *same_cache
            .entry((p, q))
            .or_insert_with(|| families.same(&st.packages[p as usize], &st.packages[q as usize]))
    };
    let mut out = BTreeSet::new();
    for seed in seeds.iter().filter(|s| s.evidence.is_index()) {
        let Some(p) = view.package_of(seed.unit) else {
            continue;
        };
        let ambiguous = containment.is_some_and(|c| c.families_accepting(seed.unit) > 1);
        let supported = view.has_inside_neighbour(seed.unit, |w, q| {
            !(ambiguous && view.derives_only_from(w, seed.unit)) && same(p, q)
        });
        if !supported {
            out.insert(seed.unit);
        }
    }
    out
}

/// The bundle-level presence rules on the index seeds (exp114;
/// [`DiscoveryOptions::context_presence`], [`DiscoveryOptions::shadow_ties`]):
/// a package's index seeds answer "is this code P's?" one unit at a time;
/// these ask whether the bundle shows P AT ALL. Declarations and banners
/// pass untouched, and a package with one keeps its index seeds.
fn presence(
    seeds: Vec<Seed>,
    graph: &ReferenceGraph,
    module_bytes: &[u64],
    containment: &Containment<'_>,
    families: &PackageFamilies,
    options: DiscoveryOptions,
) -> Vec<Seed> {
    if options.context_presence <= 0.0
        && !options.shadow_ties
        && options.copy_presence <= 0.0
        && !options.factory_tie_fence
        && !options.factory_whole_seed
        && !options.factory_banner_index
    {
        return seeds;
    }
    // A family key per package: one family judgement joins packages.
    let mut reps: Vec<String> = Vec::new();
    let mut key_of = |p: &str| -> usize {
        match reps.iter().position(|r| families.same(r, p)) {
            Some(i) => i,
            None => {
                reps.push(p.to_string());
                reps.len() - 1
            }
        }
    };
    let seeds = if options.factory_banner_index {
        factory_banners(seeds, graph, containment)
    } else {
        seeds
    };
    let mut keys: Vec<usize> = seeds.iter().map(|s| key_of(&s.package)).collect();
    let declared: BTreeSet<usize> = seeds
        .iter()
        .zip(&keys)
        .filter(|(s, _)| !s.evidence.is_index())
        .map(|(_, &k)| k)
        .collect();
    let mut keep: Vec<Option<Seed>> = seeds.into_iter().map(Some).collect();
    let mut dropped_units: Vec<u32> = Vec::new();
    if options.context_presence > 0.0 {
        let mut present: BTreeSet<usize> = declared.clone();
        let mut judged: BTreeSet<usize> = BTreeSet::new();
        for (s, &k) in keep.iter().flatten().zip(&keys) {
            let u = s.unit as usize;
            let group = module_bytes[u];
            let grouped = group > u64::from(graph.units[u].bytes);
            let oversized = grouped
                && containment
                    .tightest_context(s.unit, &s.package)
                    .is_some_and(|ctx| {
                        ctx as f64
                            >= options.context_presence * group as f64
                                + options.context_presence_slack as f64
                    });
            judged.insert(k);
            if !oversized {
                present.insert(k);
            }
        }
        for (slot, &k) in keep.iter_mut().zip(&keys) {
            if judged.contains(&k) && !present.contains(&k) {
                dropped_units.extend(slot.take().map(|s| s.unit));
            }
        }
    }
    if options.copy_presence > 0.0 {
        copy_presence(
            graph,
            &mut keep,
            &mut keys,
            &declared,
            containment,
            options,
            &mut key_of,
            &mut dropped_units,
        );
    }
    let before: Vec<u32> = keep.iter().flatten().map(|s| s.unit).collect();
    if options.shadow_ties {
        shadow_ties(&mut keep, &keys, &declared, graph, containment, &mut key_of);
    }
    let mut fenced: Vec<u32> = Vec::new();
    if options.factory_tie_fence {
        factory_ties(&mut keep, graph, containment, &mut fenced);
    }
    if options.factory_whole_seed {
        factory_whole(&mut keep, &keys, graph, containment, &mut fenced);
    }
    let mut out: Vec<Seed> = keep.into_iter().flatten().collect();
    out.extend(fenced.into_iter().map(|unit| Seed {
        unit,
        package: FENCE.to_string(),
        evidence: Evidence::Declared {
            reader: FENCE.to_string(),
        },
    }));
    if options.presence_fence {
        let kept: BTreeSet<u32> = out.iter().map(|s| s.unit).collect();
        let all: BTreeSet<u32> = before.into_iter().chain(dropped_units).collect();
        out.extend(
            all.into_iter()
                .filter(|u| !kept.contains(u))
                .map(|unit| Seed {
                    unit,
                    package: FENCE.to_string(),
                    evidence: Evidence::Declared {
                        reader: FENCE.to_string(),
                    },
                }),
        );
    }
    out
}

/// The package a fenced unit is claimed as while the fixpoint runs
/// ([`DiscoveryOptions::presence_fence`]); [`unfence`] removes every
/// claim of it after.
const FENCE: &str = "<fence>";

/// `found` without the fence's claims (see [`FENCE`]).
fn unfence(mut found: Discovery) -> Discovery {
    found.claims.retain(|_, c| c.package != FENCE);
    found.library_class.retain(|_, p| p != FENCE);
    found.boundary.remove(FENCE);
    for q in &mut found.queue {
        q.uses.retain(|p| p != FENCE);
    }
    found.queue.retain(|q| !q.uses.is_empty());
    found
}

/// [`DiscoveryOptions::copy_presence`] over the kept seeds (see [`presence`]):
/// a package whose every index seed is a copy seed loses them — to the
/// origin with [`DiscoveryOptions::copy_presence_origin`], else to no one
/// (`dropped`, which the fence holds).
#[allow(clippy::too_many_arguments)]
fn copy_presence(
    graph: &ReferenceGraph,
    keep: &mut [Option<Seed>],
    keys: &mut [usize],
    declared: &BTreeSet<usize>,
    containment: &Containment<'_>,
    options: DiscoveryOptions,
    key_of: &mut dyn FnMut(&str) -> usize,
    dropped_units: &mut Vec<u32>,
) {
    // Judged per PACKAGE, not per family: a scope sibling's own code
    // (`@scope/core`) says nothing about whether `@scope/server`'s seeds
    // are copies. A package is present when its index seeds on code no
    // smaller context holds carry at least `copy_presence_own` of its seed
    // bytes (any one, at 0), or its family has a declared or banner seed.
    let mut total: BTreeMap<String, u64> = BTreeMap::new();
    let mut own: BTreeMap<String, u64> = BTreeMap::new();
    let mut copies: BTreeMap<usize, Option<u32>> = BTreeMap::new();
    for (i, (s, k)) in keep.iter().zip(keys.iter()).enumerate() {
        let Some(s) = s.as_ref().filter(|s| s.evidence.is_index()) else {
            continue;
        };
        if declared.contains(k) {
            continue;
        }
        let bytes = u64::from(graph.units[s.unit as usize].bytes);
        *total.entry(s.package.clone()).or_default() += bytes;
        match containment.copy_share(s.unit, &s.package, options.copy_presence_factor) {
            Some((share, owner)) if share >= options.copy_presence => {
                copies.insert(i, owner);
            }
            _ => *own.entry(s.package.clone()).or_default() += bytes,
        }
    }
    let absent = |p: &str| {
        let mine = own.get(p).copied().unwrap_or(0);
        mine == 0 || (mine as f64) < options.copy_presence_own * total[p] as f64
    };
    for (i, (slot, key)) in keep.iter_mut().zip(keys.iter_mut()).enumerate() {
        let Some(&owner) = copies.get(&i) else {
            continue;
        };
        if !slot.as_ref().is_some_and(|s| absent(&s.package)) {
            continue;
        }
        match (options.copy_presence_origin, owner, slot.as_mut()) {
            (true, Some(o), Some(s)) => {
                s.package = containment.name(o).to_string();
                *key = key_of(&s.package);
            }
            _ => dropped_units.extend(slot.take().map(|s| s.unit)),
        }
    }
}

/// [`DiscoveryOptions::shadow_ties`] over the kept seeds (see [`presence`]).
fn shadow_ties(
    keep: &mut [Option<Seed>],
    keys: &[usize],
    declared: &BTreeSet<usize>,
    graph: &ReferenceGraph,
    containment: &Containment<'_>,
    key_of: &mut dyn FnMut(&str) -> usize,
) {
    // Per index seed, the families the structural rule accepts (its own
    // included) and the owner naming each rival family.
    let mut accepts: Vec<BTreeMap<usize, u32>> = vec![BTreeMap::new(); keep.len()];
    let mut own_bytes: BTreeMap<usize, u64> = BTreeMap::new();
    for (i, s) in keep.iter().enumerate() {
        let Some(s) = s.as_ref().filter(|s| s.evidence.is_index()) else {
            continue;
        };
        let own_context = containment.tightest_context(s.unit, &s.package);
        let mut tied_rival = false;
        for o in containment.accepting_owners(s.unit) {
            let k = key_of(containment.name(o));
            if k == keys[i] {
                continue;
            }
            tied_rival = true;
            // Only a rival holding this code in a module no bigger than the
            // package's own can be its ORIGIN; a bigger one is a carrier.
            let rival_context = containment.tightest_context(s.unit, containment.name(o));
            if rival_context.zip(own_context).is_some_and(|(r, p)| r <= p) {
                accepts[i].entry(k).or_insert(o);
            }
        }
        if !tied_rival {
            *own_bytes.entry(keys[i]).or_default() += u64::from(graph.units[s.unit as usize].bytes);
        }
    }
    let mut packages: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (i, s) in keep.iter().enumerate() {
        if s.as_ref().is_some_and(|s| s.evidence.is_index()) {
            packages.entry(keys[i]).or_default().push(i);
        }
    }
    for (k, seeds) in packages {
        if declared.contains(&k) || own_bytes.contains_key(&k) {
            continue;
        }
        let rival = seeds
            .iter()
            .flat_map(|&i| accepts[i].keys())
            .filter(|r| own_bytes.contains_key(r))
            .max_by_key(|&&r| (own_bytes[&r], std::cmp::Reverse(r)))
            .copied();
        let Some(rival) = rival else {
            continue;
        };
        for i in seeds {
            match (accepts[i].get(&rival), keep[i].as_mut()) {
                (Some(&owner), Some(s)) => s.package = containment.name(owner).to_string(),
                _ => keep[i] = None,
            }
        }
    }
}

/// [`DiscoveryOptions::factory_tie_fence`] over the kept seeds (see
/// [`presence`]): per cut factory, its seeds when every one is an index seed
/// the structural rule accepts for two or more families are dropped, and
/// every body statement of the factory goes to `fenced` (claimed by no one).
fn factory_ties(
    keep: &mut [Option<Seed>],
    graph: &ReferenceGraph,
    containment: &Containment<'_>,
    fenced: &mut Vec<u32>,
) {
    let module_of = |u: u32| match graph.units[u as usize].kind {
        UnitKind::FactoryBody { module } => Some(module),
        _ => None,
    };
    // per factory: (has an untied seed, has a tied seed)
    let mut verdict: BTreeMap<u32, (bool, bool)> = BTreeMap::new();
    for s in keep.iter().flatten() {
        let Some(m) = module_of(s.unit) else {
            continue;
        };
        let tied = s.evidence.is_index() && containment.families_accepting(s.unit) > 1;
        let v = verdict.entry(m).or_default();
        if tied {
            v.1 = true;
        } else {
            v.0 = true;
        }
    }
    let gone: BTreeSet<u32> = verdict
        .into_iter()
        .filter(|(_, (untied, tied))| *tied && !*untied)
        .map(|(m, _)| m)
        .collect();
    if gone.is_empty() {
        return;
    }
    for slot in keep.iter_mut() {
        if slot
            .as_ref()
            .is_some_and(|s| module_of(s.unit).is_some_and(|m| gone.contains(&m)))
        {
            *slot = None;
        }
    }
    fenced.extend(
        (0..graph.units.len() as u32).filter(|&u| module_of(u).is_some_and(|m| gone.contains(&m))),
    );
}

/// [`DiscoveryOptions::factory_banner_index`]: `seeds` without the banner
/// seeds on cut-factory body statements another family's prints cover more
/// of.
fn factory_banners(
    seeds: Vec<Seed>,
    graph: &ReferenceGraph,
    containment: &Containment<'_>,
) -> Vec<Seed> {
    seeds
        .into_iter()
        .filter(|s| {
            if s.evidence != Evidence::Banner
                || !matches!(
                    graph.units[s.unit as usize].kind,
                    UnitKind::FactoryBody { .. }
                )
            {
                return true;
            }
            !containment.outweighed(s.unit, &s.package)
        })
        .collect()
}

/// [`DiscoveryOptions::factory_whole_seed`] over the kept seeds (see
/// [`presence`]): per cut factory and package family, the family's seeds
/// there are kept only when one of them is a declared or banner seed or an
/// index seed whose whole print the family holds; every other body statement
/// of a factory that lost seeds goes to `fenced`.
fn factory_whole(
    keep: &mut [Option<Seed>],
    keys: &[usize],
    graph: &ReferenceGraph,
    containment: &Containment<'_>,
    fenced: &mut Vec<u32>,
) {
    let module_of = |u: u32| match graph.units[u as usize].kind {
        UnitKind::FactoryBody { module } => Some(module),
        _ => None,
    };
    let mut backed: BTreeSet<(u32, usize)> = BTreeSet::new();
    for (s, &k) in keep.iter().zip(keys) {
        let Some(s) = s else {
            continue;
        };
        let Some(m) = module_of(s.unit) else {
            continue;
        };
        if !s.evidence.is_index() || containment.whole_held(s.unit, &s.package) {
            backed.insert((m, k));
        }
    }
    let mut lost: BTreeSet<u32> = BTreeSet::new();
    let mut kept: BTreeSet<u32> = BTreeSet::new();
    for (slot, &k) in keep.iter_mut().zip(keys) {
        let Some((m, u)) = slot
            .as_ref()
            .and_then(|s| Some((module_of(s.unit)?, s.unit)))
        else {
            continue;
        };
        if backed.contains(&(m, k)) {
            kept.insert(u);
        } else {
            *slot = None;
            lost.insert(m);
        }
    }
    // the dropped package must not take the module back through propagation
    // (a caller's `require_x()` reaches every body statement): every body
    // statement of a module that lost seeds and kept none of its own is fenced
    fenced.extend(
        (0..graph.units.len() as u32)
            .filter(|&u| module_of(u).is_some_and(|m| lost.contains(&m)) && !kept.contains(&u)),
    );
}

/// One seed per unit: a declaration or banner beats an index claim; among
/// equals, the first. Seeds on external units are dropped.
fn settle_seeds(graph: &ReferenceGraph, seeds: &[Seed]) -> Vec<Seed> {
    let mut by_unit: BTreeMap<u32, &Seed> = BTreeMap::new();
    for seed in seeds {
        if graph.is_base_external(seed.unit) {
            continue;
        }
        match by_unit.get(&seed.unit) {
            Some(had) if !had.evidence.is_index() || seed.evidence.is_index() => {}
            _ => {
                by_unit.insert(seed.unit, seed);
            }
        }
    }
    by_unit.into_values().cloned().collect()
}

/// DOWN-CLASS (module doc): what a claim reaches through its OUT-OF
/// edges over unidentified code.
fn down_class(st: &State<'_>, groups: &Groups, floor: u32) -> BTreeMap<u32, String> {
    let v = RoundView {
        st,
        groups,
        spec: &lib::LIBRARY_CLASS,
    };
    let graph = st.graph;
    let mut class: BTreeMap<u32, String> = BTreeMap::new();
    for u in 0..v.units() {
        let Some(p) = v.package_of(u) else {
            continue;
        };
        if graph.units[u as usize].bytes < floor {
            continue;
        }
        let mut stack: Vec<u32> = v.callees(u).to_vec();
        while let Some(x) = stack.pop() {
            if class.contains_key(&x) || v.is_external(x) {
                continue;
            }
            class.insert(x, st.packages[p as usize].clone());
            stack.extend(v.callees(x).iter().copied());
        }
    }
    class
}

/// The candidate queue (module doc): unidentified regions and the
/// packages their OUT-OF edges reach (abstract: package names only).
fn candidate_queue(st: &State<'_>, groups: &Groups, cap: u32) -> Vec<QueueEntry> {
    let view = RoundView {
        st,
        groups,
        spec: &lib::QUEUE,
    };
    let mut queue = Vec::new();
    // Units whose closure is already known to pass the cap: a closure
    // reaching one passes it too (it holds that closure), so the search
    // stops there (exp128: the same answers, without re-walking a large
    // CommonJS module's body for every statement that reaches it).
    let mut memo = ClosureMemo {
        too_big: vec![false; view.units() as usize],
        shared: st
            .graph
            .units
            .iter()
            .any(|u| matches!(u.kind, UnitKind::FactoryBody { .. }))
            .then(HashMap::new),
    };
    for v in 0..view.units() {
        if !view.is_statement(v) || view.is_external(v) {
            continue;
        }
        let uses: BTreeSet<&str> = view
            .callees(v)
            .iter()
            .filter_map(|&w| view.package_of(w))
            .map(|p| st.packages[p as usize].as_str())
            .collect();
        if uses.is_empty() {
            continue;
        }
        if let Some((region, bytes)) = bounded_closure(&view, v, u64::from(cap), &mut memo) {
            queue.push(QueueEntry {
                unit: v,
                region_units: region.len() as u32,
                region,
                region_bytes: bytes,
                uses: uses.into_iter().map(String::from).collect(),
            });
        }
    }
    queue
}

/// What [`bounded_closure`] remembers between candidates.
struct ClosureMemo {
    /// Units whose closure passed the cap: a closure reaching one passes
    /// it too (it holds that closure).
    too_big: Vec<bool>,
    /// Only with cut CommonJS modules (exp128; None otherwise, so a run
    /// without them takes exactly the plain search): each candidate's
    /// region (search order, sorted, bytes). A candidate reaching a unit
    /// whose region holds the candidate has that very region (each reaches
    /// the other) — a module body whose statements all read one binding
    /// all its statements write is one such cycle, thousands strong.
    shared: Option<HashMap<u32, std::rc::Rc<SharedRegion>>>,
}

/// A remembered candidate region: search order, sorted, bytes.
struct SharedRegion {
    order: Vec<u32>,
    sorted: Vec<u32>,
    bytes: u64,
}

/// |D(v)| over unidentified units, None past `cap` bytes (the head
/// first, then search order; a region shared through
/// [`ClosureMemo::shared`] keeps the first candidate's order after the
/// head).
fn bounded_closure(
    view: &RoundView<'_, '_>,
    v: u32,
    cap: u64,
    memo: &mut ClosureMemo,
) -> Option<(Vec<u32>, u64)> {
    // The verdict is the closure's total, whatever the visiting order: a
    // unit's bytes count when it is first reached, so a unit calling
    // thousands (a cut CommonJS module's wrapper) stops the search as soon
    // as the cap is passed (exp128).
    let size = |u: u32| u64::from(view.st.graph.units[u as usize].bytes);
    let mut seen = BTreeSet::from([v]);
    let mut order = vec![v];
    let mut stack = vec![v];
    let mut bytes = size(v);
    let over = |memo: &mut ClosureMemo| {
        memo.too_big[v as usize] = true;
        None
    };
    if bytes > cap {
        return over(memo);
    }
    while let Some(x) = stack.pop() {
        for &w in view.callees(x) {
            if view.is_external(w) {
                continue;
            }
            if memo.too_big[w as usize] {
                return over(memo);
            }
            if let Some(region) = memo.shared.as_ref().and_then(|m| m.get(&w))
                && region.sorted.binary_search(&v).is_ok()
            {
                let region = region.clone();
                let mut out = vec![v];
                out.extend(region.order.iter().copied().filter(|&u| u != v));
                memo.shared.as_mut().map(|m| m.insert(v, region.clone()));
                return Some((out, region.bytes));
            }
            if seen.insert(w) {
                bytes += size(w);
                if bytes > cap {
                    return over(memo);
                }
                order.push(w);
                stack.push(w);
            }
        }
    }
    if let Some(m) = memo.shared.as_mut() {
        let mut sorted = order.clone();
        sorted.sort_unstable();
        m.insert(
            v,
            std::rc::Rc::new(SharedRegion {
                order: order.clone(),
                sorted,
                bytes,
            }),
        );
    }
    Some((order, bytes))
}

/// The seeds a run finds without an index: what the package readers
/// declare in each statement's own text (`identity::scan_app_declarations`;
/// a version constant names nothing; an AWS client pair names its client
/// package, a family-level claim), and kept `/*!` banners — the banner's
/// own statement, or the statement it precedes (a lazy module's: its first
/// body statement; a bundled module's: none — it is already vendor).
pub fn evidence_seeds(
    code: &str,
    semantic: &oxc_semantic::Semantic<'_>,
    graph: &ReferenceGraph,
) -> Vec<Seed> {
    let statements: Vec<u32> = (0..graph.units.len() as u32)
        .filter(|&u| is_statement(graph, u) && !graph.units[u as usize].runtime)
        .collect();
    let spans: Vec<oxc_span::Span> = statements
        .iter()
        .map(|&u| graph.units[u as usize].span)
        .collect();
    let mut seeds = Vec::new();
    for d in crate::modules::identity::scan_app_declarations(code, &spans) {
        let package = match d.reader.as_str() {
            "aws-sdk-client" => d.client.as_deref().map(aws_client_package),
            _ => d.package,
        };
        if let (Some(package), Some(unit)) = (package, graph.unit_at(d.start)) {
            seeds.push(Seed {
                unit,
                package,
                evidence: Evidence::Declared { reader: d.reader },
            });
        }
    }
    for comment in semantic.comments().iter().filter(|c| c.is_block()) {
        let Some(value) = crate::modules::bang_block_value(code, comment.span) else {
            continue;
        };
        let Some(package) = crate::modules::identity::readers::parse_banner(&value).pkg else {
            continue;
        };
        if let Some(unit) = banner_unit(graph, comment.span)
            && !graph.units[unit as usize].runtime
        {
            seeds.push(Seed {
                unit,
                package,
                evidence: Evidence::Banner,
            });
        }
    }
    seeds
}

/// The statement a banner belongs to (see [`evidence_seeds`]).
fn banner_unit(graph: &ReferenceGraph, comment: oxc_span::Span) -> Option<u32> {
    let next = match graph.unit_at(comment.start) {
        Some(u) if is_statement(graph, u) => return Some(u),
        Some(u)
            if graph.units[u as usize].kind == UnitKind::Factory && !graph.is_cut_factory(u) =>
        {
            return None;
        }
        // Inside a lazy wrapper, between body statements: the next one.
        _ => graph.units.partition_point(|x| x.span.start < comment.end) as u32,
    };
    match graph.units.get(next as usize)?.kind {
        UnitKind::Statement | UnitKind::LazyBody { .. } => Some(next),
        UnitKind::LazyModule => {
            let body = next + 1;
            (graph.units.get(body as usize)?.kind == UnitKind::LazyBody { module: next })
                .then_some(body)
        }
        UnitKind::Factory if graph.is_cut_factory(next) => Some(next + 1),
        UnitKind::FactoryBody { .. } => Some(next),
        UnitKind::Factory => None,
    }
}

/// `BedrockRuntimeClient` → `@aws-sdk/client-bedrock-runtime`: the client
/// package a smithy command's client names. A word starts at a capital that
/// follows a lower-case letter, so an acronym stays whole (`STSClient` →
/// `client-sts`); an all-capital run of two acronyms cannot be split
/// (`SSOOIDCClient` → `client-ssooidc`, the package is `client-sso-oidc`).
/// It is a family-level claim and an equality key within one run, never
/// shown as a name.
fn aws_client_package(client: &str) -> String {
    let stem = client.strip_suffix("Client").unwrap_or(client);
    let mut out = String::from("@aws-sdk/client-");
    let mut prev_lower = false;
    for c in stem.chars() {
        if c.is_ascii_uppercase() && prev_lower {
            out.push('-');
        }
        prev_lower = c.is_ascii_lowercase() || c.is_ascii_digit();
        out.push(c.to_ascii_lowercase());
    }
    out
}

/// Run discovery over `graph` from `seeds` with the standard library
/// profile and no index at hand (no containment).
pub fn discover(
    graph: &ReferenceGraph,
    seeds: &[Seed],
    families: &PackageFamilies,
    options: DiscoveryOptions,
) -> Discovery {
    discover_with(
        graph,
        seeds,
        families,
        options,
        &LibraryProfile::standard(),
        None,
    )
}

/// Run discovery over `graph` from `seeds` under `profile`; `containment`
/// guards every propagated claim when an index is at hand.
pub fn discover_with(
    graph: &ReferenceGraph,
    seeds: &[Seed],
    families: &PackageFamilies,
    options: DiscoveryOptions,
    profile: &LibraryProfile,
    containment: Option<&Containment<'_>>,
) -> Discovery {
    let groups = module_groups(graph);
    let guard = containment.map(|c| Guard {
        containment: c,
        evidence_conflict: options.evidence_conflict,
        literal_conflict: options.literal_conflict,
        contested_cover: options.contested_cover,
        helper_families: options.helper_families,
        payload_min_bytes: options.payload_min_bytes,
    });
    let all = &profile.propagation[..];
    let without_module: Vec<LibraryRule> = all
        .iter()
        .filter(|r| r.rule != Rule::Module)
        .copied()
        .collect();
    let capped: Vec<Seed>;
    let seeds = if options.declared_max_bytes > 0 {
        capped = seeds
            .iter()
            .filter(|s| {
                !matches!(s.evidence, Evidence::Declared { .. })
                    || graph.units[s.unit as usize].bytes <= options.declared_max_bytes
            })
            .cloned()
            .collect();
        &capped[..]
    } else {
        seeds
    };
    let mut active = settle_seeds(graph, seeds);
    let mut vetoed = Vec::new();
    let (st, rounds) = loop {
        let (st, rounds) = fixpoint(graph, &groups, &active, options.max_rounds, all, guard);
        let drop = match options.veto_support {
            VetoSupport::AllRules => isolated(&st, &groups, &active, families, containment),
            VetoSupport::WithoutModule => {
                let (support, _) = fixpoint(
                    graph,
                    &groups,
                    &active,
                    options.max_rounds,
                    &without_module,
                    guard,
                );
                isolated(&support, &groups, &active, families, containment)
            }
        };
        if drop.is_empty() {
            break (st, rounds);
        }
        let (gone, kept): (Vec<Seed>, Vec<Seed>) =
            active.into_iter().partition(|s| drop.contains(&s.unit));
        vetoed.extend(gone);
        active = kept;
    };
    let evidence: HashMap<u32, &Evidence> = active.iter().map(|s| (s.unit, &s.evidence)).collect();
    let claims = (0..graph.units.len() as u32)
        .filter_map(|u| {
            let (p, rule, round, by) = st.claimed[u as usize]?;
            Some((
                u,
                Claim {
                    package: st.packages[p as usize].clone(),
                    rule,
                    round,
                    evidence: evidence.get(&u).map(|e| (*e).clone()),
                    by,
                },
            ))
        })
        .collect();
    Discovery {
        claims,
        rounds,
        vetoed,
        library_class: down_class(&st, &groups, options.closure_floor),
        queue: candidate_queue(&st, &groups, options.queue_cap),
        boundary: boundary(&st, &groups),
    }
}

/// The run's pieces discovery reads (all chosen by the toolchain).
#[derive(Clone, Copy, Debug)]
pub struct BundlePieces {
    pub layout: crate::toolchain::BundleLayout,
    pub grammar: crate::toolchain::ModuleWrapperGrammar,
    pub interop: crate::toolchain::InteropHelpers,
}

/// The targeted round's node floor when the run does not set one (config
/// `library.lookup.targeted.min_nodes`).
pub const TARGETED_MIN_NODES: u32 = 20;
/// The targeted round's neighbour support when the run does not set one:
/// none — the region's own calls are the context.
pub const TARGETED_SUPPORT: usize = 0;

/// What a run asks discovery for (`--library-matching report` turns it
/// on): the per-unit listings, an index to look statement units up in,
/// and the run's tunables (the config file's `[library]` keys).
#[derive(Clone, Copy, Default)]
pub struct DiscoveryRequest<'x> {
    pub list_units: bool,
    pub index: Option<&'x crate::libindex::lookup::LibraryIndex>,
    /// The lookup's rules; None = the default (agreement).
    pub rules: Option<crate::libindex::lookup::LookupRules>,
    /// The targeted round's floor and support, `(min_nodes, support)`;
    /// None = [`TARGETED_MIN_NODES`], [`TARGETED_SUPPORT`].
    pub targeted: Option<(u32, usize)>,
    /// The fixpoint's tunables.
    pub options: DiscoveryOptions,
    /// Every unit with its fingerprints in the report (`unitRows`): the
    /// `discover` verb's scorer input (exp096). The pipeline never asks.
    pub unit_prints: bool,
}

impl<'x> DiscoveryRequest<'x> {
    /// The index lookup a run makes: the request's rules (default: the
    /// pipeline's) for every statement, and a targeted round on queued
    /// regions (a 20-node floor, no neighbour support: the region's own
    /// calls are the context).
    pub fn lookup(&self) -> Option<IndexLookup<'x>> {
        let rules = self.rules.unwrap_or_default();
        let (min_nodes, support) = self
            .targeted
            .unwrap_or((TARGETED_MIN_NODES, TARGETED_SUPPORT));
        Some(IndexLookup {
            index: self.index?,
            rules,
            targeted: Some(crate::libindex::lookup::LookupRules {
                min_nodes,
                support,
                ..rules
            }),
        })
    }
}

/// An index to look the bundle's statement units up in, and the rules:
/// `rules` for every statement (index seeds), `targeted` for the units of
/// each queued region after the first fixpoint (None: no targeted round).
#[derive(Clone, Copy)]
pub struct IndexLookup<'x> {
    pub index: &'x crate::libindex::lookup::LibraryIndex,
    pub rules: crate::libindex::lookup::LookupRules,
    pub targeted: Option<crate::libindex::lookup::LookupRules>,
}

/// Library discovery over a RAW bundle (comments still in it): the
/// reference graph over the layout's container (bundler facts from the
/// run's pieces; `factories` = the declarator spans of the classified
/// CommonJS modules), the evidence seeds plus `index_claims` (a unit's
/// span start → package, from outside the run — the lab's) plus the
/// index's own claims when `lookup` is given, then [`discover`]; then,
/// with a targeted rule, a lookup of every queued region and a second
/// [`discover`] with what it claimed.
///
/// The returned graph's edges carry their region scope
/// ([`library_regions`]: each claimed package is a region) — the scope's
/// producer. Nothing reads it yet (docs/matching-architecture.md §4).
pub fn discover_in_bundle(
    code: &str,
    ingest: &crate::ingest::Ingest<'_>,
    pieces: BundlePieces,
    factories: &[oxc_span::Span],
    index_claims: &[(u32, String)],
    lookup: Option<IndexLookup<'_>>,
    options: DiscoveryOptions,
) -> (ReferenceGraph, Discovery) {
    discover_in_bundle_with(
        code,
        ingest,
        pieces,
        factories,
        index_claims,
        lookup,
        options,
        &LibraryProfile::standard(),
    )
}

/// [`discover_in_bundle`] under a given library profile.
#[allow(clippy::too_many_arguments)]
pub fn discover_in_bundle_with(
    code: &str,
    ingest: &crate::ingest::Ingest<'_>,
    pieces: BundlePieces,
    factories: &[oxc_span::Span],
    index_claims: &[(u32, String)],
    lookup: Option<IndexLookup<'_>>,
    options: DiscoveryOptions,
    profile: &LibraryProfile,
) -> (ReferenceGraph, Discovery) {
    let (mut graph, found) = discover_unscoped(
        code,
        ingest,
        pieces,
        factories,
        index_claims,
        lookup,
        options,
        profile,
    );
    graph.assign_scopes(&library_regions(&graph, &found));
    (graph, found)
}

/// The library regions of a discovery: one region per claimed package
/// (ids in package-name order), spanning its claimed units. A unit inside
/// a claimed unit (a lazy body inside a claimed wrapper) is in that region
/// by containment.
pub fn library_regions(graph: &ReferenceGraph, found: &Discovery) -> crate::graph::edges::Regions {
    let packages: BTreeSet<&str> = found.claims.values().map(|c| c.package.as_str()).collect();
    let id: BTreeMap<&str, u32> = packages
        .into_iter()
        .enumerate()
        .map(|(i, p)| (p, i as u32))
        .collect();
    crate::graph::edges::Regions::new(
        found
            .claims
            .iter()
            .map(|(&u, c)| (graph.units[u as usize].span, id[c.package.as_str()]))
            .collect(),
    )
}

#[allow(clippy::too_many_arguments)]
fn discover_unscoped(
    code: &str,
    ingest: &crate::ingest::Ingest<'_>,
    pieces: BundlePieces,
    factories: &[oxc_span::Span],
    index_claims: &[(u32, String)],
    lookup: Option<IndexLookup<'_>>,
    options: DiscoveryOptions,
    profile: &LibraryProfile,
) -> (ReferenceGraph, Discovery) {
    let semantic = ingest.semantic();
    let (statements, facts, graph) =
        bundle_graph(code, ingest, pieces, factories, options.factory_bodies);
    let families = match (options.derived_families, lookup) {
        (false, _) => PackageFamilies::shipped(),
        (true, Some(l)) => {
            PackageFamilies::shipped().with_shipped_links_among(l.index.known_names())
        }
        (true, None) => PackageFamilies::shipped().with_shipped_links(),
    };
    let mut seeds: Vec<Seed> = index_claims
        .iter()
        .filter_map(|(start, package)| {
            Some(Seed {
                unit: graph.unit_at(*start)?,
                package: package.clone(),
                evidence: Evidence::Index,
            })
        })
        .collect();
    seeds.extend(evidence_seeds(code, semantic, &graph));
    let Some(lookup) = lookup else {
        let found = discover_with(&graph, &seeds, &families, options, profile, None);
        return (graph, found);
    };
    if options.banner_names == BannerNames::Known {
        seeds = known_banner_names(seeds, lookup.index);
    }
    let units =
        crate::cut::units::container_units(statements, factories, &facts, options.factory_bodies);
    let tables = crate::hash::serialize::SymbolTables::build(semantic);
    let blurred = facts.grammar.blurred_literals(statements, semantic);
    let prints = crate::libindex::units::unit_prints(&units, &tables, &blurred);
    // exp127: `terser_slices = "auto"` reads the bundle's prints once,
    // before any lookup
    lookup.index.decide_terser_slices(&prints, &lookup.rules);
    let looker = Looker {
        code,
        graph: &graph,
        prints: &prints,
        module_bytes: module_bytes(&graph),
        same: &|a: &str, b: &str| families.same(a, b),
    };
    let containment = Containment::new(
        lookup.index,
        code,
        &graph,
        &prints,
        &looker.module_bytes,
        lookup.rules,
        &families,
    )
    .with_specific_rivals(options.specific_rivals);
    let everything: Vec<u32> = (0..graph.units.len() as u32).collect();
    // The evidence-conflict guard reads index seeds too: the lookup picks
    // the best package under its specificity and package-count filters; a
    // unit the index as a whole gives to another family more is no seed.
    // The helper floor likewise: emitted helper code is no package's seed.
    let unconflicted = |s: &Seed| {
        !(options.evidence_conflict && containment.outweighed(s.unit, &s.package))
            && !(options.helper_families > 0
                && containment.helper_dominated(s.unit, options.helper_families))
    };
    seeds.extend(
        looker
            .claims(lookup.index, &everything, lookup.rules, Evidence::Index)
            .into_iter()
            .filter(unconflicted),
    );
    let present = |seeds: &[Seed]| {
        presence(
            seeds.to_vec(),
            &graph,
            &looker.module_bytes,
            &containment,
            &families,
            options,
        )
    };
    let found = unfence(discover_with(
        &graph,
        &present(&seeds),
        &families,
        options,
        profile,
        Some(&containment),
    ));
    let Some(targeted) = lookup.targeted else {
        return (graph, found);
    };
    let mut more = Vec::new();
    for q in &found.queue {
        let region: Vec<u32> = q
            .region
            .iter()
            .copied()
            .filter(|u| !found.claims.contains_key(u))
            .collect();
        more.extend(
            looker
                .claims(lookup.index, &region, targeted, Evidence::QueueIndex)
                .into_iter()
                .filter(|s| q.uses.iter().any(|used| families.same(used, &s.package)))
                .filter(unconflicted),
        );
    }
    if more.is_empty() {
        return (graph, found);
    }
    seeds.extend(more);
    let found = unfence(discover_with(
        &graph,
        &present(&seeds),
        &families,
        options,
        profile,
        Some(&containment),
    ));
    (graph, found)
}

/// The seeds with every banner name resolved to a package `index` knows
/// ([`BannerNames::Known`]); a banner naming none is no seed.
fn known_banner_names(
    seeds: Vec<Seed>,
    index: &crate::libindex::lookup::LibraryIndex,
) -> Vec<Seed> {
    seeds
        .into_iter()
        .filter_map(|s| match s.evidence {
            Evidence::Banner => Some(Seed {
                package: index.resolve_known(&s.package)?,
                ..s
            }),
            _ => Some(s),
        })
        .collect()
}

/// The container statements, bundler facts and statement reference graph
/// of a RAW bundle — what discovery and [`unit_rows`] both read.
fn bundle_graph<'a>(
    code: &str,
    ingest: &crate::ingest::Ingest<'a>,
    pieces: BundlePieces,
    factories: &[oxc_span::Span],
    factory_bodies: bool,
) -> (
    &'a [oxc_ast::ast::Statement<'a>],
    crate::cut::units::BundlerFacts,
    ReferenceGraph,
) {
    let semantic = ingest.semantic();
    let wrapper = pieces.layout.find_wrapper(ingest.program, semantic);
    let statements = crate::cut::container_statements(
        ingest.program,
        semantic,
        wrapper.as_ref().map(|w| w.body_span),
    );
    let facts = crate::cut::units::BundlerFacts::recognise(
        code,
        statements,
        pieces.grammar,
        pieces.interop,
    );
    let graph = crate::graph::references::build_reference_graph_with(
        code,
        semantic,
        statements,
        factories,
        &facts,
        factory_bodies,
    );
    (statements, facts, graph)
}

/// One fingerprint in a [`UnitRow`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PrintRow {
    pub start: u32,
    pub end: u32,
    pub hash: String,
    pub nodes: u32,
    pub content: u32,
}

impl PrintRow {
    fn of(p: &crate::libindex::units::UnitPrint) -> PrintRow {
        PrintRow {
            start: p.span.start,
            end: p.span.end,
            hash: p.hash.clone(),
            nodes: p.nodes,
            content: p.content,
        }
    }
}

/// One unit of the reference graph with what the index lookup reads of it
/// (`unitRows`, the `discover` verb's scorer input): its fingerprints, the
/// module size specificity compares, and how many bindings it declares.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnitRow {
    pub start: u32,
    pub end: u32,
    pub bytes: u32,
    /// `Statement`, `LazyModule`, `LazyBody`, `FactoryBody` or `Factory`.
    pub kind: &'static str,
    /// A lazy or factory body's module unit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module: Option<u32>,
    pub runtime: bool,
    pub module_bytes: u64,
    /// Symbols declared inside this unit and no unit nested in it.
    pub bindings: u32,
    /// The whole unit's print (None: a container — wrapper or module).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub whole: Option<PrintRow>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub functions: Vec<PrintRow>,
}

/// Every unit of the bundle's reference graph, in graph order, with its
/// prints (the lookup's: `libindex::units::unit_prints` over the same
/// units discovery builds; `factory_bodies` as the run's
/// [`DiscoveryOptions::factory_bodies`]).
pub fn unit_rows(
    code: &str,
    ingest: &crate::ingest::Ingest<'_>,
    pieces: BundlePieces,
    factories: &[oxc_span::Span],
    factory_bodies: bool,
) -> Vec<UnitRow> {
    use crate::cut::units::UnitKind;
    let semantic = ingest.semantic();
    let (statements, facts, graph) = bundle_graph(code, ingest, pieces, factories, factory_bodies);
    let units = crate::cut::units::container_units(statements, factories, &facts, factory_bodies);
    let tables = crate::hash::serialize::SymbolTables::build(semantic);
    let blurred = facts.grammar.blurred_literals(statements, semantic);
    let prints = crate::libindex::units::unit_prints(&units, &tables, &blurred);
    let sizes = module_bytes(&graph);
    let mut bindings = vec![0u32; graph.units.len()];
    let scoping = semantic.scoping();
    for symbol in scoping.symbol_ids() {
        if let Some(u) = graph.unit_at(scoping.symbol_span(symbol).start) {
            bindings[u as usize] += 1;
        }
    }
    graph
        .units
        .iter()
        .enumerate()
        .map(|(i, u)| {
            let (kind, module) = match u.kind {
                UnitKind::Statement => ("Statement", None),
                UnitKind::LazyModule => ("LazyModule", None),
                UnitKind::LazyBody { module } => ("LazyBody", Some(module)),
                UnitKind::FactoryBody { module } => ("FactoryBody", Some(module)),
                UnitKind::Factory => ("Factory", None),
            };
            let p = prints.get(i).and_then(Option::as_ref);
            UnitRow {
                start: u.span.start,
                end: u.span.end,
                bytes: u.bytes,
                kind,
                module,
                runtime: u.runtime,
                module_bytes: sizes[i],
                bindings: bindings[i],
                whole: p.map(|p| PrintRow::of(&p.whole)),
                functions: p
                    .map(|p| p.functions.iter().map(PrintRow::of).collect())
                    .unwrap_or_default(),
            }
        })
        .collect()
}

/// The index lookup's view of the bundle: each statement unit's prints,
/// module size and text.
struct Looker<'l> {
    code: &'l str,
    graph: &'l ReferenceGraph,
    prints: &'l [Option<crate::libindex::units::StatementPrints>],
    module_bytes: Vec<u64>,
    same: &'l dyn Fn(&str, &str) -> bool,
}

impl Looker<'_> {
    /// Seeds for the statement units among `units` the index claims.
    fn claims(
        &self,
        index: &crate::libindex::lookup::LibraryIndex,
        units: &[u32],
        rules: crate::libindex::lookup::LookupRules,
        evidence: Evidence,
    ) -> Vec<Seed> {
        use crate::libindex::lookup::{UnitQuery, claim_units};
        let queries: Vec<UnitQuery<'_>> = units
            .iter()
            .filter(|&&u| is_statement(self.graph, u) && !self.graph.units[u as usize].runtime)
            .filter_map(|&u| {
                let span = self.graph.units[u as usize].span;
                Some(UnitQuery {
                    unit: u,
                    prints: self.prints.get(u as usize)?.as_ref()?,
                    module_bytes: self.module_bytes[u as usize],
                    text: &self.code[span.start as usize..span.end as usize],
                })
            })
            .collect();
        claim_units(index, &queries, rules, self.same)
            .into_iter()
            .map(|c| Seed {
                unit: c.unit,
                package: c.package,
                evidence: evidence.clone(),
            })
            .collect()
    }
}

/// One claimed statement in the report.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimRow {
    pub start: u32,
    pub end: u32,
    pub package: String,
    pub rule: Rule,
    pub round: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence: Option<Evidence>,
}

/// A span and a package (vetoed seeds; library-class rows).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpanRow {
    pub start: u32,
    pub end: u32,
    pub package: String,
}

/// One queued region in the report.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueRow {
    pub start: u32,
    pub end: u32,
    pub region_units: u32,
    pub region_bytes: u64,
    pub uses: Vec<String>,
}

/// The graph's size, for the report.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphStats {
    pub units: u32,
    pub edges: u32,
    pub statements: u32,
    pub statement_bytes: u64,
    pub runtime_units: u32,
    pub factories: u32,
    pub lazy_modules: u32,
}

/// One package's totals in the report (statement bytes).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageRow {
    pub package: String,
    pub seed_units: u32,
    pub seed_bytes: u64,
    /// Claimed by the graph rules (DOWN, MODULE, GLUE).
    pub grown_units: u32,
    pub grown_bytes: u64,
    /// Index seeds the isolation veto dropped.
    pub vetoed_units: u32,
    pub vetoed_bytes: u64,
    /// Library class reached from this package's claims.
    pub class_units: u32,
    pub class_bytes: u64,
    /// Queued regions that reference this package.
    pub queue_units: u32,
    pub queue_bytes: u64,
    /// The boundary check: INTO edges landing on this package's claims,
    /// and on how many of its units.
    pub into_edges: u32,
    pub into_units: u32,
}

/// `--diagnostics` `libraryDiscovery`: totals and one row per package by
/// default; every claimed, vetoed, library-class and queued unit only when
/// asked for (`--library-discovery-units`). Report-only: nothing is named,
/// moved or renamed by it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryReport {
    /// The library index's id (sha256 of its manifest), when one was read.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index_id: Option<String>,
    /// The recipe slice the index was read from, and why that one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index_slice: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index_slice_reason: Option<String>,
    pub graph: GraphStats,
    /// The library profile's rules, in order, each with what it reads
    /// (`matching::rules`): a claim row's `rule` (and a seed's evidence
    /// kind) names one of them.
    pub profile: Vec<crate::matching::rules::RuleRow>,
    /// Seeds KEPT (one per unit, after the isolation veto), by evidence kind.
    pub seeds: BTreeMap<String, u32>,
    pub rounds: Vec<RoundStats>,
    /// Statement bytes claimed: by seeds, and grown by the rules.
    pub seed_statement_bytes: u64,
    pub grown_statement_bytes: u64,
    pub vetoed_statement_bytes: u64,
    pub library_class_statement_bytes: u64,
    pub queue_statement_bytes: u64,
    pub packages: Vec<PackageRow>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub claims: Vec<ClaimRow>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub vetoed: Vec<SpanRow>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub library_class: Vec<SpanRow>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub queue: Vec<QueueRow>,
    /// Every unit with its prints, only when asked for
    /// ([`DiscoveryRequest::unit_prints`]; the `discover` verb).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub unit_rows: Vec<UnitRow>,
}

impl DiscoveryReport {
    /// The report; `units` adds the per-unit listings.
    pub fn new(graph: &ReferenceGraph, found: &Discovery, units: bool) -> DiscoveryReport {
        let mut r = DiscoveryReport {
            graph: graph_stats(graph),
            rounds: found.rounds.clone(),
            ..DiscoveryReport::default()
        };
        let mut packages: BTreeMap<String, PackageRow> = BTreeMap::new();
        for (&u, c) in &found.claims {
            let b = statement_bytes(graph, u);
            if let Some(e) = &c.evidence {
                *r.seeds.entry(e.kind().to_string()).or_default() += 1;
            }
            let p = package_row(&mut packages, &c.package);
            if c.rule == Rule::Seed {
                r.seed_statement_bytes += b;
                p.seed_units += 1;
                p.seed_bytes += b;
            } else {
                r.grown_statement_bytes += b;
                p.grown_units += 1;
                p.grown_bytes += b;
            }
        }
        for s in &found.vetoed {
            let b = statement_bytes(graph, s.unit);
            r.vetoed_statement_bytes += b;
            let p = package_row(&mut packages, &s.package);
            p.vetoed_units += 1;
            p.vetoed_bytes += b;
        }
        for (&u, package) in &found.library_class {
            let b = statement_bytes(graph, u);
            r.library_class_statement_bytes += b;
            let p = package_row(&mut packages, package);
            p.class_units += 1;
            p.class_bytes += b;
        }
        for q in &found.queue {
            let b = statement_bytes(graph, q.unit);
            r.queue_statement_bytes += b;
            for used in &q.uses {
                let p = package_row(&mut packages, used);
                p.queue_units += 1;
                p.queue_bytes += b;
            }
        }
        for (package, b) in &found.boundary {
            let p = package_row(&mut packages, package);
            p.into_edges = b.into_edges;
            p.into_units = b.into_units;
        }
        r.packages = packages.into_values().collect();
        r.profile = LibraryProfile::standard().rules().describe();
        if units {
            r.list_units(graph, found);
        }
        r
    }

    fn list_units(&mut self, graph: &ReferenceGraph, found: &Discovery) {
        let span = |u: u32| graph.units[u as usize].span;
        self.claims = found
            .claims
            .iter()
            .map(|(&u, c)| ClaimRow {
                start: span(u).start,
                end: span(u).end,
                package: c.package.clone(),
                rule: c.rule,
                round: c.round,
                evidence: c.evidence.clone(),
            })
            .collect();
        let span_row = |u: u32, package: &str| SpanRow {
            start: span(u).start,
            end: span(u).end,
            package: package.to_string(),
        };
        self.vetoed = found
            .vetoed
            .iter()
            .map(|s| span_row(s.unit, &s.package))
            .collect();
        self.library_class = found
            .library_class
            .iter()
            .map(|(&u, p)| span_row(u, p))
            .collect();
        self.queue = found
            .queue
            .iter()
            .map(|q| QueueRow {
                start: span(q.unit).start,
                end: span(q.unit).end,
                region_units: q.region_units,
                region_bytes: q.region_bytes,
                uses: q.uses.clone(),
            })
            .collect();
    }
}

fn package_row<'m>(
    rows: &'m mut BTreeMap<String, PackageRow>,
    package: &str,
) -> &'m mut PackageRow {
    rows.entry(package.to_string())
        .or_insert_with(|| PackageRow {
            package: package.to_string(),
            ..PackageRow::default()
        })
}

fn statement_bytes(graph: &ReferenceGraph, u: u32) -> u64 {
    if is_statement(graph, u) {
        u64::from(graph.units[u as usize].bytes)
    } else {
        0
    }
}

fn graph_stats(graph: &ReferenceGraph) -> GraphStats {
    let mut stats = GraphStats {
        units: graph.units.len() as u32,
        edges: graph.edges.len() as u32,
        ..GraphStats::default()
    };
    for (u, unit) in graph.units.iter().enumerate() {
        match unit.kind {
            UnitKind::Factory => stats.factories += 1,
            UnitKind::LazyModule => stats.lazy_modules += 1,
            _ => {
                stats.statements += 1;
                stats.statement_bytes += statement_bytes(graph, u as u32);
            }
        }
        stats.runtime_units += u32::from(unit.runtime);
    }
    stats
}
