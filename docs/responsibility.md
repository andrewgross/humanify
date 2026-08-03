# Who owns which question

One owner per question. This exists because the expensive bugs in this codebase
are not "two functions look alike" — they are **two functions that answer the
same question differently, where nothing declares the difference.**

The worked example: `buildFullFingerprint` populates `features` and `memberKey`;
`buildBindingFullFingerprint` populates neither. Both feed the same matching
cascade, so `singletonContradicts` — the only guard before a zero-corroboration
match — was structurally dead on the binding path. 11,094 accepts, 0 examined,
reported as `singletonRejected: 0`, which reads exactly like perfect precision.

Every row below was checked against the code, not inferred from a name.

## The table

| question                                               | owner                                                                                                               | notes                                                                                                                                                                       |
| ------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Is this string a legal name to bind?                   | `isValidRenameTarget` — `src/rename/validated-rename.ts`                                                            | syntactic identifier, not reserved, not a global builtin                                                                                                                    |
| May this _particular_ rename happen here?              | `getRenameRejection` — same file                                                                                    | the above **plus** binding exists, target not in scope, no outer capture, not a previously-free name, no child shadowing. 7 rules                                           |
| Is this import alias free?                             | `nsNameIsFree` — `src/split/cjs-emit.ts`                                                                            | delegates to `isValidRenameTarget`, then adds the two alias-specific clauses: unclaimed tree-wide, not shadowed in any importer                                             |
| Apply a rename                                         | `attemptValidatedRename` / `attemptShadowingRename` — `validated-rename.ts`                                         | **never call `scope.rename` directly in transfer code.** One documented exception below                                                                                     |
| Which names has an applied rename bound in this block? | `renameClaims` ledger — `validated-rename.ts`                                                                       | keyed by BLOCK NODE, not Scope object, because the pipeline runs two scope trees over one AST and a `bindings` map is per-tree. Additive only: it can reject, never approve |
| Decorate a colliding name                              | `src/llm/validation.ts` (`DECORATION_WORDS`, `findWith*`)                                                           | `prior-name-snap.ts` imports `DECORATION_WORDS` rather than re-listing it — a `Result` variant once escaped a private copy                                                  |
| Count changed lines                                    | `experiments/lib/diff.ts`                                                                                           | normal `diff`, `<`/`>`, `-rN` for trees. A modified line counts **twice**                                                                                                   |
| Walk an emitted tree                                   | `jsFilesUnder` (`runnable-scaffold.ts`) for pipeline work; `treeFiles` (`experiments/lib/trees.ts`) for measurement | both skip `.humanify/`. `listJsFilesRecursive` (`src/file-utils.ts`) does **not** — see the gap below                                                                       |
| Read a split ledger                                    | `loadPriorSplitLedger` + `src/split/layout.ts` (pipeline); `readLedger` (`experiments/lib/trees.ts`)                | the ledger TYPE is `StableSplitLedger`, re-exported, never re-declared                                                                                                      |
| Derive a bundle's statements                           | `bundleStatements` (`experiments/lib/trees.ts`)                                                                     | throws on a split file. `fileStatements` is the counterpart and throws on a bundle                                                                                          |
| Read an environment kill switch                        | `envFlag` — `src/kill-switches.ts`                                                                                  | set means the literal `"1"`. A test asserts `src/` reads a switch nowhere else                                                                                              |
| Default a setting                                      | `src/commands/default-args.ts`                                                                                      | `DEFAULT_LLM_TIMEOUT_MS`, `defaultModuleConcurrency`, and the ceiling **derived** from the lane table                                                                       |
| Decide a statement's file                              | `PLACEMENT_TIERS` — `src/split/stable-split.ts`                                                                     | a proper registry: counters, log line and diagnostics trail all derive from the one array                                                                                   |
| Boot an emitted tree                                   | `experiments/lib/boot-gate.sh`                                                                                      | fatal when `bun` is missing. Both halves: `--version` **and** a live prompt                                                                                                 |
| Canonicalise a relational fingerprint array            | the two builders, at construction (`buildFullFingerprint`, `buildBindingFullFingerprint`)                           | `arraysEqual` compares **positionally**, so the sort is load-bearing, not tidiness. Tested on both builders                                                                 |
| Run the pipeline for an eval pair                      | `experiments/lib/run-pipeline.ts`                                                                                   | the ONLY place that spawns it for a scored run. Records exit code, wall time, peak RSS, cache writes, switches, prior kind                                                  |
| Say what happened in a run                             | `experiments/lib/run-manifest.ts` (`<version>-run.json`)                                                            | `manifestWarnings` is a table of the dangerous combinations, each citing the incident that earned it; silent on a clean cold run                                            |
| Say whether a run was VALID                            | `experiments/lib/invariants.ts` (`<version>-run-status.json`)                                                       | the pipeline's own exit code. Absent ≠ passing: pre-existing result sets report UNKNOWN                                                                                     |
| Tell a scorecard from its siblings                     | `isScorecardShape` — `experiments/lib/run-manifest.ts`                                                              | tests `churn`, the field the CONSUMER reads — not `pair`, which the manifest also has. Filename checks broke twice                                                          |
| Explain a pure-rename invariant failure                | `describeStructuralDivergence` — `src/output-validation.ts`                                                         | re-parses the original **only on failure** and names the first diverging token; a stream per file would not fit a 14 MB bundle                                              |

