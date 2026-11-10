# exp212 — rename collision: a catch-body `var` renamed onto its catch parameter

STATUS: fix on branch `exp212-rename-collision`, not merged. `cargo dev check` green (worktree). No real-LLM re-run of 2.1.89 (agreed with the coordinator: an exit 0 would prove little).

## Totals first

- **Failure**: the cold Claude Code 2.1.89 run on main `61063385e` (walk
  `/work/walk-61063385-up`) exited 1. The rename check rejected
  `runtime.js` because two bindings in one function ended up with the same
  name. The second walk's cold 2.1.89 (`/work/walk-61063385-down`, same
  binary and flags) exited 0, so it depends on the model's draw.
- **Root cause**: one missing case in the rename applier's `shadows-child`
  rule (`rename::validated`, the ONE applier every rename goes through).
- **Fix**: the binding's own initialized `var` declarator now counts as a
  write site for `shadows-child`, at the declarator's own scope.
- **Exposure**: 60 initialized `var` declarators sit directly in a catch body
  in 2.1.89's pre-rename source, and 4 in 2.1.216 (Babel census). Any version
  with this shape can hit it, but only when the model gives the catch
  parameter and the var the same name.

## What happened

The `$3077` / `$3079` in the error message are not names. They are binding
slots of the structural signature (bindings numbered in order). The output
referred to one slot where the input referred to two, so a reference had
been captured.

Aligning the identifiers of `runtime.js.original` and `runtime.js.validated`
(Babel, matched by position) finds the first capture in the filesystem
wrapper's `readSync` method. It is a function that the bundler's `using`
lowering emitted in this shape:

    try { ... } catch (e) { var err = e; var flag = 1; } finally { dispose(stack, err, flag); }

Order of events in the run's log:

1. The function's first naming batch answered `e` → `readError` (applied)
   and `err` → `error` (not applied).
2. A later batch re-asked `err` (with the parameters) and got `readError`.
3. The applier checked `err` → `readError` in the function scope. Nothing
   there binds `readError`. `err`'s only reference is in the `finally`,
   which does not pass through the catch. So `shadows-child` found nothing,
   and the rename was **applied**.
4. The output printed `catch (readError) { var readError = readError; ... }`.
   Under Annex B, that initializer assigns the catch parameter to itself and
   the hoisted var stays `undefined`, so the `finally` disposes with the
   wrong value. The signature check caught it and rejected the file.

The applier missed the declarator because it sits inside the catch scope
while the binding belongs to the function scope, and the declarator is
neither a reference nor a constant violation. Finding #15 (16-findings-queue)
fixed the mirror direction (renaming the catch parameter onto the var) by
making the capture guard read the declarator's span. This fix makes
`shadows-child` read its scope.

### The hand re-recorded verdict in `test/parity/wp31-rename.json`

One scenario out of 59 changed, and I changed it by hand because the TS
probe that recorded it was deleted at the cutover.

- **Scenario**: "x: catch params — own clause scope, shadowing checks see
  the body". Program: `var e = 0; try { f(); } catch (err) { var x = err;
  e = err; } finally { e; }`.
- **Ops 1-3** (unchanged): `err` → `e` and `err` → `x` are refused
  (`target-visible`), and `err` → `error` is applied, so the catch
  parameter is now `error`.
- **Op 4** (`x` → `error` in the program scope). **Old**: `applied: true`,
  final name of `x` = `error`, emitted `catch (error) { var error = error;
  ... }`. **New**: `applied: false`, `reason: "shadows-child"`, `x` keeps
  `x`, emitted `catch (error) { var x = error; ... }`.
- **Op 5** (unchanged): `e` → `err` is applied.

**Why the old verdict recorded the bug**: the probe froze whatever the TS
functions returned, right or wrong. The TS had the same gap (its
`wouldRenameShadowInChildScope` read only `referencePaths` and
`constantViolations`), so it applied op 4. The `emitted` string it froze is
exactly the 2.1.89 capture: `var error = error` assigns the catch parameter
to itself, and the program-scope `x` is never assigned. Nobody read that
string as a failure when #15 re-recorded this scenario, because #15 fixed
only the catch-parameter direction (op 2).

## Change

- `rename/validated/scopes.rs`: `BabelBinding::initialized_declarator_span:
  Option<Span>` becomes `initialized_declarator: Option<(Span, BScopeId)>`
  (the declarator's `path.scope`).
- `rename/validated.rs`: `would_rename_shadow_in_child_scope` also climbs
  from the declarator's scope. The #15 capture guard reads the span as
  before.
- Tests (each red without the fix, green with it):
  - `validated_test::renaming_a_catch_body_var_to_the_catch_param_name_is_rejected`:
    the smallest case, plus the uninitialized-`var` negative control, which
    stays allowed (it writes nothing).
  - `validated_test::the_2_1_89_disposal_catch_sequence_is_refused`: the
    rejected method's shape with the up walk's own sequence. The catch
    parameter goes to `readError` (applied), then the var goes to
    `readError` (now refused, `shadows-child`). The var's first answer,
    `error`, is still applied.
- `test/parity/wp31-rename.json`: the one scenario above.

## Gate

`cargo dev check` in the worktree: ALL 11 STAGES PASSED.

| stage | result |
|---|---|
| shims | PASS |
| no-private-addresses | PASS |
| rust:fmt | PASS |
| rust:clippy | PASS |
| census:clones | REVIEW (advisory; the 51 groups were already on main, none in the touched files) |
| census:concurrency | PASS |
| census:slop | PASS |
| rust:unit | PASS |
| rust:build | PASS |
| format-golden | PASS |
| e2e | PASS |

Clean checkout gate: see the report (run after the commit).

## What this does NOT prove

- I have no byte replay of the failing run (it ran without a cache). The
  link to the failure is the log's answers plus the unit test of the same
  shape, not a re-run that reproduces the exit 1.
- A fixed-binary 2.1.89 run that exits 0 would be consistent with the fix
  but would not prove it, because the unfixed binary also exits 0 on some
  draws.
- Neutrality: the fix can change output only when this exact collision is
  attempted. Such a rename used to produce an invalid file, so on any run
  that exited 0 the bytes should match. That was not measured.
- Related case NOT addressed: a `var` in an inner block renamed onto a
  `let`/`const` of an enclosing block is a SyntaxError (redeclaration). The
  output parse gate would catch it. It is not the same bug and was not seen.
