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
//! plus the wrapper's own body.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde::Serialize;

use crate::graph::references::{ReferenceGraph, UnitKind};
use crate::libindex::families::PackageFamilies;

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

/// The tunables (design §5): the content floor a DOWN-CLASS start needs,
/// the largest region queued for an index lookup, and what may support a
/// seed against the isolation veto.
#[derive(Clone, Copy, Debug)]
pub struct DiscoveryOptions {
    pub closure_floor: u32,
    pub queue_cap: u32,
    pub max_rounds: u32,
    pub veto_support: VetoSupport,
}

impl Default for DiscoveryOptions {
    fn default() -> Self {
        DiscoveryOptions {
            closure_floor: 30,
            queue_cap: 256_000,
            max_rounds: 50,
            veto_support: VetoSupport::WithoutModule,
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
}

/// Is `u` a statement (top-level or of an init body) — the population the
/// rules claim from (wrappers are GLUE's; factories are external).
fn is_statement(graph: &ReferenceGraph, u: u32) -> bool {
    matches!(
        graph.units[u as usize].kind,
        UnitKind::Statement | UnitKind::LazyBody { .. }
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
    let mut pending: Vec<u32> = Vec::new();
    for u in 0..n as u32 {
        match graph.units[u as usize].kind {
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
    claimed: Vec<Option<(u32, Rule, u32)>>,
}

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

    /// DOWN: every caller is claimed as one package.
    fn down(&self, u: u32) -> Option<u32> {
        let callers = &self.graph.inn[u as usize];
        if callers.is_empty() {
            return None;
        }
        self.one_package(callers.iter().copied())
    }

    /// Does `u`'s closure over unidentified units stay inside group `g`'s
    /// statements?
    fn closed_in(&self, groups: &Groups, g: u32, u: u32) -> bool {
        let inside = |w: u32| groups.of[w as usize] == Some(g) && is_statement(self.graph, w);
        let mut seen = BTreeSet::from([u]);
        let mut stack = vec![u];
        while let Some(x) = stack.pop() {
            for &w in &self.graph.out[x as usize] {
                if seen.contains(&w) || self.ext(w) {
                    continue;
                }
                if !inside(w) {
                    return false;
                }
                seen.insert(w);
                stack.push(w);
            }
        }
        true
    }

    /// MODULE over one group whose claims are all one package.
    fn module(&self, groups: &Groups, g: u32, out: &mut Vec<(u32, u32, Rule)>) {
        let members = &groups.statements[g as usize];
        let mut packages = members.iter().filter_map(|&u| self.package_of(u));
        let Some(p) = packages.next() else {
            return;
        };
        if packages.any(|q| q != p) {
            return;
        }
        for &u in members {
            if self.ext(u) || !self.closed_in(groups, g, u) {
                continue;
            }
            // The module's own wrapper counts as outside until GLUE claims
            // it (the lab prototype's rule): a body statement is filled
            // only through DOWN or once its init is identified.
            let outside_caller = self.graph.inn[u as usize].iter().any(|&c| {
                !(groups.of[c as usize] == Some(g) && is_statement(self.graph, c)) && !self.ext(c)
            });
            if !outside_caller {
                out.push((u, p, Rule::Module));
            }
        }
    }

    /// One round's claims, rule order DOWN, MODULE, GLUE (first wins).
    fn round(&self, groups: &Groups, module_rule: bool) -> Vec<(u32, u32, Rule)> {
        let mut found = Vec::new();
        for u in 0..self.graph.units.len() as u32 {
            if is_statement(self.graph, u)
                && !self.ext(u)
                && let Some(p) = self.down(u)
            {
                found.push((u, p, Rule::Down));
            }
        }
        for g in (0..groups.statements.len() as u32).filter(|_| module_rule) {
            self.module(groups, g, &mut found);
        }
        for (&wrapper, body) in &groups.bodies {
            if !self.ext(wrapper)
                && let Some(p) = self.one_package(body.iter().copied())
            {
                found.push((wrapper, p, Rule::Glue));
            }
        }
        let mut seen = BTreeSet::new();
        found.retain(|&(u, _, _)| seen.insert(u));
        found
    }
}

/// Run the fixpoint from `seeds`; the rounds' tally.
fn fixpoint<'g>(
    graph: &'g ReferenceGraph,
    groups: &Groups,
    seeds: &[Seed],
    max_rounds: u32,
    module_rule: bool,
) -> (State<'g>, Vec<RoundStats>) {
    let mut st = State {
        graph,
        packages: Vec::new(),
        claimed: vec![None; graph.units.len()],
    };
    for seed in seeds {
        let p = st.intern(&seed.package);
        st.claimed[seed.unit as usize] = Some((p, Rule::Seed, 0));
    }
    let mut rounds = Vec::new();
    for k in 1..=max_rounds {
        let added = st.round(groups, module_rule);
        if added.is_empty() {
            break;
        }
        let mut stats = RoundStats {
            round: k,
            units: added.len() as u32,
            ..RoundStats::default()
        };
        for &(u, p, rule) in &added {
            st.claimed[u as usize] = Some((p, rule, k));
            *stats.by_rule.entry(rule).or_default() += 1;
            if is_statement(graph, u) {
                stats.statement_bytes += u64::from(graph.units[u as usize].bytes);
            }
        }
        rounds.push(stats);
    }
    (st, rounds)
}

/// The index seeds with no neighbour (runtime aside) claimed as their own
/// family.
fn isolated(st: &State<'_>, seeds: &[Seed], families: &PackageFamilies) -> BTreeSet<u32> {
    let mut family_cache: HashMap<u32, String> = HashMap::new();
    let mut family = |p: u32| {
        family_cache
            .entry(p)
            .or_insert_with(|| families.family(&st.packages[p as usize]))
            .clone()
    };
    let mut out = BTreeSet::new();
    for seed in seeds.iter().filter(|s| s.evidence.is_index()) {
        let Some(p) = st.package_of(seed.unit) else {
            continue;
        };
        let own = family(p);
        let u = seed.unit as usize;
        let supported = st.graph.out[u]
            .iter()
            .chain(&st.graph.inn[u])
            .filter(|&&w| !st.graph.units[w as usize].runtime)
            .filter_map(|&w| st.package_of(w))
            .any(|q| family(q) == own);
        if !supported {
            out.insert(seed.unit);
        }
    }
    out
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

/// DOWN-CLASS (module doc).
fn down_class(st: &State<'_>, floor: u32) -> BTreeMap<u32, String> {
    let graph = st.graph;
    let mut class: BTreeMap<u32, String> = BTreeMap::new();
    for u in 0..graph.units.len() as u32 {
        let Some(p) = st.package_of(u) else {
            continue;
        };
        if graph.units[u as usize].bytes < floor {
            continue;
        }
        let mut stack: Vec<u32> = graph.out[u as usize].clone();
        while let Some(x) = stack.pop() {
            if class.contains_key(&x) || st.ext(x) {
                continue;
            }
            class.insert(x, st.packages[p as usize].clone());
            stack.extend(graph.out[x as usize].iter().copied());
        }
    }
    class
}

/// The candidate queue (module doc).
fn candidate_queue(st: &State<'_>, cap: u32) -> Vec<QueueEntry> {
    let graph = st.graph;
    let mut queue = Vec::new();
    for v in 0..graph.units.len() as u32 {
        if !is_statement(graph, v) || st.ext(v) {
            continue;
        }
        let uses: BTreeSet<&str> = graph.out[v as usize]
            .iter()
            .filter_map(|&w| st.package_of(w))
            .map(|p| st.packages[p as usize].as_str())
            .collect();
        if uses.is_empty() {
            continue;
        }
        if let Some((region, bytes)) = bounded_closure(st, v, u64::from(cap)) {
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

/// |D(v)| over unidentified units, None past `cap` bytes.
fn bounded_closure(st: &State<'_>, v: u32, cap: u64) -> Option<(Vec<u32>, u64)> {
    let mut seen = BTreeSet::from([v]);
    let mut order = vec![v];
    let mut stack = vec![v];
    let mut bytes = 0u64;
    while let Some(x) = stack.pop() {
        bytes += u64::from(st.graph.units[x as usize].bytes);
        if bytes > cap {
            return None;
        }
        for &w in &st.graph.out[x as usize] {
            if !st.ext(w) && seen.insert(w) {
                order.push(w);
                stack.push(w);
            }
        }
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
        Some(u) if graph.units[u as usize].kind == UnitKind::Factory => return None,
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
        UnitKind::Factory | UnitKind::FactoryBody { .. } => None,
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

/// Run discovery over `graph` from `seeds`.
pub fn discover(
    graph: &ReferenceGraph,
    seeds: &[Seed],
    families: &PackageFamilies,
    options: DiscoveryOptions,
) -> Discovery {
    let groups = module_groups(graph);
    let mut active = settle_seeds(graph, seeds);
    let mut vetoed = Vec::new();
    let (st, rounds) = loop {
        let (st, rounds) = fixpoint(graph, &groups, &active, options.max_rounds, true);
        let drop = match options.veto_support {
            VetoSupport::AllRules => isolated(&st, &active, families),
            VetoSupport::WithoutModule => {
                let (support, _) = fixpoint(graph, &groups, &active, options.max_rounds, false);
                isolated(&support, &active, families)
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
            let (p, rule, round) = st.claimed[u as usize]?;
            Some((
                u,
                Claim {
                    package: st.packages[p as usize].clone(),
                    rule,
                    round,
                    evidence: evidence.get(&u).map(|e| (*e).clone()),
                },
            ))
        })
        .collect();
    Discovery {
        claims,
        rounds,
        vetoed,
        library_class: down_class(&st, options.closure_floor),
        queue: candidate_queue(&st, options.queue_cap),
    }
}

/// The run's pieces discovery reads (all chosen by the toolchain).
#[derive(Clone, Copy, Debug)]
pub struct BundlePieces {
    pub layout: crate::toolchain::BundleLayout,
    pub grammar: crate::toolchain::ModuleWrapperGrammar,
    pub interop: crate::toolchain::InteropHelpers,
}

/// What a run asks discovery for (`--diagnostics` turns it on): the
/// per-unit listings, and an index to look statement units up in.
#[derive(Clone, Copy, Default)]
pub struct DiscoveryRequest<'x> {
    pub list_units: bool,
    pub index: Option<&'x crate::libindex::lookup::LibraryIndex>,
}

impl<'x> DiscoveryRequest<'x> {
    /// The index lookup a run makes: the default rules for every statement,
    /// and a targeted round on queued regions (a 20-node floor, no
    /// neighbour support: the region's own calls are the context).
    pub fn lookup(&self) -> Option<IndexLookup<'x>> {
        let rules = crate::libindex::lookup::LookupRules::default();
        Some(IndexLookup {
            index: self.index?,
            rules,
            targeted: Some(crate::libindex::lookup::LookupRules {
                min_nodes: 20,
                support: 0,
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
pub fn discover_in_bundle(
    code: &str,
    ingest: &crate::ingest::Ingest<'_>,
    pieces: BundlePieces,
    factories: &[oxc_span::Span],
    index_claims: &[(u32, String)],
    lookup: Option<IndexLookup<'_>>,
    options: DiscoveryOptions,
) -> (ReferenceGraph, Discovery) {
    let semantic = ingest.semantic();
    let wrapper = pieces.layout.find_wrapper(ingest.program, semantic);
    let statements = crate::modules::container_statements(
        ingest.program,
        semantic,
        wrapper.as_ref().map(|w| w.body_span),
    );
    let facts = crate::graph::references::BundlerFacts::recognise(
        code,
        statements,
        pieces.grammar,
        pieces.interop,
    );
    let graph = crate::graph::references::build_reference_graph(
        code, semantic, statements, factories, &facts,
    );
    let families = PackageFamilies::shipped();
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
        let found = discover(&graph, &seeds, &families, options);
        return (graph, found);
    };
    let units = crate::graph::references::container_units(statements, factories, &facts, false);
    let tables = crate::hash::serialize::SymbolTables::build(semantic);
    let prints = crate::libindex::units::unit_prints(&units, &tables);
    let looker = Looker {
        code,
        graph: &graph,
        prints: &prints,
        module_bytes: module_bytes(&graph),
        family: &|p: &str| families.family(p),
    };
    let everything: Vec<u32> = (0..graph.units.len() as u32).collect();
    seeds.extend(looker.claims(lookup.index, &everything, lookup.rules, Evidence::Index));
    let found = discover(&graph, &seeds, &families, options);
    let Some(targeted) = lookup.targeted else {
        return (graph, found);
    };
    let mut more = Vec::new();
    for q in &found.queue {
        let used: BTreeSet<String> = q.uses.iter().map(|p| families.family(p)).collect();
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
                .filter(|s| used.contains(&families.family(&s.package))),
        );
    }
    if more.is_empty() {
        return (graph, found);
    }
    seeds.extend(more);
    let found = discover(&graph, &seeds, &families, options);
    (graph, found)
}

/// The index lookup's view of the bundle: each statement unit's prints,
/// module size and text.
struct Looker<'l> {
    code: &'l str,
    graph: &'l ReferenceGraph,
    prints: &'l [Option<crate::libindex::units::StatementPrints>],
    module_bytes: Vec<u64>,
    family: &'l dyn Fn(&str) -> String,
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
        claim_units(index, &queries, rules, self.family)
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
    pub graph: GraphStats,
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
        r.packages = packages.into_values().collect();
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
