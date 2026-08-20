# 084 — BRIEF (hypothesis): alias flips are caused by an over-broad shadow scan

> Decision context (Andrew, 2026-08-20): sticky import aliases are "definitely
> worth it". This brief maps the mechanism, the fix, and the gates BEFORE code.

## Measured mechanism (2026-08-20, post-veto walk /work/exp082-walk)

Alias-form flips on the busy hop: **193 line-pairs (~386 git lines)**, in two
distinct classes:

1. **`X → srcX` flips, ~91 line-pairs — THE TARGET.** Diagnosed case:
   `src/truncate-and-clean-string.js` kept its exact path and prior alias
   `truncateAndCleanString`, yet 216 emits `srcTruncateAndCleanString`
   everywhere. Cause: a NEW importer (`get-todo-reminder-mode/
format-mcp-log-message.js`) references the imported function by its bare
   name — a reference that the emitter itself will rewrite to
   `alias.truncateAndCleanString(...)`. The shadow scan
   (`identifierNamesByFile`, src/split/cjs-emit.ts) counts EVERY identifier in
   an importing file, including these will-be-rewritten sites, so the bare
   alias reads as shadowed; the stability tier (`claimPriorAliases`) refuses;
   and because one module has one alias tree-wide, one new importer flips the
   alias in every existing importer.

2. **`srcX → X2` flips, ~94 line-pairs — NOT ADDRESSABLE.** Diagnosed case:
   `stripAnsi` — the bundler moved the function into a NEW 4-statement module
   (prior module correctly retained `strip-ansi.js` at 0.79 content overlap);
   importers follow the function to its new file, so their import lines
   change no matter what alias is chosen. This is the accepted cost of
   decision "follow the bundler" — record, do not chase.

## The fix

Sharpen the shadow scan: when collecting shadow-capable names in an importing
file, SKIP identifier nodes that are cross-file reference sites (the
referencePaths / constantViolations of bindings declared in another file) —
those are exactly the sites the emitter rewrites to `alias.member`, so they
cannot be captured by the alias after emission. Local declarations and their
references (the real shadow risk the scan exists for) are untouched.

## Gates and predictions (pre-registered)

- red/green unit tests on `cjs-emit`: a new importer referencing the bare
  imported name must NOT flip the alias; a genuine local named like the alias
  MUST still refuse it.
- `npm run check` 8/8; matcher preflight untouched (no matcher code).
- Cold walk: busy-hop `X → srcX` flip class ≈ 0 (from 91 line-pairs); the
  `srcX → X2` class unchanged; `novel`/`realLines` EXACT; calm hop inside the
  32-line spread.
- Emitted code must remain runnable (e2e stage covers the rewrite): the ONLY
  behavior change is which name the alias binds — every rewrite site uses the
  alias consistently, so a wrong choice here shows up loudly as e2e failure,
  not silently.

Ceiling: ~180 git lines on this hop, plus not re-flipping on future hops
(each flip repeats in reverse when the shadow disappears).
