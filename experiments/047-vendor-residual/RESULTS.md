# 047 — The vendor residual: results

Read `README.md` (the brief) for what was believed going in, and this file for
what the numbers said. Where they disagree, **this file is right** — that is
measurement-pitfalls rule 9, and the brief carries a STATUS block naming which of
its own claims did not survive.

> ## Which numbers here are gate-valid, and which are not
>
> **The first gate run for this experiment was accidentally cache-pinned, and its
> `src/` numbers are not gate-valid.** `run.sh` passed `--llm-cache` on every
> invocation, and all 24,079 entries in the shared cache pre-dated the run: **not
> one new entry was written, so not one prompt reached the model.** Every naming
> decision was a replayed answer. A run through the cache masks the model's
> inherent variance, so success rates measured through it are distorted.
>
> Two consequences, kept separate throughout this file:
>
> - **The manifest/vendor result is unaffected.** Entry ordering never consults
>   the LLM — it is a pure function of the prior manifest and bundle order — so
>   6,407 → 1,627 holds under any draw. The cache-pinned run confirmed it, and
>   the cold run re-confirms it.
> - **`src/` non-regression could NOT be established that way.** "0 files
>   differing from control" was true only because both legs replayed identical
>   cached answers. Cross-session serving drift is ±2.7k lines — precisely what
>   the cache was hiding.
>
> The one thing the cache-pinned run proves that a cold run cannot: **zero new
> cache entries across all eight runs** means the change alters no naming prompt
> at all. Since the cache is keyed by canonical request content, a change that
> perturbed any prompt would have missed and written an entry. That is a stronger
> name-neutrality proof than any diff, and it is why the run is kept on the
> record — labelled, not promoted.
>
> `run.sh` now defaults to **no cache**; `EVAL_LLM_CACHE=<dir>` is an explicit
> opt-in that announces itself as not gate-valid. It also unsets
> `HUMANIFY_LLM_CACHE`, because `unified.ts` falls back to that env var and
> omitting the flag alone would have left the hole open.
>
> Sections below are marked **[cache-pinned]** or **[cold]** accordingly.

## Totals first

| task                                | verdict                                                        |
| ----------------------------------- | -------------------------------------------------------------- |
| 1 — is 197→198's 127/127 real?      | **rotation confirmed AND `vendorReal` stands** — see below     |
| 2 — manifest entry-block reordering | **SHIPPED** — manifest churn 6,393 → 1,563 cold (−75.6%)       |
| 3 — vendor filename rotation        | **closed, not worth building** — ceiling 138 ln on 1 of 4 hops |

`vendorReal` is **NOT re-baselined for Task 1 — the 197→198 lines are real.**
But the cold gate showed the KPI is **not draw-stable** (±200 between runs of
identical code, because it counts humanify's own filename rotation as dependency
change), so it cannot serve as a strict `=` guard under live LLM. See the cold
gate section.

Task 2's mechanism, one line: the manifest is now written in the **prior
release's order** rather than bundle order, and the naming tie-break that array
position used to carry moves into a `hashOrdinal` field.

Measured **cold** (no LLM cache, 16 pipeline runs, live model traffic), control
vs candidate:

| hop        | manifest, bundle order | manifest, prior order |            vendor |
| ---------- | ---------------------: | --------------------: | ----------------: |
| 85→86      |                  4,574 |                 **0** |        4,656 → 82 |
| 118→119 🐤 |                     20 |                    16 |         337 → 151 |
| 197→198    |                  1,482 |                 1,354 |     2,873 → 2,749 |
| 215→216    |                    317 |                   193 |     1,998 → 1,880 |
| **TOTAL**  |              **6,393** |             **1,563** | **9,864 → 4,862** |

Down on every hop. `vendorLn` 6,556 → 1,744 (−4,812) on the leaderboard.
**Attribute on the manifest column (−4,830)**: the vendor total carries a ±200/hop
naming-variance band, so −5,002 overstates the change's own effect.

## The cold gate [cold] — the real one

Sixteen pipeline runs with **no LLM cache**: a control leg (pre-exp047 via the
kill switch, which reverts the ordering AND the field) and a candidate leg, each
`REBASE_PRIOR=1`. Live traffic confirmed at the endpoint — 8,422 successful vLLM
requests during the first rebase alone, against **zero** for the entire
cache-pinned run.