## Known exceptions, deliberately not unified

- **`applyTwinPrivateRenames`** (`prior-transfer.ts:829`) writes `node.id.name`
  directly for class-private `#foo` names. No validated path exists for them,
  and the `#` namespace is disjoint — it cannot hit a reserved word or a global.
- **`JS_BUILTINS`** (`src/split/emitter.ts:13`) is a third, smaller global list
  and is labelled as such. It answers a different question — "is this free
  reference ambient?" — not "may a rename shadow this?".
- **`preserveLiterals`** splits `hashAndMapPath`'s callers into literal-blurred
  and literal-exact. Intentional and explained at `statement-align.ts:265`:
  the length-normalised hash would treat `case "open"` and `case "data"` as
  equal.
- **The split re-parses the bundle** rather than reusing the rename stage's AST,
  and `runSplit` drops `renameResult.ast` first — to avoid holding two
  full-bundle ASTs at once (`unified.ts:696`, `:828`). Not duplication.
- **`knip` cannot audit `experiments/lib`** — its callers are the ignored
  experiment scripts, so un-ignoring it reports the whole library dead. See
  `experiments/lib/README.md`.
- **The three similarity measures are three different questions**, not one
  question answered three ways, so they are not merged. Naming them here is the
  point — the risk was never that they disagree, it is that a future reader
  assumes they are interchangeable and tunes the wrong cutoff:

  | measure                                         | consumes                                                          | math    | cutoff                                                   | asks                                                   |
  | ----------------------------------------------- | ----------------------------------------------------------------- | ------- | -------------------------------------------------------- | ------------------------------------------------------ |
  | `findCloseMatches` (`close-match.ts`)           | 12 **counts** (arity, loops, literals…) — no identity of anything | cosine  | `0.8`                                                    | which unmatched pairs are worth looking at at all?     |
  | `computeShingleSet` (`function-fingerprint.ts`) | blurred callee edges + external calls, property accesses, strings | Jaccard | `0.5` (`CLOSE_MATCH_SHINGLE_FLOOR`, `SHINGLE_THRESHOLD`) | does this pair share rename-invariant CONTENT?         |
  | `computeContentShingles` (`binding-role.ts`)    | literal-preserving, slot-blind 4-grams of a serialized path       | Jaccard | `0.5` (`SINGLE_VOTE_CONTENT_FLOOR`)                      | is this binding playing the same ROLE across versions? |

  The first two are **composed in series**, which is the part worth knowing:
  `buildCloseMatchContext` generates candidates with cosine ≥ 0.8, then gates
  every candidate's name transfers on statement alignment **or** shingle
  Jaccard ≥ 0.5 (`prior-version.ts`, `findCloseMatches` → `shinglesCorroborate`).
  Cosine on counts alone cannot tell two same-shaped functions apart, so
  loosening the 0.8 without touching the 0.5 widens what is _proposed_ but not
  what is _accepted_; loosening the 0.5 is what would let a shape coincidence
  ship a wrong name as continuity. The third measure is not in this path at all
  — it serves single-vote name pinning.

## Still duplicated — ranked by how it would bite

1. **Fourteen ordered-fallback cascades, only two are registries.** Most have no
   per-stage counters and no per-item trail, which is why explaining any single
   decision has repeatedly needed offline reconstruction.
2. **`_bun-modules.json` is read by two independent implementations**
   (`unpack/adapters/bun.ts:107`, `library-detection/adapters/bun.ts:58`),
   justified in comments because the detector never sees `outputDir`.
3. **`listJsFilesRecursive` does not skip `.humanify/`**, and `env-reads` uses
   it, so pointing that command at a split output dir double-counts every read
   that also appears in the whole-bundle `humanified.js`. Reporting only.
4. **In `experiments/`**: ~24 tree walkers, three ways of reading `pairs.json`
   (only the shell paths honour `EVAL_ENDPOINT`/`EVAL_INPUTS_BASE`), and four
   cache-dir variable names.

## Fixed since this file was written

- **`close-match.ts` silently dropping featureless fingerprints.** It still
  skips them — a count vector needs `features` — but `findCloseMatches` now
  returns `skippedOld`/`skippedNew`, so an empty result can be told apart from
  an ineligible input, and the live call site logs a non-zero count. This was
  the same shape as the dead `singletonContradicts` guard: every fingerprint
  from `buildBindingFullFingerprint` lacks `features`, so the first caller to
  pass binding ids would have got a silent no-op reading as "nothing is close".

## The thing that actually prevents recurrence

Two of these are enforced by a test rather than by convention:

- `src/kill-switches.test.ts` fails if any file in `src/` reads a switch without
  going through the registry.
- `fingerprint-index.test.ts` fails if either builder stops sorting a relational
  array, on the function **and** the binding path.

Both were verified by planting the defect and watching them go red — and the
binding half of the sort check **passed against a planted break** on its first
draft, because the fixture gave no binding two callees to compare. It now
asserts it examined something. A guard that examines nothing is the exact
failure this file exists to document, and writing one while documenting it is
how easy it is.

Everything else in the table is convention — there is now one obvious place, but
nothing stops a second one appearing. When adding an owner here, prefer adding
the test that makes it true over trusting the row.
