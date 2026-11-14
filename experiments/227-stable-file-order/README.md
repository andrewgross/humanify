# exp227: a stable order inside each file and in the entry

STATUS: BUILT (TDD), GATED (all 11 stages pass), FOUR REAL HOPS (live
model, all exit 0, all three trees from this branch boot both halves). No
eval, no walk, no split-era hop. Branch `exp227-stable-file-order` from
main `70884aef1`. No Claude Code text is committed. The shapes below are
described generically.

## Totals

Classifier (`/work/noise-study/tree_stmt.py`, roll-up as in `rollup2.py`)
on 2.1.238 -> 2.1.239. Both runs start from the coordinator's control
tree `ctl-2.1.238` (written by main `70884aef1`). The main side is the
coordinator's `ctl-2.1.239`.

| 2.1.238 -> 2.1.239 | main | exp227 | change |
| --- | ---: | ---: | ---: |
| changed lines | 71,136 | 70,066 | -1,070 |
| REAL lines | 61,267 | 61,005 | -262 (explained below) |
| noise lines | 9,869 | 9,061 | **-808** |
| **noise per 100 real** | **16.11** | **14.85** | **-1.26** |
| MOVE in file: statement order changed | 715 (1.17 per 100) | 31 (0.05) | -684 |
| MOVE in file: require lines reordered | 182 (0.30 per 100) | 0 | -182 |
| both order buckets | 897 (1.46 per 100) | 31 (0.05) | **-866** |

The second hop (steady state) is 2.1.239 -> 2.1.240: exp227 starts from
its own 2.1.239, main from `ctl-2.1.239`. It is a small hop (2,172 REAL
lines on both sides), so it only shows that nothing comes back:

| 2.1.239 -> 2.1.240 | main | exp227 |
| --- | ---: | ---: |
| REAL lines | 2,172 | 2,172 |
| noise lines | 48 (2.21 per 100) | 46 (2.12 per 100) |
| order buckets | 2 (the entry) | 0 |

| real hop (binary from this branch unless noted) | exit | `--version` | `-p boot-ok` |
| --- | --- | --- | --- |
| 2.1.238 -> 2.1.239, statement order only (first build) | 0 | pass | pass |
| 2.1.238 -> 2.1.239, statement order + entry order | 0 | pass | pass |
| 2.1.239 -> 2.1.240 from exp227's own 2.1.239 | 0 | pass | pass |
| 2.1.239 -> 2.1.240 from `ctl-2.1.239`, main's binary (the control) | 0 | not run | not run |

## What decided the order before

Two different mechanisms. Neither was a sort that changed.

**1. The require lines that moved were all in the entry, `index.js`.**
Every sampled `wiring:require:reordered` line, in every hop of the noise
study (pre-split and split era), is an `index.js` line. The require block
at the top of each split file was already a deterministic function of
what it requires: it is sorted by target path. Export accessor lines are
sorted by name (and `wiring:export:reordered` was 0). The entry, though,
listed every file in the bundle's FIRST-STATEMENT order. When the bundler
moves a module, the files holding its statements change places in that
order. On 2.1.238 -> 2.1.239, 91 of the 5,635 files present on both sides
moved (406 on 2.1.215 -> 2.1.216). That is 182 lines on this hop.

**2. The statements that moved inside a file were EDITED statements.**
Emission order was already aligned to the prior tree (exp037/exp050): a
statement takes its prior slot when its key (statement hash plus every
name it binds) occurs exactly once in the file on each side. Measured
from the two ledgers, every key-matched statement was already in prior
order (0 of 45,099 out of order). What moved was a statement whose hash
changed because it was edited. Its key matched nothing, so it followed
its bundle predecessor. Where the prior order and the bundle order
disagreed around it, it jumped. Example shape: a large component function,
edited in this version, moved from the middle of its file to the end,
because in the bundle it sits after four helpers that the prior order had
placed after it. On this hop: 13 edited statements moved, 686 lines. The
two largest were 404 and 95 lines.

The classifier charges every line of a moved edited statement that the
line diff cannot align. Lines that are identical inside the statement
pair go to `edited:same`, which rolls up to REAL. That is why REAL drops
by 262: `edited:same` falls from 1,078 to 854 (-224) once those statements
stop moving. The rest (-38) is within the run-to-run spread of the other
REAL columns (`edited:shift` alone reads 2,331 / 2,401 / 2,325 on main and
on the two exp227 runs).