**Manifest and vendor churn DOWN on every hop, canary included:**

| hop        | manifest C → X    |          Δ | vendor C → X      |          Δ | `src/` Δ |
| ---------- | ----------------- | ---------: | ----------------- | ---------: | -------: |
| 85→86      | 4,574 → **0**     |     −4,574 | 4,656 → 82        |     −4,574 |     −344 |
| 118→119 🐤 | 20 → 16           |         −4 | 337 → 151         |       −186 |     −110 |
| 197→198    | 1,482 → 1,354     |       −128 | 2,873 → 2,749     |       −124 |     +354 |
| 215→216    | 317 → 193         |       −124 | 1,998 → 1,880     |       −118 |     −250 |
| **TOTAL**  | **6,393 → 1,563** | **−4,830** | **9,864 → 4,862** | **−5,002** | net −350 |

Leaderboard, cold control vs cold candidate: **`vendorLn` 6,556 → 1,744
(−4,812)**, `novel` and `realLn` unmoved, `relocSt` 0.

The cold result lands within ~1% of the cache-pinned one (−4,830 vs −4,780), and
85→86's manifest reaches **exactly 0 under both conditions** — the direct
evidence that this KPI is LLM-independent, since on that hop every entry is
byte-identical and only order changed.

### Attribute on the manifest column, not the vendor total

The vendor total carries a naming-driven noise band of roughly **±200 per hop**:
cold control 9,864 against cache-pinned control 9,632 for identical code. Vendor
names shift file paths, which changes whether `vendor-body-inherit` finds a prior
body to reuse. On the canary only −4 of the −186 is the ordering change; the rest
is that band. The manifest effect (−4,830) is over 20× the band, so the
conclusion is safe — but the honest claim is "manifest churn fell by 4,830", not
"vendor fell by 5,002".

`src/` moved −344 / −110 / +354 / −250, net −350 with no direction: noise. What
carries the `src/`-neutrality claim is NOT this diff but the deterministic
offline proof (`verify-shipped-order.ts`) that the emitted entry multiset is
unchanged and no field but `hashOrdinal` differs — plus the fact that the
cache-pinned A/B wrote **zero new cache entries**, meaning no naming prompt
changed at all.

### `vendorReal` moved −196, and the KPI is at fault, not the change

`vendorReal` is a `=` column: it must not move. It did, and reading it settles
why. The −196 is almost entirely one hop:

| hop     | control `real` | candidate `real` |    Δ |
| ------- | -------------: | ---------------: | ---: |
| 85→86   |              0 |                0 |    0 |
| 118→119 |            319 |              135 | −184 |
| 197→198 |          1,579 |            1,567 |  −12 |
| 215→216 |          1,678 |            1,678 |    0 |

Both legs process the SAME two bundles, so the libraries are identical. The
added/removed vendor files differ anyway:

- **control**: removed `git-branch-cleanup.js`, `local-storage.js`; added
  `rimraf/lib_3b8af129.js`, `simple-git.js`
- **candidate**: removed `local-storage.js`; added `git-reset.js`

A git module was named `simple-git` in one run and `git-reset` in the other —
the vendor namer's LLM draw — and the classifier charges that as one library
added plus one removed, i.e. "real dependency change".

**So `vendorReal` is not draw-stable and cannot serve as a strict `=` guard under
live LLM.** For identical code it reads 3,364 cache-pinned and 3,576 cold, a
±200 band, because it counts humanify's own filename rotation as dependency
change. This is Task 1's finding — a name is not an identity — demonstrated
dynamically rather than by reading files. The cache had frozen the names, which
made the invariant look solid.

Nothing here dropped real change: an ordering permutation cannot delete a vendor
file or alter its bytes.

### Two gate criteria that the cache had been hiding

**Self-hop is NOT byte-identical cold, for the control either.** Criterion 5 was
only ever satisfiable with the cache: the control violates it by **78 lines**,
the candidate by **34**. The candidate's 34 are 17 entries whose ONLY differing
field is `nameSource` flipping `"llm"` → `"carry-over"` — the same name, arrived
at by carry-over on the second pass. Verified directly: entry ORDER, `name`, and
`hashOrdinal` sequences are all byte-identical across the self-hop, so
`orderByPriorManifest` is a proven fixed point under live LLM.

