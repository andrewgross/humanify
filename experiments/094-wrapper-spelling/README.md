# exp094 — comparing the arrow and function forms of bundler wrappers, reliably

## STATUS

- **State: CUT 1 MERGED on main (`exp094-wrapper-spelling`); CUT 2 — the
  statement-hash mirror — DONE on branch `exp094b-statement-spelling`
  (2026-10-02), fixture-proven (statement recall 2/3 → 3/3), pending the
  same batched gpt-oss eval.** Cut 2 is DECISION-CHANGING (statement
  identity feeds the split's placement/carry paths), so it joins cut 1 in
  the batched eval queue; until that eval lands, the numbers below are
  stub-/fixture-proven at the matcher level, not cold-eval-proven at the
  naming level.
- **Andrew's call (2026-10-02):** "Let's see if we can find a way to
  RELIABLY compare the arrow vs function forms."
- **What this resolves:** exp092's lead 1 (the wrapper-spelling matcher
  miss) — FIXED and fixture-proven (recall 2/3 → 3/3, the pair now matching
  at `structuralHashUnique`, the strictest tier). The post-cutover backlog's
  item 12 ("composeDiff tier-3 head-tolerant repair … would have made
  207→208 read ~+700 instead of +9,162") — now MEASURED EXACTLY: 8,230 of
  the hop's real charge is spelling (the residue is the genuine
  repackaging, ~930 of the rust-vs-ts gap, consistent with that ~700
  estimate within composition differences).
- **Measurement-side recommendation: NO CHANGE by default.** The charge
  stands per Andrew's 2026-09-29 decision (every recorded number keeps
  computing as today); `spellingIdenticalLines` already reports the 8,230
  alongside. Implementing the tier-3 head-tolerant repair would re-derive
  every recorded label — Andrew's call, with the exact number now known.
- **The CLEAN-DIFF BREAKDOWN landed (2026-10-02, `measure/clean-diff-breakdown`):
  spelling became a CATEGORY of the clean diff** — Andrew's "report both
  numbers, with a breakdown" decision. Raw keeps the frozen charge
  byte-equal; the summary's breakdown values each soft category
  (buildMetadata / spelling / remaining real) and the composition gained the
  `spellingTolerance: raw|tolerant` flag (tolerant = the labeled category
  charged directly, `raw real = tolerant real + spellingIdenticalLines`). The
  contract and the exact 207→208 breakdown live in
  `experiments/034-eval-harness/README.md` ("RAW vs CLEAN — the contract");
  this census's numbers are its validation (raw real 51,880 / spelling 8,230
  / clean real 42,426 src; 198 + 8,230 in the four files; vendor 0; the
  +9,162 rust-vs-ts realExBuild gap = 8,230 spelling + 932 genuine).
- **Claims to read carefully:** the function census's first revision keyed
  babel's "class field" binder on oxc's ESTree name (`PropertyDefinition`)
  instead of babel's (`ClassProperty`), so its population refusal counts
  ran under a slightly stricter reading. The flip population was SAFE under
  both readings (0 refusals either way) and the committed rerun is the
  corrected one; the mistake is recorded here because a census's own bugs
  are part of its record.
- Nothing here supersedes another experiment's claim; the exp092 baseline
  file itself is unchanged on this branch (a fresh corpus scorecard with
  the new binary is in `out-corpus/`, and the README of exp092 carries the
  re-record note).

## The problem, precisely

When comparing two versions, the pipeline and the measurement stack compare
code by SHAPE, never by name. But shape carries trivia: the same
bundler-generated wrapper can be spelled `(...) => { … }`
(ArrowFunctionExpression) or `function (...) { … }` (FunctionExpression) —
identical behavior, different AST node kind. At the real 2.1.207→2.1.208
hop, upstream's packaging tool re-serialized its module wrappers
(arrow → function expression), and BOTH shape-reading surfaces charged for
it:

1. **The pipeline's match key** (`hash::serialize`'s MatchKey families): the
   flip breaks the function match, so names get re-asked and churn the diff
   (exp092's lead 1 — reproduced on the committed fixture: `keepWrapper`,
   recall 2/3, "the statement twin never even proposes").
