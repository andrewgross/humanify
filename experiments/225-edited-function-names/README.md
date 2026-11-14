# exp225 — an edited function keeps the names of its unchanged bindings

STATUS: BUILT (TDD), GATED (all 11 stages pass), FOUR REAL HOPS (live
model): 2.1.238 -> 2.1.239 and 2.1.239 -> 2.1.240 with this branch, a
same-day control of main on 2.1.238 -> 2.1.239, and 2.1.235 -> 2.1.236 with
this branch. RESULT: -9.5% in the cause's buckets against the control on
2.1.238 -> 2.1.239, 14% fewer identifiers asked, novel/realLines exact;
flat on 2.1.235 -> 2.1.236 (no control there). Every hop exits 0. Both
boot halves pass on the branch's 2.1.239, 2.1.240 and 2.1.236 trees. No
eval, no walk. Branch
`exp225-edited-function-names` from main `d3fed6a4`. No Claude Code text is
committed; shapes are described generically.

## Totals

Noise cause #3 of the noise study: lines inside an edited top-level
statement that differ only by a binding's name. The classifier buckets are
`edited:rename:local`, `edited:rename:decl`, `edited:rename:export-name`
and `edited:rename:minified` ("my buckets" below). Classifier: the noise
study's `tree_stmt.py` (an unchanged copy that also lists every line it
puts in these buckets; it reproduces the study's numbers exactly).

| 2.1.238 -> 2.1.239 (prior: the walk's 2.1.238 tree) | walk 73d01430 | main d3fed6a4 (control, same day) | **exp225** |
| --- | ---: | ---: | ---: |
| my buckets, lines | 2,402 | 2,265 | **2,050** |
| my buckets per 100 real lines | 3.83 | 3.62 | **3.28** |
| `edited:shift` (names trading places; counted REAL) | 3,142 | 3,200 | **3,028** |
| classifier REAL | 62,641 | 62,584 | 62,407 |
| all noise per 100 real | 24.39 | 24.22 | **23.95** |
| scorer `novel` / `realLines` | 2,933 / 271,144 | 2,933 / 271,144 | **2,933 / 271,144** |
| identifiers asked of the model (naming) | 18,655 | 18,737 | **16,056** |
| naming calls | 6,279 | 6,299 | **5,842** |
| matching (unmatched / still ambiguous) | 2,894 / 1,660 | 2,894 / 1,660 | 2,894 / 1,660 |

Against the same-day control the branch removes **215 lines** from my
buckets (-9.5%) and **172** `edited:shift` lines; against the walk, 352 and
114. The classifier's REAL falls by 177 against the control, and 172 of
those are the `edited:shift` lines: parameters that had carried their
neighbour's name (a wrong carry the classifier counts as REAL because the
names exist on both sides). The rest of REAL moves by -5. The two
baselines differ by 137 lines in my buckets on their own (two cold model
runs), so the effect is about 1.5 to 2.5 times the draw spread seen here:
real on this pair, but not large. The scorer's `novel` and `realLines` are
exact. The model is asked **2,681 fewer identifiers** (-14%) and makes 457
fewer naming calls.

