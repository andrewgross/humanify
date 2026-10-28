# exp159 — the names pass: a unit print's ORIGINAL identifier names in the index

STATUS: DONE (2026-10-28). Data + lookup surface LANDED; the naming side is a
follow-up lane (HANDOFF below — nothing in the run reads the table yet, so
every run is byte-identical by construction and proven warm-neutral both
modes). Andrew's directive: when the library index identifies code as package
P file F, a run should be able to restore P's ORIGINAL identifier names
instead of minting them — the index is the only thing that has ever seen the
original source, so the names belong there.

## The census, first (measure before building)

**Census A — is a hash join viable? NO.** The obvious mechanism (match a
minified unit print to the same package's unminified unit print, inherit the
names) was measured on 97 packages / 237 pinned versions
(`scripts/select_sample.py`, `scripts/build_census.sh`,
`scripts/compare_alignment.py`): **only 2,742 of 13,618 min-slice unit rows
(20.1%) hash-align their plain build**, no package above 48%, and the
misalignment is not noise — Bun's minifier structurally REWRITES statements
(comma sequences for separate lets, dropped braces around single-statement
bodies, merged declarators), so print equality is simply not the bridge.

**The pivot — the build's own sourcemap.** The same builds carry a sourcemap,
and Bun anchors every statement and declarator of the minified output to the
ORIGINAL LINE it came from, while the builder has the original npm sources
installed. A declared identifier of a unit anchors to an original line; the
original file's identifiers on that line pair with it in order. The join is
positional and needs no print equality at all:

```
slot -> declared identifier (its span inside the unit's span)
     -> sourcemap anchor at the declaration start -> (source file, original line)
     -> the attributed source file's declared identifiers on that line
     -> pair BY POSITION within the line, only when the two sides agree in count
```

A line whose sides disagree in count recovers NOTHING (a minifier-dropped
local, an injected binding) — loss, never corruption; a slot the join cannot
pair is simply ABSENT, never shifted, so a lookup's answer stays position-true
for the slots it does name. Anchor = the original LINE only (Bun anchors
statement starts at line starts, col 0), never a column.

**Census B — does the real mechanism work? YES.** Both slices rebuilt with
`--names` on the same 237 pins (`/work/exp159-data/ix-names2`,
`/work/exp159-data/census-b.json`):

- **Coverage: 81.0% of the min slice's 13,401 unit rows carry names**
  (10,849). Per package (188): median 92.9%, q10 67.7%, q90 100%, **no
  package at zero**.
- **117,777 names over 11,040 rows** — median 4 per row, p90 13, max 2,911.
  Kinds: 59,942 local, 47,200 param, 9,524 function, 756 catch, 355 class.
- **Accuracy is proven where a hash join CAN check it**: on the 1,728 rows
  whose print the plain (unminified) build ALSO holds — where the positional
  join and the inherited plain-build names are independent computations —
  the minified join agrees with the plain join on **every list and every
  slot (1.0 / 1.0)**, zero sample mismatches. The ~80% the hash join
  couldn't reach are the same code, reached positionally.
- **Short (<=2-char) names are 17.3% minified vs 16.7% plain** — equal on
  both sides, so they are REAL short source identifiers (i, r, e live in the
  originals too), not minifier junk leaking through the join.
- **Ambiguity is rare**: only 1.6% of (hash, owner, file) triples carry
  several distinct names lists (175: 164x2, 8x3, 2x4, 1x6) — the sources
  disagreed across versions. Kept and answered `Ambiguous`, never dropped
  and never guessed among.
- Hand checks: p-limit 7.3.3 restores its exact hand-checked list
  (rejectOnClear, concurrency, queue, activeCount, resumeNext, next, run,
  ...); yocto-queue's row restores class Queue / class Node / local node /
  local current against the real source.

## Format choice: additive `/4`, not `/5`

INDEX_FORMAT stays `humanify-library-index/4`. The names pass is ADDITIVE —
it writes three new files and adds one OPTIONAL fingerprint stamp — and a
`/5` would have forced a version bump on every other table for nothing:

- **Presence** is the manifest's `tables` map listing `names.<slice>.bin`
  (the same pattern as HELPERS_FILE): listed means present, listed-and-missing
  is a REFUSAL at load, never an empty answer.
- **The stamp** `fingerprints.names` = `names/1` is OPTIONAL in the
  fingerprint set (`OPTIONAL_STAMPS` in `libindex/format.rs`): an index
  without the stamp is readable and answers `Absent` everywhere; a DIFFERENT
  value (`names/2`) is refused. Absence is fine, mismatch is not.
- **The shard key** gains `+unit-names/1` (NAMES_SHARD_STAMP) when `--names`
  is on, so a names build never reuses a plain shard's fingerprints —
  rebuilding a slice with names re-fingerprints it rather than inheriting
  rows that have no names. Plain shard bytes are byte-identical with the
  flag off (ShardUnit.names is serde-skipped when empty; the flag is OFF by
  default).