**The committed `src/` baselines are cache-pinned artifacts.** `noise` 3125,
`noiseLn` 61878, `newName` 4307, `mints` 85, `reorderLn` 6078 were produced
through the cache; cold control reads 3119 / 60125 / 4316 / 85 / 5964. A cold run
therefore establishes a new reference rather than checking against those, and a
single cold pair cannot resolve a sub-400-line `src/` effect.

### The one-off migration cost, which the rebased gate cannot show

`REBASE_PRIOR=1` regenerates the base with the current pipeline, so both sides
share a format and the KPI is the STEADY STATE. The first commit emitted into the
real history diffs against a prior commit that predates the format, and that cost
is real. Measured against the archive bases (`migration-cost.ts`):

| hop        | archive v−1 vs fresh v |
| ---------- | ---------------------: |
| 85→86      |                  2,498 |
| 118→119 🐤 |                  2,397 |
| 197→198    |                  4,246 |
| 215→216    |                  3,089 |
| **TOTAL**  |             **12,230** |

Cumulative over exp046 + exp047, since the archives still carry `factoryVar` on
every entry and no `hashOrdinal`. Direct field costs: `factoryVar` removal
−6,332 lines (one per entry, exactly), `hashOrdinal` +1,445 (one per annotated
entry). The rest is entry order plus vendor-name differences between the
archive's pipeline and this one. Paid once, on the first regenerated commit.

### A new lever this exposed: `nameSource`

Of the 1,567 manifest lines remaining, **345 (22%) are the `nameSource` field**,
which has the exact `factoryVar` signature — it churns for reasons unrelated to
code change (`"llm"` on first sight, `"carry-over"` once the lineage has it), and
`bun-module-classification.ts:162` already documents that "a nameSource test
silently flips". Its live uses are on the in-memory record, not the written
manifest. Sizing it properly is the next cheap win.

| hop        | manifest churn | without `nameSource` | attributable |
| ---------- | -------------: | -------------------: | -----------: |
| 85→86      |              1 |                    1 |            0 |
| 118→119 🐤 |             17 |                   13 |            4 |
| 197→198    |          1,355 |                1,043 |          312 |
| 215→216    |            194 |                  165 |           29 |
| **TOTAL**  |      **1,567** |            **1,222** |      **345** |

## Task 1 — 197→198's added/removed files (the gating question)

exp046 published this hop's 127 added / 127 removed as real dependency change
without proving it, and flagged it as the one uncertain number in its ledger,
worth ~1,285 of the 3,364 `vendorReal` lines. Both of the brief's readings turned
out to be partly right, and they are separable:

**Rotation is real, and at scale.** Matching the 133 path-keyed added/removed
files by the identity the FILE declares — highlight.js grammars carry
`name:"ARM Assembly", aliases:["arm"]`, written by the library author and
untouched by minification — pairs **119 of 133**:

| metric                                      |       files |
| ------------------------------------------- | ----------: |
| vendor files, prior / fresh                 | 1623 / 1623 |
| removed / added, path-keyed                 |   133 / 133 |
| **same declared library identity, renamed** |     **119** |
| no counterpart by declared identity         |          14 |

humanify's vendor filenames do not identify the library at all in this
population. Read by hand: `highlight.js-php.js` holds the **SQL** grammar,
`ruby.js` holds **Scilab**, `handlebars.js` holds **Crystal**, `fsharp.js` holds
the **Python console** grammar, and `arm.js` → `armasm.js` is the same file
declaring `name:"ARM Assembly"` on both sides. Several of the 14 "unmatched" are
the same module too, with the identity STRING itself edited — `Microtik
RouterOS script` → `MikroTik RouterOS script` (a typo fix upstream) and
`Javascript` → `JavaScript`. So exp046's suspicion was correct: this is not 127
libraries leaving and 127 arriving.

**But the content genuinely changed, so the lines are real.** Applying the
SHIPPED decision function — `vendor-body-inherit`'s skeleton pre-filter plus
`computeStructuralSignature`, literal-PRESERVING — to the 119 pairs:

