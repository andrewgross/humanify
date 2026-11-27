# exp259 — fewer init calls on load cycles, steadier private field names

STATUS: BUILT, GATED (`cargo dev check`, all 13 stages), measured on
one real-LLM walk of two hops (2.1.285 -> 286 -> 287) against the last
walk of exp258, and on a stub run of both pipelines over the same
versions. Branch `exp259-work`. Follows exp258.

## Totals

Changed lines outside `.humanify/`, real-LLM walk, cold first version.
"Before" is walk 15 (the end of exp258), "after" is walk 16.

| hop | walk | all lines | real | kept | noise | wiring | unknown |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 2.1.285 -> 286 | before | 42,231 | 34,825 | 3,433 | 1,024 | 1,766 | 1,183 |
| 2.1.285 -> 286 | after | 42,099 | 34,743 | 3,511 | 664 | 1,964 | 1,217 |
| 2.1.286 -> 287 | before | 47,935 | 40,729 | 2,254 | 1,392 | 2,070 | 1,490 |
| 2.1.286 -> 287 | after | 46,921 | 40,708 | 2,230 | 706 | 1,925 | 1,352 |

- Every version of both walks boots, the prompt check included, and
  every run exits 0.
- Two walks of one pipeline differ by up to about 500 noise lines
  (exp257). The fall is in `imports-reordered`, the label the change
  was built for (656 -> 281 and 895 -> 265), and the stub run of both
  pipelines shows the same fall (694 -> 262 and 897 -> 265).
- On the first hop wiring rose by about 200 lines (`require-edge` 718
  -> 866, `init-call-changed` 694 -> 739). The stub run does not show
  the rise in `require-edge` (854 -> 810), so part of it is variation
  between walks.

## What was built

1. **A chunk on a load cycle leaves out a call covered from outside
   the cycle.** exp258 left such a chunk with every init call Bun
   wrote. A call is now left out when an earlier call in the list runs
   its chunk and that earlier call's chunk shares no cycle with this
   one. A call covered only by a chunk on the same cycle stays. A
   model of the input counts about 7,800 calls in these chunks before
   and about 2,400 after.
2. **A private field takes the name of a method that only returns
   it.** A private member with no name source was numbered by its
   place in the class (`#field33`), so one new field renumbered every
   field after it. A public method with no parameter whose body only
   returns the field now names it (`toolsMachine() { return
   this.#field33 }` gives `#toolsMachine`). A `get` accessor still
   wins. Numbered private names in `src/` fell from about 5,050 uses
   to about 4,080.

## Why leaving the calls out is safe

A call does nothing when its chunk has already started. After an
earlier call returns, everything that call's chunk imports has started,
unless a chunk on the way was part-way through its own init and so
returned at once. A chunk part-way through its init is on the call
stack, so it reaches this chunk; this chunk reaches it through the
earlier call. Then the earlier call's chunk would share a cycle with
this one, which the rule excludes.

The limit is the one exp258 states: a load reached only through a
callback registered with code the chunk does not import is not in the
graph.

## Noise and wiring left, by label (after)

| group | label | 285 -> 286 | 286 -> 287 |
| --- | --- | ---: | ---: |
| noise | imports-reordered | 281 | 265 |
| noise | moved-in-file | 141 | 181 |
| noise | renamed | 116 | 104 |
| noise | reshaped | 60 | 53 |
| noise | linker-renamed | 29 | 75 |
| noise | moved-file | 34 | 23 |
| wiring | require-edge | 866 | 741 |
| wiring | init-call-changed | 739 | 554 |
| wiring | export-line | 351 | 607 |
| unknown | input-duplicate | 762 | 849 |
| unknown | linker-unpaired | 410 | 450 |

## Open

- The reordered calls left are calls of chunks on the same cycle as
  the caller.
- `moved-in-file` is mostly startup statements that Bun wrote at
  another place in the chunk. They stay in Bun's order by decision
  (exp256).
- Wiring is now the largest group after real and kept. It follows
  which chunks Bun groups code into and which calls an earlier call
  covers; both change when imports change elsewhere.
- About 4,000 uses of numbered private names are left.
