# exp178 — names mode left one app binding unsettled (the 2.1.197→198 bimodality)

STATUS: BUILT (2026-11-01), gate green, NOT merged. Branch `exp178-names-vote-settle`, cut from
main `5a48564bd`. Code commit `761f46f3f` (+ `57bae356a`, a comment-only
correction); every run below was scored on `57bae356a`. Not merged.

## Totals first

- **Cause found and proven with the trail.** The transfer settled a
  module-binding row by NAME alone (`settle_module_binding_node`). Names
  mode restores library source names inside nested scopes, so a library's
  own local can share a spelling with a minified module binding. On
  2.1.198 lru-cache's local `let dt = this.#h` (four copies) shares the
  name `dt` with the app's error-classes lazy init `var dt = __esm(...)`.
  The statement-twin tier renamed the lru-cache locals, and its bookkeeping
  then settled the module row `dt` as "prior-version-match", WITHOUT
  renaming it. A settled row is no longer a module-vote target, so its 414
  votes (400 for the prior name `initErrorUtils`) were never counted. The
  binding was left to the prior-diff reconcile, which ties on cold-drawn
  `init*` names in some runs and then hands the binding to the model.
  Off mode has no restored names, so no nested `dt` exists and the module
  vote settles it.
- **The fix is the general rule, not a special case:** a transfer settles
  a module-binding row only when it renamed THAT binding: the original name
  AND the owner scope. Three call sites (statement-twin, binding-cascade,
  same-name settle) pass the scope they renamed in.
- **Population (rule 2), 2.1.197→198 with the same prior:** 11 module rows
  in names mode were settled by a same-named nested binding's transfer
  (off mode: 2). 10 of the 11 were renamed anyway by their own
  binding-cascade match, so the wrong settle changed nothing for them.
  `dt` is the only one whose name came from the module vote, and the only
  outcome that changed. In off mode both collided rows (`MPt`, `o6`) end
  the same before and after.
- **Off mode: NEUTRAL** (warm, 2.1.85→2.1.86, vs main `5a48564bd`):
  0 files / 0 lines, cache +0 / +0, exit 0 / 0.
- **Names mode, cold, 2.1.197→198: the bimodality is gone in 3 of 3 runs.**
  Every run settles the binding by module vote to `initErrorUtils` and lands
  in the low mode (521 / 527 / 521 noise statements, 7,583–8,229 noiseLn).
  The reference landed high in 2 of 3 runs (r1, r3: 913–918 / ~22,200).
- **The 4-pair names label `exp178-names-r1` (cold, scratch):** novel 4,188
  / realLn 416,377, **byte-equal**. Total noise 2,745 / noiseLn 50,928, the
  reference's low repeat (r2: 2,740 / 49,830), not its high ones (r1/r3:
  3,123–3,137 / 64,720–65,539). mints 209, the lowest of the four (ref
  211–233). All 4 pairs exit 0, cache +0, boot OK; warm self-hop
  byte-identical with 0 cache writes; cold self-hop 8 lines (range 8–8).
- **Re-measuring the names bands is now worth doing** (they were set by the
  bimodal pair). Cost: 3 cold 4-pair repeats, about 100 minutes of
  exclusive lock each, so about 5 hours. NOT run; waiting for a go.

## How it was found

Diagnosis runs, all on 2.1.198 with the SAME prior (ref-names r1's 2.1.197
base), on a shared LLM cache (diagnosis only; rule 10 forbids a cache only
for a verdict). Data in `/work/exp178-data/`.

