# Issue: naming instability inflates cross-version diffs ~2× — the reconcile pass can't reach it

Status: **APPROACH 1 IMPLEMENTED + MEASURED** (2026-07-21, branch
`feat/single-vote-binding-inherit`, worktree `~/Development/humanify-stability`,
commit 6348f9c, unmerged). Approaches 2–4 still open. See "Approach 1 result"
below. The residual that survived exp014/016/020/021/022. The
diff-reconcile pass (`--reconcile-prior-diff`) cut lineage noise ~half
(1288→662, exp020) but it is a conservative _text_ pass and structurally cannot
touch the dominant remaining noise. On the newest, feature-dense hops the diff
is inflated roughly 2× by name/order churn rather than real change.

Written 2026-07-21 by the runner, off the completed 124-version history repo
(`unpacked-claude-code/claude-code-history.git`, v2.1.69 … v2.1.216).

## TL;DR

Across-version diffs churn on bindings that did not change. Provable example
(207→208 and 215→216): a `= null` placeholder gets a new name every hop —

```
-var cacheTimeoutHandle = null;      -var optionalFeatureK = null;
+var flushTimeout = null;            +var optionalFeature = null;
```

The value is `null`; it cannot have changed. This is pure naming noise, and it
is _everywhere_ on the hard hops (see the benchmark set). The reconcile pass
abstains on it by design ("skip, never force"). The fix has to move **upstream
from the text pass to the identity/transfer layer**.

## Why the reconcile pass does not (and cannot) catch this

`src/rename/reconcile-step.ts` → `src/rename/diff-reconcile.ts`
(`reconcileDiffNoise`) is a text-level LCS pass: it diffs the generated output
against the prior version and snaps a binding's name back to the prior **only
when a hunk is a provably clean, uniquely-aligned pure-rename pair**. Its
docstring enumerates the abstentions, and the churn we see is exactly them:

- **Big hunks are never candidates** (`maxHunkLines`). The tool-registry is a
  single ~600-char line; skipped.
- **A hunk tainted by >1 rename / an object key / a free identifier is skipped.**
  That same line carries a dozen renames plus real additions; tainted, skipped.
- **The pairing must be unambiguous.** A block of four identical `var _ = null`
  lines cannot be aligned — text can't tell whether `optionalFeatureK`→
  `optionalFeature` or →`pluginModule`. Skipped.
- **Export-involved bindings are skipped** (Babel's renamer splits the export
  accessor, creating hunks).
- **If the prior isn't ~95% similar the whole pass abstains** (aligned pairs
  would be coincidence).

The conservatism is correct — forcing a snap on an ambiguous pair renames
something to the _wrong_ binding and corrupts the output. So reconcile can only
ever clean the easy, unique renames; the ambiguous/multi-rename bulk is out of
reach for any text pass.

## Root cause

Name inheritance (the 90–94% "Cached" tier) works by matching a binding to its
prior-version counterpart via **structural hash**. Two failure modes generate
all the churn:

1. **Unmatchable ambiguous bindings.** A `var _ = null` placeholder, a
   `() => {}` noop, or a generic init has a trivial structural hash that
   **collides with every sibling of the same shape**. It cannot be uniquely
   matched, falls out of the cache, and is minted fresh — unstable run to run
   (`optionalFeatureK`→`optionalFeature`/`pluginModule`/`middleware`;
   `noop69`/`initializeApp511` ordinal soup).
2. **Close-match re-naming.** A binding that _is_ matched but "close" (structure
   changed slightly) goes through `applyCloseMatch`
   (`src/rename/prior-transfer.ts:286`), which transfers the prior name **but
   also attaches prior context for the LLM**, and the LLM re-picks a synonym —
   `asyncPromptProvider`→`promptAgentModule`,
   `draftFeedbackDescription`→`searchDescriptionModule`,
   `executeJsTool`→`jsExecutionCommand`. The binding barely changed; the name
   flipped anyway.

## Noise taxonomy (three distinct classes — a fix should name which it targets)

1. **Close-match rename churn** — matched bindings, name re-picked by the LLM.
   The `applyCloseMatch` path. Fixable by transfer policy (approach 1).
2. **Placeholder-mint churn** — unmatchable ambiguous bindings, minted fresh
   each hop. Needs a real identity signal (approach 2).
3. **Order churn (adjacent, not strictly "naming").** A byte-identical statement
   moves: `-var noop69 = () => {};` / `+var noop69 = () => {};` (same name, both
   sides) shows because a neighbor was added/removed and the var-block/list
   re-emitted in a different order. Reconcile can't touch this either. Worth
   scoping separately (stable statement ordering in the emitter), but it rides
   the same hops. On 207→208 you can see all three at once.

## Benchmark set — the hard transitions (measure a fix against these)

Ranked by filtered churn (version/sha/build-time excluded). "Calls"/"Tokens" are
the walk-log LLM cost for building that version — re-naming drives **both** the
diff churn and the token spend, so a good fix moves both.

