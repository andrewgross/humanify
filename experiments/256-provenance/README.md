# exp256 — a label for every changed line, and less noise on real hops

STATUS: BUILT, GATED (`cargo dev check`, all 13 stages, at `2e7d6391a`),
measured on one real-LLM walk of two hops (2.1.285 -> 286 -> 287). The
run-to-run variation was measured afterwards, in exp257. Branch
`exp256-provenance`.

## Totals

Changed lines outside `.humanify/`, real-LLM walk, cold first version.
"Before" is the walk at `752422c38`; "after" is the walk at `2e7d6391a`.

| hop | walk | all lines | real (ceiling) | noise | wiring | unknown |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| 2.1.285 -> 286 | before | 46,650 | 39,459 | 4,185 | 1,340 | 1,666 |
| 2.1.285 -> 286 | after | 45,251 | 39,364 | 2,956 | 1,299 | 1,632 |
| 2.1.286 -> 287 | before | 54,010 | 45,315 | 4,953 | 1,669 | 2,073 |
| 2.1.286 -> 287 | after | 51,973 | 44,720 | 3,710 | 1,643 | 1,900 |

- Every version of both walks boots, the prompt check included, and
  every run exits 0.
- Peak memory is 19.6 GB cold and about 31 GB with a prior tree. Before
  the sweep fix a real-size run climbed to about 220 GB.
- "Real" is a ceiling: every changed line inside a changed input
  function counts as real.

## What was built

1. **The layout record** (`.humanify/layout.json`). One file per tree
   says which input bytes each output line came from, which text the
   linker wrote, and what the split needs to remember for the next
   version. It replaces the provenance file, the match-decision file and
   the ledger file. `cargo dev layout check` and e2e hold it to its tree.
2. **The diff labeller** (`cargo dev diff label`). It gives every changed
   line of `diff -rN` between two trees exactly one label, in four
   groups: real, noise, wiring, unknown. The labels must add up to the
   diff's count. With an output path it also writes every line and its
   label (`<out>.lines.tsv`). `eval score` and the walk report read it;
   the statement-hash scorer is deleted.
3. **`cargo dev match score`**: the matcher judged against the input
   comparison, before any model answer.
4. **Memory.** The leftover-name sweep kept a copy of every visible name
   per request; it now shares one list per scope. The program's JSON
   tree no longer stores each node's position twice.

## Noise removed, by cause

Noise lines in `src/` by label, before and after:

| label | 285 -> 286 | 286 -> 287 | what changed |
| --- | ---: | ---: | --- |
| moved-in-file | 1,300 -> 149 | 673 -> 284 | values that run no code leave the startup function |
| renamed | 608 -> 236 | 1,154 -> 710 | statement labels get fixed names |
| moved-file | 168 -> 116 | 332 -> 58 | same as moved-in-file |
| reshaped | 491 -> 407 | 213 -> 159 | same as moved-in-file |
| linker-renamed | 340 -> 778 | 675 -> 565 | see "Open" |
| imports-reordered | 1,268 -> 1,260 | 1,835 -> 1,853 | left as Bun wrote them |

- **Values that run no code stay outside the startup function.** The
  linker turns a lazily loaded Bun chunk into one startup function. Bun
  moves modules between chunks from version to version, so anything
  inside that function moves with them. A declaration whose value runs
  no program code and reads no program binding (a regex, a literal, a
  `new Set([...])` of literals, `Symbol(...)`, `Object.freeze(...)`, a
  class with no computed work) is now a plain top-level declaration.
  One rule (`core::inert`) answers this for the linker and for the
  in-file order step. The first version of this change used the rule in
  the linker only; the order step then treated `Symbol(...)` and the
  like as code that runs, and one moved declaration pushed thousands of
  values to the end of the file (about 26,000 moved lines per hop). The
  shared rule fixed it.
- **Statement labels are named for what they mark** (`loop`, `choice`,
  `block`, numbered when nested). They were left as the minifier wrote
  them and changed on every build.
- **Init calls that run nothing are left out**, and three in-file order
  rules keep a statement in its prior place when the load order allows.

## Decisions

- **Init calls inside a startup function stay in Bun's order.** Bun
  orders them by when each imported chunk is first evaluated. Putting
  them back in the prior order would change which module runs first,
  and no subset of them can be shown safe. This is the largest noise
  label left.
- **Statements with effects inside a startup function stay in Bun's
  order** for the same reason.

## Open

The first three items below and the missing band were closed by
[exp257](../257-stable-names/README.md).

- A late step (the per-file reconcile) gave a top-level function its
  prior name inside its own file but left the exported name and every
  caller on the new name (fixed in exp257).
- An import alias moved to a longer name when a new local variable
  took its name (fixed in exp257).
- Numbered import names (`pathModule327`) were renumbered now and then
  (fixed in exp257).
- The first, cold version takes about 50 minutes.
