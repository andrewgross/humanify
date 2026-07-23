# The naming pipeline: every pass, in execution order

The cross-version naming system is a strict cascade: stronger evidence
applies first, weaker tiers fill only what remains, and every rename goes
through `attemptValidatedRename` (collisions reject — a tier never
overwrites another's work). `--diagnostics` records the per-identifier
attempt trail through exactly these passes (`strategyTrails` in the
report; `experiments/034-eval-harness/name-flow-sankey.ts` renders it).

Reporting convention: any breakdown leads with the TOTAL population and
ends with the REMAINING count, so effectiveness reads at a glance.

## Phase 0 — matching (prior AST alive, nothing renamed yet)

`matchPriorVersion` computes evidence; no names are applied here.

| # | pass | what it does |
|---|------|--------------|
| 0.1 | **Function cascade** | Pairs prior↔fresh functions through resolution tiers, strongest first: unique `structuralHash` → binding-identity → member-key → enclosing-statement hash → callee shapes → caller shapes → callee hashes → two-hop shapes → shingle similarity → scope ordinal. Injectivity demotion and singleton rejection guard ambiguous buckets (abstain > guess). |
| 0.2 | **Binding cascade** | Module-level bindings run the SAME cascade, alternating rounds with 0.1 — each side's matches crack the other's same-hash buckets via reference identity. Produces `moduleBindingRenames`. |
| 0.3 | **Close-match detection** | Unmatched fns with high fingerprint similarity get the prior code + partial positional name pairs as LLM context (never mechanical head renames — content changed). |
| 0.4 | **Role evidence** | Compact `BindingRole`s (content hash/shingles + callee ids) for unconsumed prior module bindings AND prior function-declaration heads — carried past the prior AST's release for the single-vote pins. |
| 0.5 | **Statement-twin compute** | Statements whose rename-invariant `statementHash` is unique 1:1 on both sides are whole-statement identity (literals included). Equal-count family buckets pair by matched-reference identity keys. Also collects private-name pairs and cascade-conflict overrides (identity-confirmed crossed heads). |

## Phase 1 — mechanical transfer (apply order in `prior-transfer.ts`)

| # | pass (trail name) | what it does |
|---|-------------------|--------------|
| 1.1 | **statement-twin** | Applies FIRST: twin slots outrank an ordinal exact match that cross-paired same-shaped siblings and any close-match guess (stale pairs then drop). Includes positional PrivateName rewrites; outer-binding slots become demoted vote testimony, never direct renames. |
| 1.2 | **exact-match** | Matched functions' slot tables (byte-identical-modulo-names): params + locals renamed to prior names. References to bindings OUTSIDE the function are routed to vote propagation with exact-grade testimony. |
| 1.3 | **close-match** | Positional signature pairs + statement-aligned body locals for close-matched fns; externals routed to votes at non-exact grade. Head names are NOT mechanically transferred (content changed → LLM decides with the prior as a suggestion). |
| 1.4 | **binding-cascade** | Applies 0.2's module-binding renames. |
| 1.5 | **module-vote / fn-name-vote** | External-reference votes: a binding referenced by matched fns inherits the prior name its callers testify to, at a ≥2 agreeing-vote floor. |
| 1.6 | **module-pin / fn-name-pin** | Below the vote floor, the shared single-vote ladder (`single-vote-pin.ts`): exactly one EXACT slot vote + cross-map name injectivity + role corroboration (hash/shingles + callee veto) + validated rename. Close-matched fns excluded. This is what names cold same-shaped families (get↔retrieve class). |
| 1.7 | **closure-capture** | Close-matched parent fns' captured locals named by exact-matched children's votes. |
| 1.8 | **close-match suggestions** | Resolved binding names injected into close-match LLM context (not renames). |
| 1.9 | **retry** | Deferred re-attempt of collision-rejected renames; swaps/chains unwind as phases free tokens, pure cycles broken via a temp name. |

## Phase 2 — LLM naming (whatever is still pending)

| # | pass | what it does |
|---|------|--------------|
| 2.1 | **Wave-scheduled LLM batches** | Cold + close-matched fns, graph order, frozen pre-wave prompt context, barrier applies (`--wave-scheduling`, default ON; byte-identical reruns from a saturated `--llm-cache`). Close-matched prompts carry prior code + name suggestions. |
| 2.2 | **prior-name snap** | An LLM suggestion sharing its stem with exactly one prior name snaps to that prior name (decoration flips: identityVal↔identityVar). |
| 2.3 | **library-prefix / fallback** | Vendor-classified naming and mechanical fallbacks. |

## Phase 3 — floor + post-output polish

| # | pass | what it does |
|---|------|--------------|
| 3.1 | **Naming floor** (`--naming-floor`, default ON) | Deterministic minted-token coverage pre-generate: derive class/fn-expression inner ids, undecorate (decoration retry). The LLM sweep half runs here only when it cannot be prior-aware. |
| 3.2 | **Generate output** | Babel generate; boot/validation gates. |
| 3.3 | **Reconcile prior-diff** (`--reconcile-prior-diff`) | Text-diff pass over (prior output, fresh output): asymmetric tier (minted→descriptive), descriptive tier (clean-declaration proof), consumer tier (changed-leaf heads witnessed by ≥2 unchanged callers in distinct hunks). Corpus gate abstains entirely on shuffle pairs (<50% line alignment). Pure-rename invariant enforced; discards itself on violation. |
| 3.4 | **Deferred sweep** | The prior-aware LLM coverage sweep over the shipping output — whatever is minted AFTER reconciliation truly has no prior counterpart. |
| 3.5 | **Split** (out of naming scope) | Stable file assignment inherits via ledger hashes; twin/identity tiers of their own — see `src/split/`. |

## Why this order

Evidence strength is monotone decreasing: whole-statement identity
(literals included) > exact slot tables > cascade identity > multi-witness
votes > single corroborated votes > LLM with context > LLM cold > floor.
Each tier only touches pending work; `postSettleAttempts` in the strategy
trail flags any violation (currently 174 on pair 216 — a known follow-up).

## Toward a declarative strategy registry (proposal)

Today the order lives implicitly in `applyPriorVersionIfPresent`'s call
sequence plus `plugin.ts` post-stages. The trail work already forced each
pass to carry a NAME — the natural next step is one ordered registry:

```ts
interface NamingStrategy {
  name: string;                    // the trail/funnel/doc name
  phase: "transfer" | "llm" | "post";
  apply(ctx: NamingContext): StrategyOutcome;  // records its own trail
}
const NAMING_PIPELINE: NamingStrategy[] = [statementTwin, exactMatch, ...];
```

What this buys: the doc above becomes generated-from-code (never stale),
the trail hooks move into the runner (one place instead of ten), probe
toggles become registry filters (ablations by name), and adding a tier is
appending an element. What it does NOT change: the tiers share staged
data (prior AST lifetime, external-ref routing, retry queue), so this is
an ordered staged pipeline with explicit context handoffs — not fully
independent plugins. Incremental path: (1) lift the phase-1 call sequence
into a step descriptor array, (2) move each apply block behind the
interface, (3) generate this doc's phase-1 table from the registry.