| hop (parent→target) | filtered churn | files | LLM calls | tokens | note                                                                     |
| ------------------- | -------------- | ----- | --------- | ------ | ------------------------------------------------------------------------ |
| 207→**208**         | **180,941**    | 988   | 4,565     | 7.7M   | extreme — biggest release; all 3 noise classes visible                   |
| 209→**210**         | 134,690        | 679   | 1,898     | 3.5M   | high churn, **low** calls → more order/real, less rename — good contrast |
| 185→**186**         | 116,967        | 727   | 3,728     | 8.0M   | first 30 MB feature hop                                                  |
| 202→**203**         | 109,902        | 787   | 3,625     | 8.3M   | rename-heavy (high calls)                                                |
| 197→**198**         | 93,379         | 705   | 2,916     | 5.7M   | the approval-behavior change — semantically interesting to keep legible  |
| 215→**216**         | 77,132         | 593   | 2,363     | 4.9M   | **fully dissected below — best starting example**                        |

**Control (quiet) hops — a fix must NOT add churn here:** 213→**214**
(180 calls, 137K tokens) and 214→**215** (217 calls, 162K tokens). Almost no
real change; if a pinning fix introduces _new_ diffs on these, it is
over-firing.

### 215→216, dissected (the reference case)

- +45,730 / −32,662 across **616 modified files, 0 new files** — a moderate
  feature release (auto-mode config namespace; ~2 new registry tools; an
  OAuth-scope guard on Claude-in-Chrome) whose diff is inflated ~2× by naming.
- Provable rename noise: `optionalFeatureI/K/H/V`(=null) →
  `optionalFeature`/`pluginModule`/`middleware`/`dependencyProvider`.
- Synonym churn: `asyncPromptProvider`→`promptAgentModule`, etc.
- ~7,900 changed lines touch a minted/`Val`/ordinal token — an **undercount**,
  because the tool-registry line packs dozens of renames into one line.

## Approaches, ranked by leverage ÷ effort

1. **Bias close-matches to inherit, don't re-name (highest leverage, contained).**
   In `applyCloseMatch` (`prior-transfer.ts:286`), keep the transferred prior
   name and **suppress the LLM re-pick unless the binding's role materially
   changed** (e.g., signature/arity/callee-set delta beyond a threshold). This
   is the exp014 "stability > freshness" lever pushed further, and it directly
   kills noise class 1 on already-matched bindings with no new identity signal.
   Risk: a function whose purpose genuinely changed keeps a stale name — gate on
   a real structural-delta signal, not blanket pinning.
2. **Crack the ambiguous-binding buckets by usage context (real fix for class 2).**
   Match a `= null` slot by _where/how it is used_ — its neighbors/position in a
   stable enclosing structure (e.g. the tool-registry array), or the value later
   assigned into it — instead of its trivial declaration hash. If it re-identifies
   across versions, its name inherits and never churns. A same-hash-bucket
   cracker exists (`project_binding_cascade_identity`) but does not reach these
   forward-declared placeholder slots.
3. **Deterministic naming for un-nameable bindings.** Give noops / null slots a
   name derived from a _stable role signal_, so it is identical every run
   regardless of the LLM. Subsumed by (2) if (2) lands.
4. **(Adjacent) Stable statement ordering in the emitter** — attacks noise class 3. Separate effort; note it so a "why is the diff still big" follow-up doesn't
   re-discover it.

Start with **(1)**: contained change to one transfer step, targets the biggest
tokened hops (186/203/208 are rename-heavy), and is measurable on 215→216
immediately.

## Approach 1 result (2026-07-21)

Implemented as a **single-vote pin** in `applyPropagatedModuleBindings`
(`prior-transfer.ts`): a below-floor name pins on ONE vote iff the vote is an
EXACT-matched function's slot-resolved testimony, the name has exactly one
claimant (injectivity), and prior/new binding roles agree — content hash-equal
or slot-blind literal-preserving shingle overlap ≥0.5, with a callee-identity
veto for hash-equal twins around different functions
(`src/prior-version/binding-role.ts`). This attacks noise class 1 at the
transfer layer, upstream of the text reconcile, exactly as the analysis above
predicted — the recovered prior name was already there; the 2-vote floor was
discarding it and letting the LLM re-mint a synonym.

Single-hop rebuilds vs the `claude-code-history.git` archive (same
`--prior-version`), all metrics **down**:

| hop                 | filtered churn  | LLM calls   | LLM tokens | **module-binding LLM mints** | pins |
| ------------------- | --------------- | ----------- | ---------- | ---------------------------- | ---- |
| 215→**216**         | 77,132→72,184   | 2,363→2,161 | 4.9M→4.8M  | **1,131→593 (−48%)**         | 539  |
| 202→**203**         | 109,902→87,437  | 3,625→3,357 | 8.3M→8.1M  | **2,055→1,342 (−35%)**       | 711  |
| 185→**186**         | 116,967→112,822 | 3,728→3,604 | 8.0M→7.9M  | **2,101→1,933 (−8%)**        | 166  |
| 214→**215** (quiet) | 1,208→787       | 217→199     | 162K→154K  | 42→29                        | 13   |