| 2.1.239 -> 2.1.240 (steady state: prior = the branch's own 2.1.239) | walk | **exp225** |
| --- | ---: | ---: |
| my buckets, lines | 2 | **2** |
| classifier REAL | 2,172 | 2,172 |
| all noise lines | 56 | **54** |
| scorer `novel` / `realLines` | 157 / 41,807 | **157 / 41,807** |
| scorer naming noise lines | 628 | **522** |

2.1.240 is a tiny release (2,228 changed lines in all), so this hop can
only show that nothing regresses on the branch's own prior. Nothing does.

| 2.1.235 -> 2.1.236 (prior: the walk's 2.1.235 tree; NO same-day control) | walk 73d01430 | **exp225** |
| --- | ---: | ---: |
| my buckets, lines | 2,492 | 2,555 (+63) |
| `edited:shift` (counted REAL) | 1,085 | 1,031 (-54) |
| my buckets + `edited:shift` | 3,577 | 3,586 (+9) |
| classifier REAL | 44,858 | 44,471 |
| identifiers asked of the model (naming) | 13,107 | 11,244 (-14%) |
| matching (unmatched / still ambiguous) | 2,313 / 1,352 | 2,313 / 1,352 |

**This pair does not show the drop.** Three things move it, read from the
lines:

- **A wrong carry hides in REAL.** When a parameter shift hands a name to
  the wrong parameter, the moved name usually exists on both sides, so the
  classifier calls the line `edited:shift` (names trading places, REAL).
  Correcting HALF of such a signature (the branch moved a trailing
  parameter's name back to it; the parameter before it, used only inside a
  nested closure, has no body evidence and went to the model) turns hidden
  shift lines into visible rename lines: one function here accounts for
  +44 in my buckets while its shift lines fell. Rename + shift together is
  flat (+9).
- **One giant component** (+103) re-rolled its module-level names; those
  are module bindings, not the locals this branch carries.
- **The walk is not a control.** Main has merged placement changes since
  the walk's binary (exp215 among them); on 2.1.238 -> 2.1.239 main alone
  moved `real:new`/`real:gone` by about -55 each against the walk, and
  here the statement pairing moved by -220/-216 (`edited:same` +181). The
  only clean comparison in this experiment is the same-day control on
  2.1.238 -> 2.1.239. The hop budget (4) did not allow a control here.

## What was wrong (the reasons, measured before fixing)

A changed function is "close-matched" to its prior version; the transfer
then carries names two ways: the parameters by POSITION, and the body's
locals by aligning unchanged statements (a local whose declaration sits in
an aligned statement carries its name; one seen only at aligned use sites
becomes a prompt hint). Everything else goes to the model.

To find out which step lost each noisy name, a no-LLM run of the walk's
own binary on 2.1.238 -> 2.1.239 (the stub model, `--dump-artifacts`) was
joined, identifier token by identifier token, to the walk's real 2.1.239
output, and each noise line was traced to the binding it renames
(scratch tools; about 60% of the lines could be traced uniquely):

| where the renamed binding came from | share of traced lines |
| --- | ---: |
| close-matched, asked the model, NO prior-name hint | ~31% |
| close-matched, asked the model WITH a hint, the model picked another name | ~16% |
| close-matched, carried mechanically, but the carry was wrong (parameters) | ~9% |
| not close-matched at all (unmatched or ambiguous function) | ~31% (40% of it one component the compiler rewrote) |
| close-matched, but the pair is uncorroborated (no transfers) | ~6% |
| other | ~7% |

So the dominant reason is "close-matched, but the transfer did not carry
the name" (about 56% together). Three concrete mechanisms account for most
of it:

1. **Parameter shift.** A release inserts a parameter in front. The
   positional pair hands each later parameter its neighbour's name — a
   WRONG carry, applied mechanically. The body says otherwise: an unchanged
   statement uses the fresh parameter exactly where the prior used the
   other name. One such function on this hop had three parameters, each
   carrying the name of the one before it.
2. **Edited sibling containers.** The body alignment descended into an
   edited `if`/`try`/loop only when its node type appeared ONCE among the
   unaligned statements. Two edited `if`s at the same level: neither was
   entered, so an unchanged declaration inside (its statement
   byte-identical) went to the model. Worse, a NEW binding of the same
   function was often asked first and took the prior name, so the
   unchanged binding got a different one (`fooVal`, `serverFoo`).
3. **Hints the model ignores.** A local whose declaration statement
   changed but whose definition did not (the snap test the code already
   computed and never used): the model kept the hinted name 1,008 times
   out of 1,029, and most of the 21 losses were names another binding had
   taken first.

Not addressed: functions that are not close-matched at all (matching, a
different lane of work), and hints whose definition really changed (the
model is right to decide those).

## The change

All in the close-match transfer (`crates/humanify-ag-core/src/matching/`):

- **Body evidence beats position** (`close_dump::vet_signature_by_body`).
  A positional parameter pair is dropped when the aligned body hints that
  fresh parameter to a different prior name, or hints its prior name to a
  different fresh parameter. A fresh parameter left without a pair whose
  unanimous body hint names a prior PARAMETER that nobody else takes
  inherits it (the parameter moved).
- **Similar containers pair** (`statement_align::similar_container_pairs`).
  Edited container statements of one type that are not type-unique pair by
  content: the Jaccard overlap of their slot-blind shingles, at or above
  the shingle floor (0.5), each the other's STRICT best (a tie abstains),
  and never crossing source order. The type-unique rule is unchanged, so
  this only adds descents; a wrong pair would still need
  identical-modulo-names inner statements to carry anything.
- **Snap carries** (`statement_align::compute_body_local_transfers`). A
  hint that is snap-eligible (aligned use sites AND an unchanged
  definition) is carried mechanically, unless another binding's evidence
  in the same function names the same prior name (injectivity; the
  positional-fallback test caught exactly that case).

Precision first: every carry still goes through the validated renamer
(collisions reject), and each change has a test for the case it must NOT
carry (indistinguishable siblings; a definition that changed; a contested
prior name; an unshifted signature whose positional pairs must stand).

## Checking the carries (precision)

On the no-LLM run (stub model, same prior, same input), main against the
branch:

| | main | exp225 |
| --- | ---: | ---: |
| close-match carries | 22,785 | 25,635 |
| names left to the model | 16,132 | 13,761 |
| identifiers asked | 17,369 | 14,936 |

Of the 2,441 bindings the branch now carries instead of asking, the live
walk's model had picked the SAME name for 2,342 (so those only save asks).
For the other 99 the carry differs from the model's live answer; the 10
read by hand against both trees all carry the right prior name (the model
had drifted: a chain of five shifted names in one validation block, a
`fooVal` collision, renamed-but-unchanged locals). The other 89 were not
read one by one. 38 positional carries changed (body evidence
over position) and 82 positional carries were dropped to the model; the
two read by hand (a function that gained a leading parameter) were wrong
on main and right on the branch.

Some carried prior names are themselves weak (a minifier letter the prior
tree never fixed). They are carried exactly as an anchored transfer would
carry them; the naming floor's existing carried-name rule applies to them.
The minted census on the live hop reads 95 (control 93).

## Gate

| stage | result |
| --- | --- |
| shims | PASS |
| no-private-addresses | PASS |
| rust:fmt | PASS |
| rust:clippy | PASS |
| census:clones | REVIEW (advisory; no group involves the new functions) |
| census:concurrency | PASS |
| census:slop | PASS (core erosion 32.5% -> 32.6%) |
| rust:unit | PASS |
| rust:build | PASS |
| format-golden | PASS |
| e2e | PASS (22 fixture pairs, 176 launches equal to the golden) |

Tests changed on purpose, each explained in place:

- The frozen TS alignment probe's `same_type_siblings` and `snap_eligible`
  cases now produce the branch's shape (siblings that differ in content
  pair; a snap carries). `superseded_parity_case` holds the new shapes.
- The TS test "does not pair changed same-type siblings" used siblings
  that content tells apart; it became
  `pairs_changed_same_type_siblings_by_content_never_crossed` (reordered
  siblings never cross-pair), and the truly ambiguous case is a new test.
- The processor test for finding #70 needed a block rewritten past
  recognition, so a changed block's local still reaches the model.
- `matching_helpers_pin_test`'s match-stage digest moved: on the three
  esbuild fixtures one snap hint became a transfer; nothing else in the
  stage's output moved (diffed line by line against main).

## Boot

| tree | `bun run.cjs --version` | `-p "say exactly: boot-ok" --model claude-opus-4-1` |
| --- | --- | --- |
| exp225 2.1.239 | pass (`2.1.239 (Claude Code)`) | pass (`boot-ok`) |
| exp225 2.1.240 | pass (`2.1.240 (Claude Code)`) | pass (`boot-ok`) |
| exp225 2.1.236 | pass (`2.1.236 (Claude Code)`) | pass (`boot-ok`) |

## Open items

- **Unmatched changed functions** (~31% of the traced lines, minus one
  rewritten component) are untouched: their names can only come back
  through matching.
- **One cold run per side.** The control and the walk differ by 137 lines
  in my buckets; the effect (-215 against the control) is outside that
  spread on 2.1.238 -> 2.1.239 but flat on 2.1.235 -> 2.1.236. A
  three-repeat band, or the eval (five pairs, its own bands), is the next
  instrument before anyone quotes a size.
- **Half-corrected signatures.** Body evidence moves a parameter's name
  only when that parameter is used in an aligned statement of the
  function's own body. A parameter used only inside a nested closure keeps
  its positional (possibly shifted) name, and a partial correction can
  turn hidden `edited:shift` lines into visible rename lines (the
  2.1.235 -> 2.1.236 case). A follow-up could infer the shift offset from
  the parameters that do have evidence.
- **Second-order prompt effects.** Two functions that are not
  close-matched drew a different name for one local on both branch hops
  (an `xResult` became a `yResult`), because their prompt context
  (callee names, names in use) changed when neighbours were carried. Not a
  wrong carry; it is the model answering a different prompt.
- **Overlap with exp224**: `edited:rename:minified` is mostly
  class-private members (`#a`), exp224's area. This branch moves that
  bucket only through ordinary locals.
- The traced-line join (stub dump to real output) lives in the scratchpad,
  not in the harness; `cargo dev` has no verb for "which transfer step lost
  this binding". Worth one if this cause is worked again.