| class                       |   pairs | lines charged |
| --------------------------- | ------: | ------------: |
| same program, same depth    |       0 |             0 |
| same program, depth changed |       6 |            24 |
| **different program**       | **113** |       **458** |

113 of 119 are a different program. Three independent confirmations that this is
a genuine highlight.js major-version bump, not a naming artefact:

1. **The version string.** `"11.11.1"` appears in 198's vendor tree and nowhere
   in 197's, whose highest is `10.7.0`.
2. **The v11 scope migration, population-wide.** `scope:` 23 → **254**;
   `className:` 1188 → 1081.
3. **The release notes match the unmatched list file-for-file.** v11 removed the
   `php3,php4,php5,php6,php7,php8` aliases — and the largest unmatched removed
   file is `powershell/lib_a0e3a818.js`, `id=alias:php3,php4,php5,php6,php7,php8`.
   v11 also moved keyword lists from space-separated strings to arrays, which is
   why the GML pair scores 0.00 on literal overlap while declaring the same
   name: 197 has `keyword:"…"` as 5 giant strings, 198 has 3,006 individual ones.

**Conclusion: `vendorReal` = 3,364 is correct and is not re-baselined.** exp046
reached the right classification by the wrong reasoning, which is worth recording
precisely because the number did not change — the ledger entry "127 libraries
added and removed" should read "~127 of the SAME libraries upgraded 10.x →
11.11.1, their humanify filenames rotating as a consequence."

### Why the caveat about 3,225 vs 3,364 dissolves

The brief flagged that the two figures "use different units for added/removed
files". Measured on this hop: the 133 removed files total 271 lines and the 133
added files total 784 lines — 1,055 — against a bodies+files column of 1,157, the
~102 difference being in-place body edits to files that kept their names. The
units reconcile; there was no double count.

## Task 2 — manifest entry-block reordering (shipped)

6,407 lines, 67% of the post-046 vendor residual, for a file where 85→86's
entries are byte-identical on both sides and **only the order changed**.

### The brief's proposal was measured and it is WORSE than doing nothing

This is the finding that matters most here, and it kills the option as written.

The brief said: add the bundle index as a field, sort entries by a stable key,
and have `loadPriorVendorNames` read the field instead of inferring order from
the array. Both halves of that fail, separately.

**Sorting fails the per-hop gate.** Every content-derived sort key takes 85→86 to
0 and regresses two other hops:

| sort key                            | 85→86 | 118→119 🐤 | 197→198 | 215→216 | worst hop |
| ----------------------------------- | ----: | ---------: | ------: | ------: | --------: |
| _as shipped (bundle order)_         | 4,574 |         20 |   1,498 |     315 |         — |
| `structuralHash, fileName`          |     0 |         28 |   1,994 |     203 |      +496 |
| `fileName`                          |     0 |         28 |   1,626 |     197 |      +128 |
| `name, fileName`                    |     0 |         28 |   1,626 |     197 |      +128 |
| `dirname(fileName), name, fileName` |     0 |         28 |   1,706 |     203 |      +208 |

The mechanism: **when an entry's sort key changes, sorting relocates it**, turning
what bundle order charged as an in-place edit into a delete at one position plus
an add at another. Bundle order keeps a content change local; a sort scatters it.
That is why the canary — 2 changed entries, nothing else — goes 20 → 28 under
every key.

**The `bundleIndex` field costs more than it saves.** A bundle index records the
churn it is meant to make recoverable, which is precisely the `factoryVar`
pathology exp046 deleted:

| variant                               | 85→86 | 118→119 🐤 | 197→198 | 215→216 |     TOTAL | worst hop |
| ------------------------------------- | ----: | ---------: | ------: | ------: | --------: | --------: |
| _as shipped_                          | 4,574 |         20 |   1,498 |     315 |     6,407 |         — |
| prior order, no field _(unsafe)_      |     0 |         20 |   1,418 |     189 |     1,627 |  all down |
| prior order + `hashOrdinal` on ambig. |     0 |         20 |   1,418 |     189 | **1,627** |  all down |
| prior order + `bundleIndex` on ambig. |   470 |         20 |   1,638 |     455 |     2,583 |      +140 |
| prior order + `bundleIndex` on ALL    | 2,264 |         20 |   2,860 |   1,912 | **7,056** |    +1,597 |