| run                                | mode  | fix | the binding's row before the module vote                    | outcome                                                                                                                              |
| ---------------------------------- | ----- | --- | ----------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------ |
| `offB`                             | off   | no  | pending, 436 refs routed to it                              | module-vote → `initErrorUtils`                                                                                                       |
| `namesA` / `namesD` (debug output) | names | no  | **Skipped("prior-version-match")**, 436 refs routed "other" | reconcile abstained (`initErrorUtils\|initPathCache\|initWorktreeDetection`) → coverage sweep drew `initializeAnthropicClientModule` |
| `namesE`                           | names | yes | pending, 436 refs routed to it                              | module-vote → `initErrorUtils`                                                                                                       |
| `offF`                             | off   | yes | pending                                                     | module-vote → `initErrorUtils`                                                                                                       |

What settled it (namesA's trails for every binding named `dt`): four
statement-twin renames of lru-cache's nested `let dt = this.#h`
(`queue`, `dequeueTask`, `taskList`, `pendingTaskList`) and five
exact-match renames of fflate's locals. The exact-match tier does not
touch module rows; the statement-twin tier's bookkeeping settled the
module row of the same NAME.

exp173 recorded "387 exact + 13 close votes, does not settle" without the
reason. The reason is that the row was already marked settled; the votes
were never tallied for it.

## The fix

- `TransferRun::settle_module_binding_node(scope, old_name)`: settles the
  row only when `rows.modules[i].scope == scope`. Every caller already
  passes the binding's owner scope (twin: `scope_of_binding`; cascade:
  `BindingRename::scope`, built from `scope_of_binding`).
- Test, watched red first:
  `transfer_test::a_nested_binding_sharing_a_module_bindings_name_never_settles_that_module_row`.
  It was red with `Skipped("prior-version-match")` on the nested twin
  bookkeeping.
- `docs/responsibility.md`: the transfer-tiers row states the identity
  rule.

## Gate table

| check                                                          | result                                                        |
| -------------------------------------------------------------- | ------------------------------------------------------------- |
| new unit test (red first, then green)                          | **pass**                                                      |
| `cargo test -p humanify-core`                                  | 1,540 pass, 0 fail                                            |
| `npm run check` from a clean checkout                          | GATERESULT                                                    |
| neutrality, OFF mode, warm, 2.1.85→86, vs main `5a48564bd`     | **NEUTRAL**: 0 files / 0 lines; cache +0 / **+0**; exit 0 / 0 |
| eval `exp178-names-198-c1` (197→198, names, cold, no self-hop) | exit 0, cache +0, boot OK                                     |
| eval `exp178-names-198-c2` (same)                              | exit 0, cache +0, boot OK                                     |
| eval `exp178-names-r1` (4 pairs, names, scratch, cold)         | 4/4 exit 0, cache +0, 4/4 boot OK                             |
| warm self-hop (2.1.216)                                        | **byte-identical, 0 cache writes**                            |
| cold self-hop (2.1.216)                                        | 8 lines (names range 8–8) ✓                                   |

## Results table

### 2.1.197→198, names mode, cold (scratch base, no cache)

| run                    | noise | noiseLn | novel / realLn  | mints | the binding                                       |
| ---------------------- | ----- | ------- | --------------- | ----- | ------------------------------------------------- |
| ref r1 (before)        | 918   | 22,442  | 1,261 / 136,396 | 61    | reconcile abstained → `initAnthropicClientModule` |
| ref r2 (before)        | 518   | 7,046   | 1,261 / 136,396 | 62    | reconcile-asymmetric → `initErrorUtils`           |
| ref r3 (before)        | 913   | 21,983  | 1,261 / 136,396 | 55    | reconcile abstained → `initModule`                |
| exp178-names-198-c1    | 521   | 8,034   | 1,261 / 136,396 | 51    | **module-vote → `initErrorUtils`**                |
| exp178-names-198-c2    | 527   | 7,583   | 1,261 / 136,396 | 49    | **module-vote → `initErrorUtils`**                |
| exp178-names-r1 (pair) | 521   | 8,229   | 1,261 / 136,396 | 58    | **module-vote → `initErrorUtils`**                |

### 4 pairs, names mode (leaderboard vs `ref-names-0357696e-r1..r3`, names bands)

