# exp224 — class-private members get names their class decides

STATUS: BUILT (TDD), GATED (all 11 stages pass), TWO-HOP REAL RUN (2.1.237 -> 238
transition, 238 -> 239 steady state, live model) plus one main control
hop. No eval, no walk. Branch `exp224-private-member-names` from main
`d3fed6a41`. No Claude Code text is committed; shapes are described
generically.

## Totals

Steady state, 2.1.238 -> 2.1.239 (the noise study's classifier,
`tree_stmt.py` + `rollup2.py` grouping; per 100 REAL lines):

| | walk `73d01430` (steady) | main `d3fed6a4` control (first hop on the walk's prior) | **exp224 (steady: its own 238 -> its own 239)** |
| --- | ---: | ---: | ---: |
| changed lines | 77,922 | 109,974 | **75,236** |
| REAL (classifier) | 62,641 | 64,779 | **61,752** |
| noise | 15,281 | 45,195 | **13,484** |
| **noise per 100 real** | 24.4 | 69.8 | **21.8** |
| **minified bucket** (unchanged + inside edited statements) | 1,654 (2.64) | 1,680 (2.59) | **48 (0.08)** |
| changed lines that touch a private member (`#name`) | 2,870 | 2,868 | **564** |
| scorer `composeDiff REAL` (`cargo dev eval diff`) | 110,160 | 110,108 | 109,701 |
| scorer NAME-ONLY inside real statements | 1,296 | 1,320 | 1,086 |
| exit / `--version` / `-p boot-ok` | — | 0 / — / — | **0 / pass / pass** |

The 48 minified lines left are NOT private members: they are free
minified globals and single-letter locals (every sample checked).

The transition hop (2.1.237 -> 2.1.238, the walk's 237 as prior, so every
private member is renamed once): exit 0, boots both halves, 55.1 noise
per 100 real, of which 18.26 is the one-time private rename (7,256
lines). The stats block: 727 private members, 0 kept, 155 derived, 572
numbered. On the steady hop: 753 members, 530 kept (the twins carried
the prior's derived spelling), 51 derived, 172 numbered — the same
names either way, since the derivation is a pure function of the
class.

## Gate

`cargo dev check`: **ALL 11 STAGES PASSED** — shims, no-private-addresses,
rust:fmt, rust:clippy, census:concurrency, census:slop, rust:unit,
rust:build, format-golden, e2e (22 fixture pairs, 176 launches equal to
the golden) PASS; census:clones REVIEW (advisory; none of its groups
touch this change's code).

## The problem

A minifier re-letters class-private members (`#a`, `#w`, private methods
`#b()`) on every build: the same member is `#a` in one release and `#u`
in the next. Nothing in the pipeline named them. They are not scope
bindings, so the model, the prior transfer's slot tables and the sweep
never see them. Only the statement twins carried a prior spelling, and
only for a twin statement with pending work — and the prior's spelling
was itself a minifier letter. So every line that used a private member
changed between releases: 1.8 noise lines per 100 real before the
code-split era (the noise study's cause #4), 2.6 per 100 on 238 -> 239.

## The change

`naming::private_members::private_member_edits` (new, the one owner of
every private-name edit). At the naming era's generate, after the
statement twins' carried spellings, every private member whose name is
still minted (`rename::floor::is_minifier_token` under the run's
profile) takes a name **derived from its own class**, never asked:

1. a public accessor that only returns it or only writes it
   (`get size() { return this.#a }` -> `#size`);
2. an object key whose value reads it (`{ keyMap: c.#a }` -> `#keyMap`);
3. an assignment from a property read (`this.#a = o.timeout ?? 5` ->
   `#timeout`);
4. a GLOBAL constructor it is built from (`#a = new Map()` -> `#map`);
   a local class's name is the model's answer, so it is not a source;
5. otherwise its kind and ordinal among the class's private members of
   that kind, in declaration order (`#field2`, `#method1`,
   `#accessor1`).

Every input is a spelling the minifier keeps (property keys, public
member names, globals) or a position, so **a class that did not change
prints its members identically in every release** — no prior needed, no
model draw to re-roll, no new prior-transfer machinery. Option (a) of the
brief (ask the model, carry by class and role) was rejected: carrying
needs a class-level match the transfer does not have for edited classes
or classes nested in functions, so every such class would re-roll.

Rules that keep it safe:

- Names stay legal and unique per class: a word must pass
  `rename::validated::target::is_valid_rename_target` (the legality
  owner), must not be `constructor` (an early error as a private name)
  and must not itself be minted; a taken word gets a suffix (`#map2`).
- A name that is not minted (the author's, a restored library's, a twin's
  carried spelling) is kept and counted as taken.
- Nested classes: an inner class never takes a name an enclosing class
  ends with (it would capture the outer member's uses inside it), and an
  outer class never takes a name an inner class keeps.
- References resolve to the innermost enclosing class body declaring the
  name (computed keys included, `extends` excluded), `#x in o` included.
- Applied as text edits beside the twins' (docs/responsibility.md "Known
  exceptions": the `#` namespace has no validated path). The naming
  validator's rename signature already blinds private names, and the
  statement hash (split, scorer) masks every name, so neither moves.
- `--stats-json` gains a `privateNames` block (members, kept, derived,
  numbered).

Files: `crates/humanify-ag-core/src/naming/private_members.rs` (+ 19
tests in `private_members/private_members_test.rs`),
`naming/driver/era.rs` (the generate call), `naming/driver.rs` and
`humanify-ag-model/src/stats.rs` (the stats block),
`test/parity/wpb4-stats-schema.json` (the shape gate), and the docs
(`docs/responsibility.md`, `docs/pipeline-stages.md`,
`naming/passes.rs` module doc).

## Reading the numbers

- **The private bucket is gone.** 1,654 -> 48 lines (2.64 -> 0.08 per 100
  real), and the 48 are other causes. Lines touching a private member
  fell from 2,870 to 564; the 564 are classes that really changed (new
  members, new classes, edited bodies).
- **REAL moved down, by what the change explains.** Classifier REAL is
  889 lower than the walk's, the scorer's REAL 459 lower. Of the ~2,300
  private-touching lines that stopped changing, the classifier had
  filed 1,654 as minified noise; the rest sat inside edited statements
  where it could not align the line, and the classifier counts an
  unaligned line as REAL (its documented lower-bound rule). The scorer's
  NAME-ONLY-inside-real column falls by 210 for the same reason. No
  statement became new or gone because of a private name: the statement
  hash masks every name.
- **The total fell 24.4 -> 21.8 per 100.** About 2.6 of that is this
  change; the rest of the buckets are within draw-to-draw movement of
  the walk's (re-indent 6,799 vs 6,733, locals in edited statements
  1,912 vs 1,836).
- **The control is not a clean steady state, and that is not this
  change.** main `d3fed6a4` run on the walk's 2.1.238 tree (written by
  `73d01430`) re-names ~13,600 locals in unchanged statements and ~4,700
  declarations: main's first hop on a prior written by the older binary
  is itself a transition. It is reported only so the coordinator sees it;
  the exp224 steady hop ran on a prior its own binary wrote, so that
  transition is in its 237 -> 238 hop, not in 238 -> 239.
- **Split era not run.** The study measures this bucket at 0.03-0.08 per
  100 real on the split-era hops (272->273, 285->286): the linked
  program's private names barely re-roll there. Budget went to the
  pre-split two-hop protocol instead.
- **2.1.239 -> 2.1.240 was not used as the steady hop**, as the brief
  first suggested: that hop is tiny (2,228 changed lines, 6 minified), so
  it cannot show the effect. 238 -> 239 (the largest minified bucket in
  the study) was used, with 237 -> 238 as the transition.

## Readability

Derived names reproduce the author's where the class says them: a vendored
LRU cache comes out with `#keyMap`, `#keyList`, `#valList`, `#next`,
`#prev`, `#sizes`, `#starts`, `#ttls`, `#max`, `#maxSize`, `#dispose` —
its real source names, read off its own accessors and introspection
object. Elsewhere about three in four members fall to the ordinal
(`#field3`, `#method1`): stable, legal, but not descriptive.

## Runs

All under `/work/exp224-runs/` (this host only): `prior-2.1.237` and
`prior-2.1.238` (copies of the walk's trees), `cand-2.1.238`,
`cand-2.1.239` (exp224 binary, built from this branch before a
clippy-only type alias), `ctl-2.1.239` (main `d3fed6a4`, the split-era
walk's binary). Each launch is the walk's exact one (same flags, same
library index `77b12699…`, live gpt-oss-20b), with its own stats and log
beside it. Three real hops of the four budgeted (a fourth, a control
239 -> 240, was stopped after 30 s when that hop proved too small).

## Open items

1. **Descriptive names for the ordinal three quarters.** A model ask per
   class would read better than `#field3`, but its answers re-roll; it
   would need a carry keyed by class + member role to stay stable, which
   the transfer does not have for edited classes or classes nested in
   functions. A cheaper next step: more derivation sources (a constructor
   parameter's key in a destructured options object, a private method
   called only from one public method).
2. **Main's first hop on a `73d01430` prior renames ~18,000 lines of
   locals and declarations** (the control column). Not this lane's area;
   worth a look before the next walk is compared against the old one.
3. No eval run; `privateNames` is a new `--stats-json` block (additive,
   absent on older cards).

