# exp258 — fewer init calls, and a measure that separates kept code

STATUS: BUILT, GATED (`cargo dev check`, all 13 stages), measured on
two real-LLM walks of two hops (2.1.285 -> 286 -> 287) against the last
walk of exp257. Branch `exp258-work`. Follows exp257.

## Totals

Changed lines outside `.humanify/`, real-LLM walk, cold first version.
"Before" is walk 13 (the end of exp257). Walk 14 has the init-call
change; walk 15 also has the alias change. All three are labelled by
the labeller as this experiment leaves it.

| hop | walk | all lines | real | kept | noise | wiring | unknown |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 2.1.285 -> 286 | before | 43,185 | 34,818 | 3,432 | 1,702 | 2,026 | 1,207 |
| 2.1.285 -> 286 | walk 14 | 42,421 | 34,783 | 3,531 | 1,144 | 1,766 | 1,197 |
| 2.1.285 -> 286 | walk 15 | 42,231 | 34,825 | 3,433 | 1,024 | 1,766 | 1,183 |
| 2.1.286 -> 287 | before | 49,876 | 40,782 | 2,266 | 2,431 | 2,586 | 1,811 |
| 2.1.286 -> 287 | walk 14 | 47,933 | 40,734 | 2,236 | 1,410 | 2,070 | 1,483 |
| 2.1.286 -> 287 | walk 15 | 47,935 | 40,729 | 2,254 | 1,392 | 2,070 | 1,490 |

- Every version of all three walks boots, the prompt check included,
  and every run exits 0.
- Two walks of one pipeline differ by up to about 500 noise lines
  (exp257). The fall from "before" to walk 14 is in the labels the
  init-call change was built for and agrees with a stub run of both
  pipelines over the same three versions. The difference between walks
  14 and 15 is inside that band.

## What was built

1. **Only a chunk on a load cycle keeps every init call.** A lazy
   chunk's startup function calls the init of each chunk it imports. A
   call does nothing when an earlier call in the list has already run
   that chunk, and Bun moves such calls from build to build. The linker
   already left them out, except for about 410 chunks on or between two
   cycles of synchronous loads, where it kept every call. About 350 of
   those are only between two cycles, and for them the calls are left
   out too. Reordered init calls fell from 1,266 to 656 and from 1,901
   to 895 lines. Require lines and other linker lines fell with them,
   because fewer calls are written at all.
2. **The labeller has a fifth group, kept.** A changed-input line that
   the other side also has, under other names in the same hunk (paired
   in order) or in another hunk, is labelled `kept-renamed` or
   `kept-moved`. A line at another indentation stays real under its own
   label, `input-reindented`.
3. **The labeller pairs init calls by the chunk they call.** It used to
   pair any two init calls of one input file by their shape, so a call
   that disappeared and an unrelated call that appeared read as one
   renamed call. A call of a chunk the file does not call on the other
   side is now `init-call-changed` (wiring). A declaration the linker
   hoisted is labelled as the input statement it came from.
4. **A local that shadows the previous import alias makes way for it.**
   When a new local variable took an import alias's name, the alias
   moved to a longer name in every file that used it. The local now
   takes another name and the alias keeps its own. In walk 14 one alias
   changed name this way on the first hop; in walk 15 it has one name
   in all three versions. The stub model cannot produce this case.
5. **Unchanged code at another indentation is labelled as re-nesting.**
   It was labelled `reshaped` (noise). Upstream wrapped the code in a
   new block, so it is `input-reindented` (real).

## Why leaving the calls out is safe

A call left out would have done something only if a chunk imported by
this chunk were part-way through its own init at that moment. A chunk
part-way through its init is on the call stack, so it reaches this
chunk; this chunk imports it, so it reaches back. That puts this chunk
on a cycle. A chunk that is only between two cycles is on none, so
every chunk it imports has either not started or has finished, and a
finished chunk has started everything it imports.

The graph used holds static imports and synchronous loads, wherever in
a chunk the load is written. A load reached only through a callback
that a chunk registered with code it does not import is not in the
graph. The rule for chunks away from any cycle has always had this
limit.

## Findings

- **Bun's call order is fixed by its walk, not written by upstream.**
  Bun sorts a chunk's imports by when its walk first finishes each
  chunk, so an edit anywhere can shift one chunk in every list (read
  from Bun's source, not re-tested here).
- **Moving calls that run no code of their own does not help.** A call
  can be moved freely only when everything it runs, transitively, is
  free of effects. Measured on both hops, that removes 1 and 42 moved
  calls.
- **Kept is mostly real edits.** Of about 1,500 name differences on
  `kept-renamed` lines of the first hop, about 1,180 are in places
  where the old name is still in use nearby: upstream put another
  variable there (many are React cache slots that now hold another
  value). About 250 look like one variable given a new name.
- **Most of `linker-renamed` was two different calls.** With calls
  paired by chunk, 26 and 74 lines are renamed linker text; about 700
  lines per hop are calls that appeared or disappeared because Bun
  grouped the code into other chunks.

## Noise and wiring left, by label (walk 15)

| group | label | 285 -> 286 | 286 -> 287 |
| --- | --- | ---: | ---: |
| noise | imports-reordered | 656 | 895 |
| noise | moved-in-file | 149 | 206 |
| noise | renamed | 104 | 134 |
| noise | reshaped | 57 | 53 |
| noise | linker-renamed | 26 | 76 |
| noise | moved-file | 30 | 23 |
| wiring | require-edge | 718 | 741 |
| wiring | init-call-changed | 694 | 694 |
| wiring | export-line | 347 | 611 |
| unknown | input-duplicate | 802 | 868 |
| unknown | linker-unpaired | 337 | 568 |

`renamed` read 162 and 150 before, 214 and 154 in walk 14: it moves
between walks by about as much as it fell here.

## Open

- The reordered calls left are in the roughly 60 chunks on a load
  cycle. Deciding them needs a call graph of the whole program: 317 of
  566 synchronous loads are inside functions.
- `init-call-changed` and part of `require-edge` follow Bun's chunk
  grouping. The linker's lazy unit is the Bun chunk, so a module that
  Bun moves to another chunk changes one line in every file that
  imports it.
- Part of `renamed` is real change the measure cannot see: callers
  that switched to another function while the old one still has users.
- Most of `export-line` follows new code: on the second hop 542 of 611
  lines export a name that was not exported before.
- An init call left out because an earlier call covers it comes back,
  or goes, in every file that imports both chunks when the earlier
  chunk's imports change. The largest case on the first hop is one
  init dropped from 162 files.