2. **The measurement's diff composition** (the 034 harness's
   `composeDiff`): the wrapper's AST TYPE flips the frozen KPI
   `statementHash`, so no pairing tier can pair the two sides and both are
   charged full mass inside `real` — the fake "+9,162 lines of real change"
   (overnight report §2; placement-independent KPIs recovered the same real
   change on both sides).

## Method — census first (LLM-free), over the frozen walk trees

Two censuses over `/work/walk-rust-0926/trees` (29 versions, 29 adjacent
hops, the same corpus the overnight report and the soft-noise validation
walked):

- **`census.ts`** (statement level): `composeDiff` per hop, per surface
  (`src` and `vendor` separately — rule 8: both surfaces enumerated), with
  the soft-noise samples kept. This is the CHARGE side: how much of `real`
  is spelling-identical, where, and to whom the flipped wrappers are passed.
- **`function-census.ts`** (function level): every
  ArrowFunctionExpression / FunctionExpression node in every common file of
  every hop, each classified by the candidate equivalence rule (its
  `cousinHash` = the node re-spelled under the canonical
  `function (params) { … }` head, plus the FIRST failing safety condition),
  then PAIRED across the versions of each file. This is the SAFETY side:
  which flips exist at the granularity the pipeline's hash actually sees,
  and — the reliability question — whether ANY refused pair is sitting in
  the population, i.e. whether the rule ever keeps a real difference apart.

## Census results

### The flip population is ONE event, uniform, and clean

