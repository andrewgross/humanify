# exp219 — a lone surrogate or an infinite number no longer empties the program

STATUS: BUILT (TDD), GATED (all 11 stages pass, see Gate), ONE REAL HOP
(2.1.276 -> 2.1.277, live model, exit 0, boots both halves), BUN-DEV
FAILURES RE-RUN (4 of 4 now exit 0). No eval, no walk. Branch
`exp219-split-wrapper-span` from main `0267c9330`. No Claude Code text
is committed; the shapes below are described generically.

## Totals

| | main `0267c9330` | exp219 |
| --- | ---: | ---: |
| 2.1.276 -> 2.1.277 real hop (live model) | exit 1 after 49 s, no tree | **exit 0** after 783 s, tree written |
| that tree: `bun run.cjs --version` (Bun 1.4.1) | — | **pass** (`2.1.277 (Claude Code)`) |
| that tree: `bun run.cjs -p "say exactly: boot-ok" --model claude-opus-4-1` | — | **pass** (`boot-ok`) |
| bun-dev bundles that failed with the same error (stub runs) | 4 fail | **4 of 4 exit 0** |
| gate (`cargo dev check`, 11 stages) | green | green |

## The failure

The walk stopped at 2.1.276 -> 2.1.277 with:

```
wrapper detected (body span 60..44794110) but that block is absent from the estree json
```

The message is misleading. The wrapper was found correctly, and its
block was in the AST. The real problem was that the **whole program's
ESTree JSON failed to parse and was read as `Null`**. A search for any
block in `Null` finds nothing, so the error blamed the wrapper.

## The cause (confirmed before fixing)

