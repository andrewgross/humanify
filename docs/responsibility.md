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

| question                                  | owner                                                                                                               | notes                                                                                                                             |
| ----------------------------------------- | ------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------- |
| Is this string a legal name to bind?      | `isValidRenameTarget` — `src/rename/validated-rename.ts`                                                            | syntactic identifier, not reserved, not a global builtin                                                                          |
| May this _particular_ rename happen here? | `getRenameRejection` — same file                                                                                    | the above **plus** binding exists, target not in scope, no outer capture, not a previously-free name, no child shadowing. 7 rules |
| Is this import alias free?                | `nsNameIsFree` — `src/split/cjs-emit.ts`                                                                            | delegates to `isValidRenameTarget`, then adds the two alias-specific clauses: unclaimed tree-wide, not shadowed in any importer   |
| Apply a rename                            | `attemptValidatedRename` / `attemptShadowingRename` — `validated-rename.ts`                                         | **never call `scope.rename` directly in transfer code.** One documented exception below                                           |
| Decorate a colliding name                 | `src/llm/validation.ts` (`DECORATION_WORDS`, `findWith*`)                                                           | `prior-name-snap.ts` imports `DECORATION_WORDS` rather than re-listing it — a `Result` variant once escaped a private copy        |
| Count changed lines                       | `experiments/lib/diff.ts`                                                                                           | normal `diff`, `<`/`>`, `-rN` for trees. A modified line counts **twice**                                                         |
| Walk an emitted tree                      | `jsFilesUnder` (`runnable-scaffold.ts`) for pipeline work; `treeFiles` (`experiments/lib/trees.ts`) for measurement | both skip `.humanify/`. `listJsFilesRecursive` (`src/file-utils.ts`) does **not** — see the gap below                             |
| Read a split ledger                       | `loadPriorSplitLedger` + `src/split/layout.ts` (pipeline); `readLedger` (`experiments/lib/trees.ts`)                | the ledger TYPE is `StableSplitLedger`, re-exported, never re-declared                                                            |
| Derive a bundle's statements              | `bundleStatements` (`experiments/lib/trees.ts`)                                                                     | throws on a split file. `fileStatements` is the counterpart and throws on a bundle                                                |
| Read an environment kill switch           | `envFlag` — `src/kill-switches.ts`                                                                                  | set means the literal `"1"`. A test asserts `src/` reads a switch nowhere else                                                    |
| Default a setting                         | `src/commands/default-args.ts`                                                                                      | `DEFAULT_LLM_TIMEOUT_MS`, `defaultModuleConcurrency`, and the ceiling **derived** from the lane table                             |
| Decide a statement's file                 | `PLACEMENT_TIERS` — `src/split/stable-split.ts`                                                                     | a proper registry: counters, log line and diagnostics trail all derive from the one array                                         |
| Boot an emitted tree                      | `experiments/lib/boot-gate.sh`                                                                                      | fatal when `bun` is missing. Both halves: `--version` **and** a live prompt                                                       |

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

## Still duplicated — ranked by how it would bite

1. **Three similarity constructions** feeding threshold decisions with different
   math, inputs and cutoffs: `computeShingleSet` (blurred n-grams + raw
   literals, 0.5), `computeContentShingles` (literal-preserving 4-grams, 0.5),
   `findCloseMatches` (12-dim count vector, cosine, 0.8). Nothing forces them to
   agree on a given pair.
2. **Fourteen ordered-fallback cascades, only two are registries.** Most have no
   per-stage counters and no per-item trail, which is why explaining any single
   decision has repeatedly needed offline reconstruction.
3. **`_bun-modules.json` is read by two independent implementations**
   (`unpack/adapters/bun.ts:107`, `library-detection/adapters/bun.ts:58`),
   justified in comments because the detector never sees `outputDir`.
4. **`close-match.ts:127` silently drops any fingerprint without `features`** —
   every binding fingerprint. Latent: only functions call it today.
5. **`listJsFilesRecursive` does not skip `.humanify/`**, and `env-reads` uses
   it, so pointing that command at a split output dir double-counts every read
   that also appears in the whole-bundle `humanified.js`. Reporting only.
6. **In `experiments/`**: ~24 tree walkers, three ways of reading `pairs.json`
   (only the shell paths honour `EVAL_ENDPOINT`/`EVAL_INPUTS_BASE`), and four
   cache-dir variable names.

## The thing that actually prevents recurrence

Only **one** of these is enforced: `src/kill-switches.test.ts` fails if any file
in `src/` reads a switch without going through the registry, and that guard was
verified by planting a direct read and watching it go red.

Everything else in the table is convention — there is now one obvious place, but
nothing stops a second one appearing. When adding an owner here, prefer adding
the test that makes it true over trusting the row.