**`bundleIndex` on every entry comes to 7,056 against a 6,407 baseline** — the
brief's proposal, built as written, would have made vendor churn worse while
appearing to fix the thing it named. 1,132 of 1,592 entries land at a new index
on 85→86, so the field changes on 71% of entries on a hop where nothing else did.

### What shipped instead

Two changes, both in `src/unpack/manifest-order.ts`:

1. **Prior-release order, not a sort.** The brief's framing was the right one —
   "position must be RECOVERABLE, not necessarily bundle order" — and the
   recoverable position that minimises the diff is the position the entry held in
   the prior release. Correspondence is found in three passes: `structuralHash`
   (~99% of the tree, unchanged libraries), then `name` (content changed, so hash
   and `lib_<hash>` filename both rotated but the carried-over name held), then
   **positional pairing of the leftovers** — whatever is unmatched on each side is
   by construction the set of entries that changed this release, so pairing those
   two lists in order returns a changed entry to its own prior slot. That third
   pass is what closes the last +10 lines on 197→198; without it the hop
   regresses and the gate fails.

   An entry matching nothing at all trails the last anchored entry before it in
   bundle order. Appending those to the tail instead was measured at **+494 on
   197→198 and +8 on the canary** — relocating an entry is never cheaper than
   leaving it beside the entries it shipped with.

2. **`hashOrdinal`, not `bundleIndex`.** `priorNameFor` does not actually index
   with the bundle index; it indexes with the entry's ordinal **within its
   `structuralHash` group**. That ordinal survives a global reshuffle untouched,
   so it costs exactly zero — identical to the no-field ideal — and it is written
   only for the 19–26% of entries whose group has two or more members. For a
   singleton the ordinal is always 0 and carries nothing.

### The tie-break risk was real, and larger than the brief guessed

The brief asked how many ≥2-member same-hash groups exist, and offered "if the
answer is a handful, the risk is small." It is not a handful. Counting groups
whose members **disagree about `name`** — the ones a position-blind change would
actually misname:

| hop        | entries | in ≥2-member hash groups | groups with disagreeing names |
| ---------- | ------: | -----------------------: | ----------------------------: |
| 85→86      |   1,592 |              302 (19.0%) |                            72 |
| 118→119 🐤 |   1,493 |              296 (19.8%) |                            68 |
| 197→198    |   1,623 |              423 (26.1%) |                           129 |
| 215→216    |   1,647 |              424 (25.7%) |                           145 |

So exp046's "do not sort the manifest" caution was **correct on its merits**, and
the reason it gave was the right reason. What was a hypothesis is the claim that
the constraint forces bundle order; it forces _recoverability_, which is a
weaker and satisfiable thing.

### Verification before the eval [cache-pinned + deterministic]

- **`src/` untouched by construction, not by hope.** `orderByPriorManifest`
  returns the very objects it was handed, and `verify-shipped-order.ts` asserts
  on all four hops that the emitted entry multiset is unchanged and that **no
  field but `hashOrdinal` differs**. This is the exp044 blast radius: a vendor
  name change rewrites `src/` require paths. This check is offline and
  LLM-independent, so it is gate-valid regardless of caching — unlike the
  tree-level "0 files differ" comparison, which was cache-pinned.
- **The shipped code reproduces the probes exactly** — 0 / 20 / 1,418 / 189, the
  same four numbers the offline ceiling predicted. A probe that measures
  something the pipeline will not do is pitfall rule 4, and it is cheap to rule
  out before an hour of eval.
- **Back-compat on real trees, not assumed.** All four control trees predate the
  field. `loadPriorVendorNames` returns a byte-identical result on each
  (1,406 / 1,374 / 1,374 / 1,397 groups) against a verbatim copy of the
  pre-exp047 reader. Entries without the field keep array order, and in a legacy
  manifest array order IS bundle order.
- **Self-hop:** ordering a manifest against itself is a fixed point.
- Kill switch: `HUMANIFY_NO_MANIFEST_PRIOR_ORDER=1`.