1. oxc writes ESTree JSON for JavaScript's `JSON.parse`. serde_json is
   stricter in two places:
   - **A lone surrogate.** A JavaScript string can hold half of a
     surrogate pair (`"\ud800"`). That is legal JavaScript but not legal
     Unicode. oxc writes it as the escape `\ud800`. `JSON.parse` accepts
     that escape, but serde_json refuses it ("unexpected end of hex
     escape" / "lone leading surrogate").
   - **An infinite number.** A number literal past the double range
     (`1e400`) is `Infinity`. oxc writes it as `1e+400`. `JSON.parse` reads
     that as `Infinity`, but serde_json refuses it ("number out of range").
2. `ingest::parse_estree_json` answered `Null` on any parse error, and did
   so silently. `twins::parse_unbounded` was a second copy of the same
   parse. Each reader then failed on a symptom: the twins inventory said
   the wrapper's block was absent, and the graph's per-function and
   per-binding fallbacks hashed `Null`.
3. **What changed at 2.1.277:** one chunk added a tagged template, a
   `String.raw` over a surrogate range (`\ud800-\udfff`). Its *raw* text is
   plain ASCII, but its *cooked* value holds two lone surrogates, and the
   ESTree JSON carries the cooked value. 2.1.276 has no such template. Its
   other surrogate escapes are all valid pairs, and oxc writes those as
   real characters. Confirmed with a probe: the 2.1.277 tree's
   `runtime.js` (the text the failed run wrote) failed to parse at that
   template's `cooked` field, and the 2.1.276 prior parsed cleanly.
4. **It was not a span-unit mismatch.** oxc's spans and the ESTree JSON's
   `start`/`end` are both UTF-8 byte offsets. Embedded text literals were
   unrelated, as the brief suspected.
5. **The bun-dev bundles share the cause.** just-bash, chat.md, mysti and
   kryptokrona (the bun-slice bundles that exp192 and exp211 recorded with
   this error) all reproduce on main with the same message and all pass
   with the fix. All four contain lone-surrogate escapes. None contains a
   number past the double range. The `1e400` half matches finding 87's
   open note in `docs/rust-port/16-findings-queue.md` ("the split's
   ESTree substrate refuses a lone-surrogate escape or `1e400` in APP
   code").

## The fix

There is now one owner, `ingest::try_parse_estree_json` (with
`parse_estree_json` = that, or `Null`). Before handing the text to
serde_json, it rewrites oxc's JSON in one pass:

- **A lone surrogate escape inside a string** becomes U+FFFD followed by
  the code unit's four hex digits. This is oxc's own in-memory encoding
  of a lone surrogate, so each code unit stays distinct (`\ud800` and
  `\udfff` hash apart). A valid escaped pair is left alone, and so is an
  escaped backslash followed by the text `ud800`.
- **`1e+400` outside any string** (and `-1e+400`) becomes the largest
  finite double. The value is still a NUMBER, because readers classify
  literals by JSON type, and the literal's `raw` keeps the source spelling.
- The common case costs a substring scan. When the text holds neither
  shape, it is passed through borrowed and unchanged.

`twins` now uses the owner, and its private copy is deleted. When the
JSON did not parse, the twins error now says so ("the estree json did not
parse") instead of blaming the wrapper. There is a new row in
`docs/responsibility.md`.

**What this cannot change:** for an input with neither shape, the bytes
handed to serde_json are identical. Every eval pair and every walk hop up
to 2.1.276 parsed whole before this change, so neither shape was in any
of them, and their output cannot move. Inputs that do hold the shapes used
to fail, or hashed some functions and bindings as `Null`. They now read
the real code.

### Tests (red on main, green here)

- `ingest_test`: a lone-surrogate string keeps its code unit; a cooked
  lone-surrogate template parses; pairs and escaped backslashes are left
  alone; an infinite literal parses as a number, and a string holding
  `"1e+400"` stays a string.
- `twins_test`: a wrapper body holding a lone-surrogate template, a lone
  surrogate string and `1e400` is inventoried (55 fillers + 3); two
  statements that differ only in which lone surrogate they hold hash apart.

All 6 failed on main (the program read as `Null`) and pass here.

## Gate

| stage | worktree |
| --- | --- |
| shims | PASS |
| no-private-addresses | PASS |
| rust:fmt | PASS |
| rust:clippy | PASS |
| census:clones (advisory) | REVIEW (50 pre-existing groups, none in the changed files) |
| census:concurrency | PASS |
| census:slop | PASS (core erosion 32.6% -> 32.5%) |
| rust:unit | PASS |
| rust:build | PASS |
| format-golden | PASS |
| e2e | PASS (22 pairs, 176 launches equal to the golden) |

## The real hop: 2.1.276 -> 2.1.277

This is the walk's exact launch (`/work/walk-0267c933-up/runs/2.1.277.toml`:
the same library index, split on, the same endpoint, model and effort). It
used a copy of the walk's 2.1.276 tree as the prior, under
`/work/exp219-runs/hop/`, and the branch's release binary, copied there
before the gate rebuilt `target/`.

- Exit 0 in 783 s. It ran past the point where main stopped, through
  naming, the vendor bodies and the split: module markers, 1,817 markers
  covering 99.93% of the app code. The tree has 1,718 `src/` files and
  1,439 `vendor/` files.
- 4,613 LLM requests, 0 provider errors.
- Boot under Bun 1.4.1: `--version` prints `2.1.277 (Claude Code)`, and
  `-p "say exactly: boot-ok" --model claude-opus-4-1` prints `boot-ok`.
  The CLI warns that it remaps opus-4-1 to the latest Opus. The boot gate's
  pinned model still answers.

## bun-dev bundles (stub runs, `cargo dev run stub`, the same index)

| bundle | main | exp219 | vendor-precision card (exp219) |
| --- | --- | --- | --- |
| just-bash | exit 1 (this error) | exit 0 | 1,069 of 1,069 labelled bytes the right package |
| chat.md | exit 1 | exit 0 | nothing moved to `vendor/` (0 modules) |
| mysti | exit 1 | exit 0 | 115,273 of 145,031 labelled bytes the right package, 29,758 wrong package |
| kryptokrona | exit 1 | exit 0 | 161 of 161 region statements content-correct |

These four are the bun slice's known failures. The node-slice twins were
not re-run. Runs are in `/work/exp219-runs/bundev/{main,cand}/`.

## Open

1. **The vendor-body naming has its own lone-surrogate guard.** Every hop
   from 2.1.270 to 2.1.277 logs `3 left as extracted {"lone-surrogate": 3}`
   (`naming/vendor_bodies.rs`, `Skip::LoneSurrogate`). That guard is
   older than this change and is not touched here. Whether those three
   bodies can now be named is a separate question.
2. **The library index.** Index units holding a lone surrogate used to
   print from `Null`. They now print from their real code. The committed
   index (`4736dcfa…`) and the walk's index were built before this change,
   so a rebuild would differ for those units only. No form version was
   bumped. Claude Code's eval pairs never reach those units (their whole
   program parsed), so nothing on record moves.
3. The mysti card's 29,758 wrong-package bytes are about placement. Main
   never produced the card, so there is nothing to compare it against.