Module-binding mints (fresh LLM synonyms — the direct churn source) is the true
metric; line churn undercounts because the tool-registry line packs dozens of
renames onto one physical line. `asyncPromptProvider`, `executeJsTool`,
`draftFeedbackDescription` synonym flips are eliminated.

**Invariant + wrong-pin checks:** structural invariant is fail-loud
(`checkStructuralInvariant`→`semanticFailure`→`process.exitCode=1`); 0 failures,
all splits complete on every hop. Spot-check: pins are byte-identical or differ
only in a vendor-alias binding name wrapping the same re-export (hash-equal,
correct). Injectivity/role gates block the unsafe candidates (on 216: 108
non-exact-source, 16 no-prior-role, 7 role-mismatch, 2 name-conflict). Quiet hop:
line churn **down**; a pin is invisible in its own hop's diff by construction
(215-name = 214-name), so it can only remove churn; 12/13 quiet pins fixed a
churn the archive left. The residual quiet-hop file delta vs archive is LLM
run-to-run nondeterminism (a baseline `main`-code rebuild shows it too; zero
pinned bindings appear in the newly-churning files).

**Still open (fix #2, approach 2):** the residual `var X = null` churn (49 lines
on 216) is noise class 2 — forward-declared placeholder slots with NO
exact-matched voter, so a single-vote pin cannot reach them. They need
usage-context identity (position in the enclosing tool-registry array / the value
later assigned in). ~418/539 pins on 216 are noops `() => {}` where hash-equality
is trivial and safety rests on exact-slot-testimony + injectivity rather than
content — a noop→noop mispin is behavior-neutral but worth noting.

## Measurement methodology + the invariant that must hold

A fix is good iff, on the benchmark hops, it **reduces filtered churn (and token
count)** while:

- **Preserving the pure-rename invariant** — the shipped output must stay
  byte-identical modulo binding names + the split's concat-equivalence must hold
  (`checkStructuralInvariant` / `captureSemanticBaseline` in
  `src/output-validation.ts`; the split's own reconstruct check). Structural
  corruption fails loudly.
- **Not pinning onto the wrong binding.** The invariant catches structural
  breakage but NOT a name pinned onto a semantically-different-but-same-shaped
  binding. Spot-check: for a sample of pinned bindings, the prior and new
  binding should agree on callees/usage, not just shape. This is _the_ risk of
  being more aggressive than the reconcile pass — treat a wrong-but-valid pin as
  a failure, not a win.
- **Quiet hops stay quiet** — 213→214 / 214→215 must not gain new diffs.

Metrics to report per hop: filtered churn before/after, LLM calls + tokens
before/after, invariant pass/fail, and a manual read of the 215→216 diff to
confirm the _real_ changes (auto-mode, OAuth guard, new tools) are still legible
and un-renamed neighbors are untouched.

### Commands

```bash
R=~/Development/unpacked-claude-code/claude-code-history.git
FLT=(-I 'VERSION: "2\.1\.' -I 'GIT_SHA:' -I 'BUILD_TIME:')

# filtered churn for a hop (the primary metric)
git -C "$R" diff --shortstat "${FLT[@]}" v2.1.215 v2.1.216

# read the actual noise
git -C "$R" diff "${FLT[@]}" v2.1.215 v2.1.216 | grep -E '^[-+]var [A-Za-z_$][\w$]* = null;$'

# rebuild a single hop to measure a code change (naming ~5 min, needs the LLM endpoint):
cd ~/Development/humanify
npx tsx src/index.ts \
  ~/Development/claude-code-versions/inputs/claude-code-2.1.216/binary-decompiled/src/entrypoints/index.js \
  --split --endpoint http://192.168.1.234:8000/v1 --model openai/gpt-oss-20b \
  --api-key local --reasoning-effort low -c 32 -o /tmp/rebuild-216 \
  --prior-version ~/Development/unpacked-claude-code/versions/claude-code-2.1.215/.humanify/humanified.js \
  -vv --log-file /tmp/w216.log
# then diff /tmp/rebuild-216 against the prior tree and compare churn to baseline.
```

The walk itself is a stable-split/prior-version run; the standalone command
above reproduces one hop in isolation without touching the archive.

## Pointers

- Text reconcile (what it does + limits): `src/rename/reconcile-step.ts`,
  `src/rename/diff-reconcile.ts` (`reconcileDiffNoise`, the abstention gates).
- Transfer tiers (fix 1 locus): `src/rename/prior-transfer.ts`
  (`applyCloseMatch` ~:286; exact vs close at ~:171/:340).
- Identity / structural hash (fix 2): `src/analysis/` (structural-hash,
  function-fingerprint), and the same-hash-bucket cracker referenced in memory
  `project_binding_cascade_identity`.
- Naming floor / minting (class 2 names): `src/rename/class-id-floor.ts`,
  `coverage-sweep.ts`, `sweep-step.ts`.
- Prior art / why this is residual: exp014 (stability>freshness; shadowed-slot
  collision), exp016 (convergence — this residual is named there), exp020
  (the reconcile pass), exp021/022 (naming floor + prior-aware sweep). The
  history repo `claude-code-history.git` is the standing benchmark corpus.