- Measured: every table the names pass does not touch is byte-identical
  between a plain index and a names index over the same pins
  (`ix-sample` vs `ix-names2`: units, modules, edges, callee-prints,
  literals, packages, versions — all cmp-clean).

## Files

- `names.<slice>.jsonl` — provenance: one row per (unit hash, owner, file,
  DISTINCT names list) with the list's version ranges. Read by instruments,
  never by a run.
- `names.<slice>.bin` — the lookup table (`HXNAME01`): keyed by UNIT ROW
  (its positional id in `units.<slice>.bin`, not by hash), holding each
  distinct names list once as 12-byte entries (slot u32, name id u32, kind
  u32) behind row_starts / list_starts fences.
- `names.json` — the sorted intered strings; a name id is its position.
- `LibraryIndex::restore_names(hit) -> RestoreAnswer` (`Absent` /
  `Single(list)` / `Ambiguous(lists)`) and the store trait's
  `restore_names` — the prior store (`matching::store`'s FingerprintIndex)
  keeps no names table and answers `Absent`, never a guess.

The names capture itself (`slot_decls` / `declared_decls` / `join`) is PURE
code in `humanify-core/src/libindex/names.rs`, driven builder-side by
`humanify-index/src/names.rs` (the map as byte-offset anchors; each
attributed source parsed once). The canonical slot mapping is captured
beside every print by `libindex::units::named_print` — the HASH is
unchanged, so every unit-model digest pin holds.

## Size

Measured on both builds (per-file bytes):

| build                              | baseline               | names build                  | names files  | other delta                                        |
| ---------------------------------- | ---------------------- | ---------------------------- | ------------ | -------------------------------------------------- |
| small (320 versions, both recipes) | 53.19 MB (`ix-edges`)  | 139.14 MB (`ix-small-names`) | **85.38 MB** | manifest +0.57 MB (the `+unit-names/1` shard keys) |
| census (237 pins, both slices)     | 14.98 MB (`ix-sample`) | 31.47 MB (`ix-names2`)       | **16.49 MB** | manifest +514 B                                    |

The names cost is ~2.5x the base index's size at this shape, but the split
matters: the RUN reads only the lookup bins + the strings file —
17.5 MB of the 85.4 MB on the small build (bins 16.6 MB, `names.json`
0.86 MB); the other 67.9 MB is `names.<slice>.jsonl`, the provenance table
(one row per (hash, owner, file, list) with full name lists and version
ranges — read by instruments, never by a run). A build that wanted the
lookup cost alone would pay ~20% of what the full pass writes.

Every table the names pass does not touch measured byte-identical on the
small build too (units, modules, edges, literals, packages, versions —
cmp-clean against `ix-edges`; `callee-prints.*` is absent from the
exp151-era baseline because exp155 added it — a binary-age difference, not
a names effect). The 10 npm-install refusals match `ix-edges`' builds
exactly. Names index id
`5d30fedd585254766b035a87991264b99abae752690515a4a51e292be47b2413`.

## HANDOFF — what the naming lane must answer

The index side is DONE; nothing in the run reads the table yet. The naming
lane (the follow-up that actually restores names in deobfuscated output)
must:

1. **Capture the canonical mapping per matched print on the RUN side.** The
   builder captured it through `libindex::units::named_print` because a
   unit print's slot ordinals ARE the key: the index stores names at the
   slots of the print as the builder walked it, and the run reproduces
   identical ordinals for the same print hash (the walk is deterministic
   and the hash is unchanged). The run's discovery print path currently
   DISCARDS the mapping — mirror the builder-side capture (the doc comment
   in `humanify-index/src/names.rs` points at the builder's
   `print_names`; the pure core is `humanify_core::libindex::names`).
2. **Map slot ordinals to the row's list.** For the matched unit hit, call
   `LibraryIndex::restore_names(hit)` (or the store trait's
   `restore_names`) and use the answer's list: each `RestoredName` names
   the slot the run's own mapping holds at the same ordinal. A slot the
   join could not recover is absent — the run's name for that slot is the
   pipeline's business (mint, keep, whatever it does today); the index
   never invents one.
3. **Decide Single vs Ambiguous handling.** `Single(list)` is a direct
   answer. `Ambiguous(lists)` (1.6% of rows) is the sources disagreeing
   across versions — the lane must pick a policy (prefer the version range
   the run's matched owner version falls in, if the index rows' version
   ranges let it; otherwise abstain). The prior-store contract is settled:
   absence is answered `Absent`, never a guess, and the naming lane should
   keep that contract (precision over recall).
4. **Kind is advisory, position is load-bearing.** `NameKind` is the
   ORIGINAL source's kind (a class minifies to a var) — useful for choosing
   how to apply a name, never for aligning: the slot ordinal is the only
   key.

Scripts: `scripts/select_sample.py` (the 97-package sample),
`scripts/build_census.sh` (the census builds), `scripts/compare_alignment.py`
(census A + B scoring). Data: `/work/exp159-data/` (census-b.json, ix-sample,
ix-names2, ix-small-names, store-\*).