## Task 3 — filename rotation: CLOSED, and why

The brief gated task 3 on task 1 showing "rotation at scale". It does, by file
count — and the lever is still not worth building. Two measurements kill it.

**1. Stabilising the filename recovers 138 lines, not 1,285.** For each matched
pair, what `diff` charges now (full delete + full add) versus what it would
charge if the fresh bytes had been written to the prior path (in-place edit):

| accounting                      |   lines |
| ------------------------------- | ------: |
| charged now                     |     482 |
| if the filename had been stable |     344 |
| **recoverable**                 | **138** |

The saving is exactly 2 lines on 69 of 119 pairs and zero on the other 50,
because a vendor file is 2 lines: line 1 is the
`require("../.humanify/__bun-runtime.js")` header, which is identical when the
path depth matches, so an in-place edit charges only line 2. Where the move also
changes depth, both lines change and the saving is zero. 138 of this hop's 2,655
residual lines is **5.2%**.

**2. Rotation happens on one hop of four, because it is a CONSEQUENCE of real
change.** Removed files per hop: **0 / 2 / 3 / 133**.

| hop        | removed | added | same-identity renames |
| ---------- | ------: | ----: | --------------------: |
| 85→86      |       0 |     0 |                     0 |
| 118→119 🐤 |       2 |     2 |                     0 |
| 197→198    |     133 |   133 |                   119 |
| 215→216    |       3 |    26 |                     0 |

The mechanism is the reason: a vendor file's fallback name is
`lib_<structuralHash8>` (`bun-module-classification.ts:154`) and the vendor
namer's guess is derived from content, so **a filename can only rotate when the
content changed**. On a hop with no dependency change there is nothing to rotate,
which is what the 0 / 2 / 3 column says. Rotation is not an independent noise
source that a content anchor could remove; it is a shadow cast by real change.

This is the exp041–043 machinery's target — a name is not an identity — pointed
at a population where it has nothing to earn. Recorded as a negative result so
nobody re-runs it: **do not build content-keyed vendor filenames.** If the
reviewer experience of a rotated filename is worth fixing, it is worth fixing as
a _rename hint_ to git, not as a churn lever.

## A third unscored surface DOES exist: `index.js` (rule 8, again)

The brief enumerated `index.js` at 2,067 lines and filed it under "small, task
3". It is not scored by anything, and it is the largest single reviewer-facing
number left on this axis. `run.sh` hands the scorer `$OUT/src`/`$PRIOR_SRC` and
`$OUT/vendor`/`$PRIOR_VENDOR` and nothing else; `index.js` sits at the TREE ROOT
beside both, so it is invisible to every KPI in the harness — the same shape as
the `vendor/` omission exp046 found, one directory level up.

| hop        | index.js churn | file size | share of the file |
| ---------- | -------------: | --------: | ----------------: |
| 85→86      |      **1,311** |     1,533 |         **85.5%** |
| 118→119 🐤 |             94 |     1,532 |              6.1% |
| 197→198    |            310 |     1,510 |             20.5% |
| 215→216    |            352 |     1,502 |             23.4% |
| **TOTAL**  |      **2,067** |           |                   |

85→86 rewrites 85% of the file on a hop where every `vendor/` entry is
byte-identical and `src/` is at its floor. Reading the diff, it is pure
reordering of identical `require()` lines — `map-entry-manager.js` and
`string-mapper.js` move, nothing else — the same signature as the manifest churn
this experiment just removed.

**But do NOT reach for the same fix.** `index.js` opens with "loads every module
in the original bundle's first-statement order", and that order is SEMANTIC: it
is the runnable tree's module initialisation order, and the boot gate depends on
it. The manifest was inert metadata that no consumer read positionally once
`hashOrdinal` existed; this is executable load order. exp045 measured the
analogous `src/` axis at ~80% genuine load-order constraint, recovering ~844 of
6,078 lines — so the honest expectation here is a fraction of 2,067, not 2,067.

Sizing it means separating the reorderings that a dependency edge forces from
those that are free, with `bundleLoadOrderFacts` (the shipped model — exp045's
task A published a wrong bound by approximating it, pitfall rule 4). That is the
next lever on this axis, and it is a brief, not a result.