| KPI                   | ref r1–r3 (range)           | **exp178-names-r1** | verdict                                                     |
| --------------------- | --------------------------- | ------------------- | ----------------------------------------------------------- |
| novel / realLn        | 4,188 / 416,377             | **4,188 / 416,377** | **= (bar)**                                                 |
| treeLn                | 144,651–146,767             | 144,159             | −492 vs r2 (band ±2,116)                                    |
| cleanReal             | 83,959–85,001               | 83,741              | −218 vs r3 (band ±1,042); novel/realLn are the hold columns |
| noise / noiseLn       | 2,740–3,137 / 49,830–65,539 | 2,745 / 50,928      | the low mode, every run                                     |
| reloc                 | 1,368–1,385                 | 1,402               | +17 vs r1 (band ±17; `reloc` is name-keyed)                 |
| newName               | 3,611–3,645                 | 3,646               | ~0                                                          |
| mints                 | 211–233                     | 209                 | ~0 (bar: not worse ✓)                                       |
| vendorLn              | 1,579–1,587                 | 1,593               | +6 vs r2 (band ±8)                                          |
| vendorReal            | 10,832–10,928               | 10,852              | ~0                                                          |
| app: noise / noiseLn  | 2,696–3,093 / 49,309–65,027 | 2,702 / 50,411      | low mode                                                    |
| app: novel / realLn   | 4,169 / 415,935             | 4,169 / 415,935     | =                                                           |
| vendor: vendorReal    | 10,696–10,792               | 10,716              | ~0                                                          |
| cross: treeLn (stmts) | 272 (14)                    | 272 (14)            | =                                                           |

Per pair (noise stmts / noiseLn): 85→86 1,497 / 33,560 (ref 1,493–1,507 /
33,518–33,950); 118→119 436 / 4,233 (ref 421–442 / 4,065–4,302); 197→198
521 / 8,229 (ref 518–918 / 7,046–22,442); 215→216 291 / 4,906 (ref
289–293 / 4,890–4,938). Only 197→198 had a high mode, and it is gone.

## What this does not prove (rule 8)

- Three cold runs all landing low is evidence, not a proof by count: under
  the old binary, a run landed high in about 3 of 5 recorded runs, so three
  lows by chance would be about 6%. The proof is the trail: the binding no
  longer reaches the reconcile or the model at all; the module vote names
  it in every run, as in off mode.
- The names bands in `noise-bands.json` `byMode.names` were measured with
  the bimodal pair and are now too wide. Re-measuring needs 3 cold
  4-pair repeats (about 100 minutes of exclusive lock each, about 5 hours). Not run: reported first, as asked.
- One more name-keyed lookup of module rows has the same shape and is NOT
  changed: `suggest_from_close_match_externals` maps a close-matched
  function's free names to pending module rows by name. A free name that
  resolves to an enclosing function's local of the same spelling would
  point its hint at the module row. It is a prompt hint only, never a
  rename, and changing it can move off-mode output; a follow-up can key
  it by binding.
- Only 2.1.197→198 was searched for collisions in detail. Any bundle where
  library restoration revives a short source name next to a minified
  module binding had the same exposure.

## Proposed CLAUDE.md change (for Andrew; not edited here)

In the names-mode reference paragraph (`ref-names-0357696e-r1/r2/r3`),
replace the sentences from "**Names-mode bands are WIDE: ...**" through
"...not on total noise." with:

> **Names-mode bands are WIDE: noiseLn ±15,709, noise ±397, treeLn ±2,116.**
> All of it came from one bimodal pair, 2.1.197→198: the error-classes lazy
> init (275 callers) was frozen unrenamed because a library's restored local
> of the same spelling settled its module row by name (exp178, fixed: a row
> settles only for its own binding). From exp178 on, that binding settles by
> module vote in every run and names-mode noise lands in the low mode
> (~2,745 / ~50,900); the bands still carry the old bimodality until they
> are re-measured on three cold repeats.