## The change

**Statement order (`emit::align`).** After the key claims, a statement
whose key claimed nothing may claim the prior slot of the one prior
statement it shares a name with. Conditions: that name is bound by
exactly one statement on each side of the file, every such name points at
the same prior statement, and that slot is not already claimed by a key.
A name shared by more than one statement, such as a parameter name like
`props`, claims nothing. As before, `load_order::order_respecting_load_order`
has the last word: a `var`/`let`/`const` initializer or an expression
statement never crosses one it depends on or that has a load-time effect.
Function declarations are free.

**Entry order (`emit::entry_order`, driven by `emit::cjs`).** The entry
keeps the PRIOR tree's order for every file that is still there. It reads
the prior tree's own `index.js`. A new file goes right after its bundle
predecessor. Two checks guard load-time behaviour:

- Effect order. Node runs a file's header requires before its body. An
  "effect file" has a statement with a load-time effect (in the
  load-order model) or a write to another file's binding. The new entry
  is simulated, and Node must run every effect file's body in the same
  order as it would for the bundle-order entry. If not, the entry lines
  that loaded the first disagreeing effect file are pinned to their
  bundle-order places and the order is rebuilt. This repeats until the
  orders agree, for at most 512 rounds; otherwise bundle order is kept.
  So no require with a load-time side effect changes its relative order.
- The existing load-order hazard check (a body reading a file still
  loading its header) runs on the new entry, which is dropped if a hazard
  is found. Deferrals are settled before this and never change, so every
  other file is byte for byte what main writes.

With no prior, or when no order passes both checks, the entry is bundle
order, byte for byte as before. The frozen TS vectors replay unchanged.
The second comment line of `index.js` says which order was used. On
2.1.238 -> 2.1.239 the prior order was kept for all 5,635 surviving files:
none ended up out of prior order.

I rejected the other option the brief allowed, sorting the entry by path,
because it would change the load order completely on the first hop. The
prior order is the previous bundle's order, so it is the closest order to
the one that is known to boot.

## Gate

| stage | result |
| --- | --- |
| shims | PASS |
| no-private-addresses | PASS |
| rust:fmt | PASS |
| rust:clippy | PASS |
| census:clones (ADVISORY) | REVIEW (only findings that were there before; none in the files changed here) |
| census:concurrency | PASS |
| census:slop (ADVISORY) | PASS (core erosion 32.6% -> 32.5%) |
| rust:unit | PASS |
| rust:build | PASS |
| format-golden | PASS |
| e2e | PASS (22 pairs, 176 launches equal to the golden) |

New tests: `align_test` (an edited statement keeps its prior slot by a
name only it binds; a shared name claims nothing), `entry_order_test`
(prior order kept, new file after its predecessor, pinned files keep
bundle order, effect run order follows Node's header-first loading,
pinning keeps every effect in bundle order, the entry parser), and
`cjs_test::the_entry_keeps_the_prior_order_except_for_load_time_effects`.

## Open items

- Split era (2.1.300 -> 2.1.301) was not run: the four-hop budget went to
  the two 2.1.238 -> 2.1.239 builds and the 2.1.239 -> 2.1.240 pair. The
  entry mechanism is the same there (every sampled reordered require line
  in the split-era study is an `index.js` line too). It needs a main
  control 2.1.300 tree to measure from.
- The 31 order lines left on 2.1.238 -> 2.1.239 come from edited
  statements whose own name changed along with the edit, so no unique name
  links them to their prior slot. Example shape: an edited function that
  was renamed follows its bundle predecessor and lands on the other side
  of an unchanged neighbour. The line diff then shows the neighbour (23
  lines) as moved.
- novel / realLn (the eval scorer) were not measured. No eval was run.
  The classifier's REAL moved by -262, and -224 of that is explained
  above.
- Overlap with other lanes: exp226 (placement) decides which FILE a
  statement goes to. This lane only orders statements within a file and
  orders the entry. A file whose first statement moves because of
  placement no longer moves in the entry. exp225 (names inside edited
  functions): the name tier keys on the names a statement binds, so a
  more stable local naming in edited functions could let some of the 31
  remaining lines claim their slot. That interaction is good, not a
  conflict.