| measure                               | result                                                                                                                                                                                                                  |
| ------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| hops with statement-level flips (src) | **1 of 29** — exactly 2.1.207→2.1.208                                                                                                                                                                                   |
| flips at that hop                     | **17 statements** in 4 files (the OIDC/AWS library giants: `plugin-policy-detector.js`, `create-stshttp-auth-provider.js`, `sso-token-refresh-threshold-ms.js`, `auth-token-manager.js`)                                |
| charged mass of those flips           | **8,230 lines** of the hop's 51,880 src `real` (15.9%) — and 8,230 of the hop's whole-walk total: **every spelling line lives at this one hop**                                                                         |
| direction and form                    | all 17 are arrow → function; sequence-callee heads (`var x = (0, ns.createModule)(…)`)                                                                                                                                  |
| receiver of every flipped wrapper     | **ONE callee: `getProperty.createModule`** — inspected: it receives the wrapper and only CALLS it (`moduleFactory((moduleObj = {exports:{}}).exports, moduleObj)`), never constructs it, never reflects on `.prototype` |
| vendor surface                        | **0 flips at every one of the 29 hops** (vendor real across the whole walk: 1,949 lines, none spelling)                                                                                                                 |
| genuine residue in the 4 files        | 198 lines of repackaged require-path changes — charged to `real`, correctly NOT flagged (the four files' full real charge is 8,428)                                                                                     |

### The function-level census (the pipeline's granularity)

| measure                                              | result                                                                                                                                                                                                                             |
| ---------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| \_function-level spelling cousins across all 29 hops | **17 — the same 17 wrappers, nothing else**                                                                                                                                                                                        |
| of those, passing every safety condition             | **17 of 17**                                                                                                                                                                                                                       |
| refused cousin pairs anywhere in the walk            | **0** — the reliability half of the answer: nothing real exists in the population that the rule would keep apart, and nothing real is merged either (a refused pair is how a real difference stays unmatched; here there are none) |
| population at the flip hop (common files)            | 54,321 prior / 54,540 fresh functions; refusals by reason: `conciseBody` 39,369 / 39,535, `lexicalBindingUse` 588 / 593, `namedId` 166 / 167, `generator` 17 / 17, `newTarget` 1 / 1, `newCallee` **0**                            |

`newCallee: 0` matters: no arrow in the walked corpus is ever a `new`
callee, so the one residual the hash cannot see by construction
(constructibility / `.prototype` observability of the flipped function
value) is also bounded empirically — and the census's receiver inspection
closes it for the actual flip population (a call argument to a receiver
that only calls it).

## The equivalence rule (written out precisely)

Two wrapper spellings `(params) => { body }` and
`function (params) { body }` are the SAME FUNCTION — may be hashed, paired
and compared equal — iff ALL of:

1. **The parameters are the same modulo renaming** (both spellings carry the
   same params; the paren-free single-parameter arrow form is included).
2. **No generator** — arrows cannot be generators; a `function*` never
   unifies (its `generator:true` rides in the hash bytes regardless).
3. **`async` matches** — both spellings carry `async` as a field; it is not
   a condition of the unification (it does not interact with
   `this`/`arguments`), it just must be the same, which the unified
   serialization preserves.
4. **The function has no binding `id`** — a named function expression can
   self-reference and carries `fn.name`; an arrow can reproduce neither.
5. **An arrow's body is a block** — a concise body (`a => a + 1`) is not
   re-spellable without restructuring the AST.
6. **The function's own lexical scope references no `this`, no `arguments`
   and no `new.target`** — the flip rebinds exactly these. Scope rules:
   - parameters and their default values evaluate under the flipped
     binding → in scope;
   - nested ARROW functions pass `this`/`arguments` through → in scope;
   - nested classic functions (including object/class methods and
     getters/setters), class shells, STATIC BLOCKS and CLASS FIELD
     INITIALIZERS bind their own → out of scope (behind a binder);
   - a class's `extends` clause and COMPUTED keys evaluate at definition
     time in the ENCLOSING scope → in scope (the one deliberate tightening
     over exp037's "whole class is a barrier", caught while writing the
     Rust rule: `class C extends this.Base {}` inside a flipped wrapper
     observes the flip);
   - `x.arguments` (a member property) and non-computed object/property
     KEYS are names, not binding references → never refuse; a SHORTHAND
     `{arguments}` destructure/assignment IS a reference and refuses;
   - `new.target` is a SyntaxError inside an arrow (verified while
     red-testing: the arrow twin of a `new.target`-using function cannot
     even be written), so the condition bites only on the classic side —
     kept defensively;
   - `import.meta` evaluates identically in both spellings → not a
     condition.
7. **The `.prototype` / constructibility residual** — a function expression
   object has an own `prototype` property and is constructible; an arrow
   has neither, so a flipped function whose value is `new`'d or reflected
   on (`fn.prototype`, `Reflect.ownKeys(fn)`) is observably different.
   The HASH cannot see use sites, so this residual is delegated to census,
   not rule: measured, it is empty — every corpus flip is a plain call
   argument, no walked arrow is ever a `new` callee, and
   constructor-prototype functions in practice bind own-scope `this`
   (refused by condition 6 anyway).

## Where each surface implements it

| surface                                                                                                                                                      | implements?                                            | notes                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| ------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **pipeline hash, MatchKey families** (`hash::serialize` — function MatchKey, statement-context hash, statement-align, twin gates, `factory_structural_hash`) | **YES (this branch)**                                  | `arrow_serializes_as_function`: a SAFE arrow serializes under the `FunctionExpression` token. Only the ARROW's bytes change — a safe function expression's stream was already the unified spelling (oxc gives both forms the same field set; the type token was the only difference), so the function-side conditions ride in the bytes themselves. Red/green in `hash/serialize_test.rs` (5 tests: unification, refusals, binders, Verbatim split, emission).                                                                                                                                                                                                                                                                                                                            |
| **pipeline hash, `Verbatim` policy** (IdentityKey, naming validators, vendor inherit)                                                                        | NO, deliberately                                       | Verbatim answers a SAME-RELEASE identity question ("the exact same declaration bytes"); there a spelling difference IS a difference.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| **pipeline `statement_hash`** (STATEMENT_HASH_VERSION 3 since exp094b — split inheritance, statement twins, family permute, placement ledger)                | **YES (exp094b, branch `exp094b-statement-spelling`)** | The SAME rule, the SAME shared module (`hash::wrapper_spelling`, extracted from serialize.rs so both Rust hash arms read one predicate): a SAFE arrow's node line walks under the FunctionExpression token, and the function-head fields (`async`, `generator`) ride in the statement stream's node content — that stream hashes no scalars, so without the fields the flip's function-side conditions could not hold there (and v2 hashed `function`, `async function` and `function*` as ONE class, a coarseness the mirror fixes rather than imports). Fixture: statement recall 2/3 → 3/3. The version bump re-keys every split ledger; any stale era re-derives from the prior text through the proven bijection (`place::ledger::rederive_stale_era_hashes`, widened from TS-only). |
| **measurement soft-noise detector** (`diff-composition.ts` → now `experiments/lib/js/wrapper-spelling.ts`)                                                   | YES (shared rule extracted)                            | The detector's local walk was MOVED to the shared owner so three consumers cannot drift; the exp037 pins (incl. the walk-tree 8,230) reproduce exactly, and the shared rule adds the class-field-initializer binder + the extends/computed-key tightening. Advisory `spellingIdenticalLines` semantics unchanged.                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| **match ground truth** (`match-truth/canonical.ts`)                                                                                                          | YES (soundness fix)                                    | The unconditional head erasure MANUFACTURED ground truth for a `this`/`arguments`-loaded pair — against the instrument's own "never manufactures" contract. The erasure now consults the same shared rule; a refusal may only MISS ground truth, the designed error direction.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| **measurement CHARGE** (tier-2/tier-3 of composeDiff)                                                                                                        | NO — Andrew's standing 2026-09-29 decision             | The +9,162-style RAW charge stands; the advisory field reports the mass. The CLEAN-DIFF view (2026-10-02) is a derivation, not a second charge: composeDiff's `spellingTolerance: "tolerant"` charges the flagged mass to its own labeled category, and the summary reports raw, clean and the per-category breakdown (034 README, "RAW vs CLEAN — the contract"). Making tier 3 head-tolerant would re-derive every recorded label (backlog item 12's caveat); the exact pre-derivation number is now known (below).                                                                                                                                                                                                                                                                     |

## The A/B

### Pipeline-side (the prototype): the exp092 fixture, regenerated

Regenerating the committed two-version fixture with the branch's binary
(`exp092` README's recipe):

| measure                           | main's binary     | exp094 binary (cut 1)                 | exp094b binary (cut 2)                       |
| --------------------------------- | ----------------- | ------------------------------------- | -------------------------------------------- |
| must-match functions              | 3                 | 3                                     | 3                                            |
| matched at `structuralHashUnique` | **2**             | **3** — matches at the STRICTEST tier | 3                                            |
| missed must-pairs                 | 1 (`keepWrapper`) | **0**                                 | 0                                            |
| statement twins proposed          | 2/3               | 2/3 (deliberately not yet unified)    | **3/3** — the statement arm mirrors the rule |

The negative case (the reliability proof) is pinned in the Rust tests and
in the corpus censuses: a `this`/`arguments`/`new.target`/generator/id/
concise-body flip REFUSES — the pair stays unmatched — and the walk corpus
contains **zero** such refused pairs, so on real data the rule separates
nothing real.

### Cut 2's two-arms-agree check (exp094b): the statement arm on the frozen walk

`statement-arm-check.ts` (raw data `out-stmt/`) runs main's binary and the
exp094b binary over the same hop's files, reads each dump's per-statement
twins hashes, and diffs the unique-tier hash joins — the statement twin's
own proposal join — between the two eras:

| measure                                                | flip hop 2.1.207→2.1.208 (the 4 census files)                                                                                                                                                  | quiet hops 2.1.199→200 and 2.1.215→216 (150-file stride samples each) |
| ------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------- |
| GAINED joins (merged only under the statement arm)     | **17** — exactly the census's 17 flips                                                                                                                                                         | **0** on both hops                                                    |
| every gained pair a rule-accepted flip                 | **17/17** (verified per pair: the two statements' function signatures differ by exactly N safe arrows → N plain same-async functions; every UNSAFE/generator/named occurrence count-identical) | —                                                                     |
| LOST joins (the async/generator head-field tightening) | **0**                                                                                                                                                                                          | **0**                                                                 |

So the two arms AGREE on the population: the statement arm merges exactly
the flips the rule accepts and refuses everything the MatchKey arm refuses
(the shared predicate refuses in both), and the head-field tightening —
which fixes v2 hashing `function` / `async function` / `function*` as one
class — splits no join anywhere measured.

### Ground-truth mirror: the corpus must-set is UNCHANGED by the soundness fix

Re-scoring the exp092 baseline dumps in-process under the exp094
canonicalizer versus the old unconditional erasure (`diff-must-set.ts`, the
scorer's own pairing): **zero must-pairs entered or left, all 13 packages**
(`async axios bluebird dayjs immutable jquery lodash minimist moment ms q
ramda underscore`). The hole was real (constructible in one line) but
unpopulated on the corpus — the fix is free there. (Re-scoring the recorded
baseline CARDS against today's dumps shows +2 must in lodash — that delta
PREDATES this branch: the `.runs` dumps were regenerated by the
match-instrument lane after the baseline card was recorded, and the
in-process A/B isolates this branch's change to exactly zero.)

A fresh full-corpus run with the branch's binary COMPLETED
(`out-corpus/exp094-binary-scorecard.json` + `run.log`; ~35 min — the
#68/#69 prior-side fixes cut the once-6h verb):

| measure            | recorded baseline (main @ f8b87490) | exp094 binary                                                                                                                                                                                                                                       |
| ------------------ | ----------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| must-match total   | 3,082/3,086 (recall 0.9987)         | **3,084/3,088 (recall 0.9987)**                                                                                                                                                                                                                     |
| per-package misses | dayjs 38/39, jquery 480/483         | **identical misses, identical counts** (the known leads 2-3: literal magnitude, free module-scope identifiers)                                                                                                                                      |
| statement twins    | 20/20                               | 20/20                                                                                                                                                                                                                                               |
| the only mover     | —                                   | lodash 586→588 must — the PRE-EXISTING drift (`.runs` dumps regenerated by the match-instrument lane after the baseline card was recorded; the in-process A/B above isolates this branch's contribution to exactly zero), and it is 588/588 MATCHED |

The corpus contains NO wrapper flips (all 13 pairs are plain library
builds, not bundler-wrapped), so the correct corpus reading is ZERO
REGRESSION: the matcher's recall on real packages is byte-for-count
unchanged, while the fixture (the only corpus-shaped input with a flip,
by construction) recovers 2/3 → 3/3. The scorecard re-records the exp092
baseline when this lands on main.

### Measurement-side (report-only): what a tier-3 head-tolerant repair would read

| measure (2.1.207→2.1.208, src)                            | value                                                                                                                                   |
| --------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------- |
| `real` charged at the hop                                 | 51,880                                                                                                                                  |
| `spellingIdenticalLines` (the advisory, unchanged)        | 8,230                                                                                                                                   |
| real after the hypothetical tier-3 repair                 | 43,650, all of the erasure being the 17 flips' mass                                                                                     |
| genuine charge inside the 4 flip files                    | 198 lines (repackaged require-paths) — the repair must NOT erase it, and the detector's structural-difference refusal is what keeps it  |
| rust-vs-ts realExBuild gap at this hop (overnight report) | +9,162, of which 8,230 is spelling ≈ the report's own placement-independent confirmation that both sides recovered the same real change |

Nothing REAL is erased BY MEASUREMENT either: the detector's refusal set
(any structural difference, `!0` vs `true` included) is what the repair
would reuse, and it flags exactly the 17 flips — the 198 genuine lines are
correctly left charged.

## What re-keys (and what does not)

- `FACTORY_HASH_VERSION` 3 → 4: vendor manifests persist
  `factory_structural_hash` bytes, which route through the canonical
  serializer. A stale manifest is REFUSED and re-keyed BY CONTENT (the
  exp093 mechanism, unchanged) — and the exp093 lesson applies in the
  mirror direction: a FACTORY spelled as a safe function expression keeps
  byte-identical hashes ACROSS the era, so the version gate, not the bytes,
  carries the refusal (`unpack_test.rs` pin updated with the
  `fixed:{was: 3}` note).
- No OTHER persisted artifact stores canonical-serializer hashes: the
  function fingerprints, statement contexts, twin gates and close-tier keys
  are all recomputed from the two trees each run. The frozen KPI
  `statementHash` (the harness copy in `experiments/lib/js/`) is UNTOUCHED
  by BOTH cuts — recorded labels stay comparable on the measurement side
  (the scorer computes its own hashes from the emitted trees with that
  frozen copy; it never reads the binary's statement hashes).
- **Cut 2 (exp094b) re-keys the split ledger**: `STATEMENT_HASH_VERSION`
  2 → 3. A stale-era ledger — the TS's v1, or the previous Rust era's v2 —
  is REFUSED by every hash reader and re-derived from the prior text
  through the proven bijection (`place::ledger::rederive_stale_era_hashes`,
  widened from TS-only to any recorded era; a class MERGE across the era
  fails the bijection and refuses loudly, never half-carries).
- Matching behavior changes (the point): the wrapper pair matches, so names
  carry instead of being re-asked; and (cut 2) the wrapper STATEMENT twins,
  so the split inheritance and the name votes that read it stop splitting
  at a re-packaging hop. **Decision-changing → both cuts join the batched
  gpt-oss eval queue** with the other pending lanes; the eval's judgment
  (noise down, novel/realLn unmoved, bands respected) is the final gate for
  main.

## Recommendation

**GO, in three cuts — cuts 1 and 2 landed:**

1. **Land the MatchKey unification** (cut 1 — MERGED on main with the
   batched eval still pending). It is the smallest decisive surface — one
   rule in one serializer fixes the matcher miss, the twin gates, the
   statement contexts and the factory hash at once — with the safety halves
   proven (unit-negative tests + zero refused pairs in the census +
   receiver inspection for the observability residual).
2. **Mirror the rule into `statement_hash` + bump
   `STATEMENT_HASH_VERSION` to 3** (cut 2 — DONE on
   `exp094b-statement-spelling`, 2026-10-02): it unifies the SPLIT
   inheritance and the statement twins — fixture statement recall 2/3 →
   3/3, MEASURED, red-first — at the cost of re-keying every split ledger
   (refused loudly and re-derived from prior text, the mechanism
   `place/ledger.rs` already had, widened to every stale era). Same rule,
   same shared module (`hash::wrapper_spelling`), same census backs it; the
   statement arm also carries the function-head fields (`async`,
   `generator`) in its node content, which v2 could not see at all.
3. **The measurement tier-3 head-tolerant repair (backlog item 12)** stays
   Andrew's call, now with exact numbers: it would read the 207→208 hop as
   real 43,650 instead of 51,880 and would have closed ~8,230 of the
   recorded +9,162 rust-vs-ts gap, but every recorded label re-derives.
   The advisory `spellingIdenticalLines` already carries the information
   without breaking comparability, which is why NO-GO by default is the
   recommendation. (Post-cutover backlog item 12's row points here.)

## Reproducing

```bash
# censuses (minutes; incremental, safe to re-run)
npx tsx experiments/094-wrapper-spelling/census.ts \
  /work/walk-rust-0926/trees experiments/094-wrapper-spelling/out-census
npx tsx experiments/094-wrapper-spelling/function-census.ts \
  /work/walk-rust-0926/trees experiments/094-wrapper-spelling/out-fn --surface src

# the ground-truth must-set A/B (in-process, seconds)
npx tsx experiments/094-wrapper-spelling/diff-must-set.ts \
  <092 .runs dir> experiments/092-match-ground-truth/baseline-f8b87490.json <package>

# re-scoring the recorded dumps under the new canonicalizer
npx tsx experiments/094-wrapper-spelling/rescore-dumps.ts \
  <092 .runs dir> experiments/092-match-ground-truth/baseline-f8b87490.json

# the fixture regeneration + pins (exp092 README's recipe, with the exp094b branch's binary)
cargo build --release --locked -p humanify-cli
./target/release/humanify match experiments/lib/match-truth/fixtures/new.js \
  --prior-version experiments/lib/match-truth/fixtures/prior.formatted.js \
  -o experiments/lib/match-truth/fixtures/dump.json --work-dir /tmp/match-fixture-work

# the two-arms-agree check (exp094b): the statement arm's join delta,
# old binary vs new, flip hop + sampled quiet hops over the frozen walk
npx tsx experiments/094-wrapper-spelling/statement-arm-check.ts \
  /work/walk-rust-0926/trees experiments/094-wrapper-spelling/out-stmt \
  --old <main-binary> --new ./target/release/humanify --quiet 2.1.199,2.1.215

# the full corpus with the branch's binary (~35 min on the post-#68 binary; run alone)
npx tsx experiments/lib/match-truth/run.ts --corpus <092 .corpus dir> \
  --bin ./target/release/humanify --out experiments/094-wrapper-spelling/out-corpus/scorecard.json
```

Raw census data: `out-census/` (per-hop composeDiff tallies + spell
samples), `out-fn/` (per-hop function-level cousin pairing + population
refusal counts), `out-corpus/` (the launched corpus run), `out-stmt/`
(the exp094b two-arms-agree check: the statement arm's gained/lost unique
joins under both binaries).
