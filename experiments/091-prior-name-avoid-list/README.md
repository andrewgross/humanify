# 091 — prior-name avoid-list

**STATUS: REFUTED AT THE CENSUS (task 0, 2026-09-19). No code built, no walk
run.** The brief's kill criterion was met and then some: on the busy hop the
avoid-list would block roughly five correct name reuses for every collision
it prevents, and on both calm hops it prevents nothing at all.

## Totals (busy hop 2.1.215→2.1.216, gpt-oss walk `/work/exp088-walk`)

| direction                                                     | bindings | lines |
| ------------------------------------------------------------- | -------: | ----: |
| (1) collisions the list would PREVENT (reachable by a prompt) |       26 |    54 |
| (2) correct reuses the list would BLOCK (strict identity)     |      264 | 1,240 |
| (2) same, loose identity (upper bound)                        |      339 | 1,542 |

Calm hops: 213→214 prevents 0, blocks 5 (14 ln); 214→215 prevents 0, blocks
8 (28 ln).

The brief's ~280 lines was the mass of two restore refusal buckets that
_include_ this class; the census finds the prompt-reachable collision class
itself is 54 lines. The rest of those buckets is not this mechanism.

## What was measured

Inputs: prior tree, fresh tree, the fresh run's `-vv` log (every prompt and
parsed answer). Script: `census/task0-census.ts <prior/src> <fresh/src>
<log> <out.json>`; per-hop outputs in `census/task0-*.json`.

**(1) Collisions the list would prevent.** Name-only rename pairs old→new
(083's masked-line pairing) where `new` was a different binding's name in
the prior file, owned by a binding in another top-level statement — so the
sketched avoid set would have carried it and validation would have refused
the answer. Restricted to names that a prompt actually produced this run;
names that reached the tree by transfer/reconcile are counted apart because
a prompt-side list cannot touch them.

**(2) Correct reuses the list would block.** LLM answers equal to the SAME
binding's prior name — strict: the enclosing top-level statement is
masked-identical between prior and fresh and carries that name; loose: only
the declaration line is masked-identical — where the name appears nowhere
in the prompt text. The machinery had no correspondence for that binding
(else it would have hinted the name), so from the pipeline's view the name
is live elsewhere in the prior file and the list refuses it. Each binding
counted once (the debug wrapper logs every call twice: a full-prompt block
and a compact `Code:/Identifiers:/Used names:` block — same call, two
timings). Lines = occurrences of the name in the fresh file, i.e. the lines
that would newly change.

Busy-hop breakdown of (2) strict by prompt kind: function-fresh 101,
retry 63, module-level 44, close-match 35, module-level-with-hint 21.

Narrowing the design to close-match prompts only (the brief's stated
scope) does not save it: 35 blocked vs at most 20 prevented there. A
close-match prompt whose prior block does NOT contain the name means the
name's owner sits outside the matched prior function — exactly the
condition that puts it in the avoid set.

## Full ledger, busy hop

| bucket of the 1,170 rename pairs (1,968 ln)                                   | pairs | lines |
| ----------------------------------------------------------------------------- | ----: | ----: |
| new name owned in ANOTHER prior function, produced by a prompt (list reaches) |    26 |    54 |
| new name owned in ANOTHER prior function, came by transfer (list can't reach) |    38 |    68 |
| new name owned in the SAME prior function (existing duplicate check)          |   332 |   454 |
| old binding ambiguous (several prior bindings share the old name)             |    32 |    70 |
| new name is not a prior binding at all (a genuinely fresh word)               |   645 | 1,199 |
| old is not a prior binding (property/alias token)                             |    97 |   123 |

Of the 26 reachable collisions, the prior owner still carries the name in
the fresh tree in 13 (a real two-identity entanglement) and was itself
renamed in the other 13.

## What the census WRONGLY includes / misses (stated before the numbers)

- (1) is positional masked-line pairing: lines that also changed
  structurally are missed; multiset pairing can pair unrelated lines with
  the same mask. Both directions bias small.
- (2) maps an answer to a fresh binding by NAME; 5,532 answers whose name
  is declared in several fresh files were excluded as ambiguous, 1,476
  whose name is declared nowhere (overwritten by a later pass). (2) is
  therefore an undercount.
- (2) "not in the prompt" is a plain identifier-boundary substring test.
- Strict identity requires the whole enclosing statement to be
  masked-identical, so a correct reuse inside an edited function is only in
  the loose count.

## Why it dies

The list's whole value is to refuse a name the prior file owns. But the
pipeline only knows which prior names are "spoken for" through its own
correspondence, and the census shows the model lands the right prior name
unaided (no hint, no prior block) five times more often than it collides.
Every one of those becomes a forced rename under the list — a stability
loss manufactured by the lever. Same family as exp044's alias reservation
(+3,742 ln): a rule that reserves names on the pipeline's incomplete
knowledge of who owns them.

Closed with exp089 (ask-side hints) and exp083 (answer determinism): the
prompt-side lever family for changed code is exhausted on this model.
