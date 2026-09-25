# 15 — Porting lessons: what the parity gates taught

**Status: the implementing agent's distillation (2026-09-20), written for
learning — every entry cites the commit or gate log that taught it.** The
migration's premise is that identical decisions are provable, and the
process of proving them keeps finding mechanisms nobody knew the TypeScript
carried. This doc is the accumulating list, organized by what the LESSON is
about — each entry says where the full story lives.

## 1. A visitor's alias list is a behavior surface

Babel's `traverse` dispatches on ALIASES. `x?.()` parses as
`OptionalCallExpression`, whose alias list does not include
`CallExpression` — so a `CallExpression: {...}` visitor never sees an
optional call, silently: no edges, no names, no counts. The graph's
callee analysis dropped every optional call's edge for the entire life of
the TS pipeline. Found because a one-edge difference refused to go away;
the fix was validated by PREDICTION (three rows' counts stated before the
compare, then matched byte-exactly) — see commit 2c7113c.

Lesson: when porting a traversal, enumerate the visitor's alias list per
node kind; when auditing, ask what a visitor does NOT dispatch on.

## 2. Same library, different node kinds — the parser's model is semantics

- oxc preserves `ParenthesizedExpression`; Babel drops it. Every TS
  `t.isX(expr)` must see through parens in Rust — ONE owner now
  (`core::babel_view::unparen`; the hand-rolled copies it replaced lived
  in four files — the census would have flagged the fifth).
- oxc's object methods are `ObjectProperty { method: true }` with an inner
  Function node; Babel has `ObjectMethod` as ONE node whose span includes
  the key. The method node is the graph row; the inner function would be a
  duplicate row. And methods have NO `id` — a row's name comes from
  `node.id` only, so a method's name is "" even when its key is an
  identifier (commit 6f69b62).
- oxc stores a function body as `FunctionBody`; Babel's body IS a
  `BlockStatement` — a container lookup keyed on block shape finds nothing
  (the classification's zero-factory bug, commit 8400f3d).
- Babel represents `({all: aHu} = x)` as ObjectPattern/ObjectProperty;
  oxc as `AssignmentTargetPropertyProperty`. A KEY in a pattern is a
  NON-binding identifier position Babel's Identifier visitor still visits
  — so the module-binding edge builder edges it when the name matches a
  module binding. A clean-room port that only walks "references" misses
  the whole class (the Pp→all edge, commit 126904b).

## 3. `isBindingIdentifier` is positional, not what its name says

Empirically pinned by probes (the truth table is in the cascade's module
docs): Babel's `isBindingIdentifier` is TRUE for plain references inside
unary arguments (`!y`) and assignment targets, FALSE for object-property
values, member objects, private names and non-computed keys. The TS's
exclusion checks therefore exclude a very different set than the name
implies — read Babel's `isBinding` keys table (it lives in
@babel/types getBindingIdentifiers.keys) rather than reasoning about
"bindings". Lesson: any TS predicate with "Identifier" in its name is
positional; probe it on a truth table before porting it as its name reads.

## 4. Map iteration order is a decision input, not an implementation detail

- The ambiguous map: demote/revoke RE-PARK priors via `Map.set`, which
  APPENDS; propagation iterates entries in that order, so entry position
  decides which claims it sees. The Rust reconstructed order from index
  positions → a candidate pool of 7 where the TS held 10 (the debugging
  agent's mechanism trace + round-0 replay probe pin it; commit c93cae2).
- The statement walk's field order: Babel's `Object.keys` = field
  DECLARATION order (callee before arguments); serde_json's map is
  alphabetical — the first pass produced a flipped snap verdict
  (0.4706 vs a different fraction), fixed with the generated
  BABEL_CHILD_KEYS table (commit 2cc35d9).
- The shingle/shape sort: `localeCompare` on a pure-ASCII alphabet is a
  byte sort — prove it per site rather than assume.

Lesson: before porting anything that reads a Map/Set "for all entries",
ask whether any READ can observe position. If yes, the order IS the
interface and must be reproduced by construction (Vec in build order),
never reconstructed after the fact.

## 5. Stale-state accounting is correct behavior

The cascade's stats-vs-rows discrepancies (a tier's pair count differing
from its stat by single digits) are NOT noise: demote/revoke delete from
`matches` but not from `resolutions`, stats are attributed BEFORE
propagation, and the displayed tier falls back to "propagation" only when
no entry exists. A clean-room "fix" of the staleness reds the gate by
design. Port line-for-line in mutation order; document the staleness as
load-bearing (matching/cascade.rs's module doc).

## 6. A gate for a narrower question catches bugs a broader one can't

The modules gate's hash-CLASS check (not the bytes, not just the rows)
caught the semver dot-advance bug — a bug where EVERY semver-shaped
string blurred as a plain length marker, invisible to the statement
partition (which doesn't blur strings) and to the functions gate (which
compared rows, where the bytes were excluded). The bug was the
`is_volatile_semver` loop checking the dot without advancing past it
(commit 8400f3d). Rule: every new equivalence-class gate found a bug in
its first hours. The cost of writing the gate is recovered by the first
class divergence.

## 7. The oracle's dead paths are data too

The TS graph-time factory classification is NULL on every real bundle
(the beautifier splits the `{exports:{}}` marker across lines and the
scan misses) — so the factory-body skip never fires at graph time, in the
SHIPPED pipeline, on all four oracle pairs. The port reproduces this
(same scan, same miss). Parity means reproducing the accidents too —
and recording them: modules.json is two-site (unpack non-null, graph
null) so the gate compares the data that exists. An inventory agent
found this by reading the oracle dumps' EMPTY arrays, not the code.

## 8. Float determinism is achievable when the inputs are counts

The close-match cosine similarity: every feature value is an integer <
2^53, so dot products are EXACT; only two sqrts and a divide are
IEEE-rounded — identical bits in TS and Rust. The scores are compared by
BIT PATTERN in the parity tests because serde_json's float PARSER (not
the math) is up to 1 ulp off on full-precision literals — emit `scoreBits`
hex alongside decimals in any dump that carries floats (commit 2cc35d9).

## 9. Reference semantics: three different questions, three owners

"is this identifier a write?" has THREE answers in the port, each
correct for its consumer: the hash placeholders (all references slot),
the module-binding edges (write flags + unary/update arguments excluded —
Babel's binding-keys table), and the alternation's referencePaths model
(assignment-target writes excluded, `mb++`/for-of targets included —
probed, not assumed). They live as separate named helpers with doc
comments citing their consumer; docs/responsibility.md should carry the
three-way distinction. The dangerous version of this is two helpers
answering ONE question differently with nothing declaring it (the
memory's 11,094-accepts example) — the safe version is the same question
with the DIFFERENCE declared.

## 10. The gates are cold — protect that property

The entire matching cascade (function + binding + propagation + ordinal +
interchangeable + close-match + twins' inventory/unique-tier) runs with
ZERO LLM calls. That is what makes byte-exact four-pair gating possible
at all (rule 10 of measurement-pitfalls: a cache replay is not a verdict).
When porting anything new, note whether it introduces nondeterminism
(floats → pin bits; maps → fix orders) and whether it is cold; the warm
cache exists for the LLM-dependent sections later (naming, WP3+).

## 11. A shifted index that happens to be right is the worst failure mode

`collect_aligned_pairs` indexed the ORIGINAL unit vectors with positions
from the FILTERED remainder slices — every descent after a hash-paired
unit entered the wrong pair. The ported fixtures never caught it because
its two escape hatches are exactly what fixtures tend to contain: a
remainder whose positions coincide with the originals (no aligned unit
before it — prefix remainders, tail-aligned units), and wrong-target
descents that mint nothing (non-containers). The bug produced a count
that CHANGED WITH STATEMENT ORDER — the TS gives the same aligned count
both ways, so order-invariance is the probe. Found by the first gate
that ran the close tier at real scale (21 divergences across 720 pairs,
WP2.2, 2026-09-21; fix b53b3a8, red test
test/parity/wp22-align-red-probe.mjs).

Lesson: when a port re-derives positions through a filter, carry the
OBJECTS like the TS instead — and write one fixture with a hash-paired
unit BEFORE an unpaired one, in both statement orders.

## 12. Object.keys ≠ VISITOR_KEYS — the parser's field order is its own data

The TS token walk iterates `Object.keys(babelNode)`: the PARSER's field
assignment order, which slots non-child scalars BETWEEN the children
(MemberExpression: `object, computed, property`; AssignmentExpression:
`operator, left, right`; UnaryExpression: `operator, prefix, argument`).
Babel's VISITOR_KEYS puts those last. Hash EQUALITY classes survive any
fixed order (a relabeling applied to both sides), but slot ORDINALS and
k-gram SHINGLES do not: `computed:` sitting after `property:` moved a
real hint's shingle jaccard 0.3846 → 0.5, across the 0.5 snap floor.
Found by the matches.close gate's snap flips; pinned by probing the real
parser's key order per node type (WP2.2 round 2, commit dd0570a).

Lesson: a key-order table generated from one source (VISITOR_KEYS) is
not the other's field order — probe `Object.keys` on the real parser
and emit the table from THAT.

## 13. A slice fixture is not the dump's context

Free identifiers serialize VERBATIM in a slice but resolve to `$n` slots
in the full file (they bind to module scope there) — so a fixture cut
from the middle of a bundle shifts shingle jaccard the OTHER way and
"confirms" a wrong theory. Two oxc-vs-babel node-shape differences
(oxc keeps explicit ParenthesizedExpression nodes; babel emits
NullLiteral{}/BooleanLiteral{value} without `raw` while oxc emits
"Literal" with raw+value) only showed their true sign under the FULL
dump texts (WP2.2 round 2's two residual flips, opposite directions).

Lesson: pin contested token-stream behavior on the full context the
pipeline actually walks; slice fixtures are for structure, not for
slot/shingle-sensitive verdicts.

## 14. Optional chains: the wrapper is the shape

oxc wraps an optional chain in `ChainExpression` and normalizes its
links to plain `MemberExpression`/`CallExpression` carrying an
`optional` bool; babel has no wrapper and types EVERY link
`OptionalMemberExpression`/`OptionalCallExpression` with a per-link
`optional` flag. Token-stream parity needs a per-link TRANSLATION, not a
per-node carry: a link is babel's Optional\* type iff its own
`optional: true` OR its object/callee spine reaches one — `a.b?.c()`
keeps its object `a.b` plain, and the paren-terminated `(a?.b)()` stays
a plain CallExpression. A dropped per-link flag shifted k-gram windows
enough to flip a real hint's jaccard 0.5 → 0.493 across the snap floor
(WP2.2 round 3, commit a7cfac3; nine chain fixtures pinned).

Lesson: when the two parsers shape the same construct differently,
serialize the babel shape by TRANSLATION over the oxc spine, and pin
every compound case (`a?.b`, `a.b?.c`, `(a?.b)()`, computed links,
optional call arguments) — the naive wrapper-drop is wrong for the
compound ones.

## 15. A JS regex's character classes are not Rust's

The detection signals are non-unicode JS `RegExp`s, and three of their
classes mean something different from the Rust default: `\s` is
ECMAScript WhiteSpace+LineTerminator (U+FEFF yes, U+0085 no — Rust's
`char::is_whitespace` is the reverse on both), `\b` is ASCII-only (`é`
is a non-word char, so `é__commonJS` IS bounded), and `.` refuses all
four line terminators (U+2028/U+2029 too). The scan windows
(`code.slice(0, 16K)`, `slice(0, 200)`) count UTF-16 units, not bytes.
None of these occur in the real corpus — the 202 real inputs agreed on
the first run — so the gate carries 35 synthetic vectors that do, and a
planted red (Unicode `\s`) proves the gate sees them (WPB.1, 2026-09-24).

Lesson: port each regex as explicit primitives named for the JS
semantics (`core::detect::js_text`), and give the gate inputs that
separate the two semantics — a corpus that never exercises the
difference cannot vouch for it.

Two related JSON traps from WPB.5: `serde_json::Value`'s map is a
BTreeMap, so going through `Value` alphabetizes keys that the TS
emits in insertion order (JS also enumerates array-index keys first) —
order-carrying objects need their own type (`humanify_model::profiling::JsObject`);
and lesson 8's 1-ulp float-parse drift bit again on frozen timings, fixed
the same way (IEEE bits shipped beside the decimals).

## 16. An exemption's boundary can be injected, turning a class gate into a byte gate

The structural hash BYTES differ by design (00-control §3), and every
vendor artifact downstream of them — `lib_<hash8>` file names, the
`runtimeIdentifier` every reference is rewritten to, the LLM batch keys,
carry-over against a TS-written prior manifest — inherits the difference.
Comparing "by class" left the one LLM-dependent step (finding 6) unproven
for days: no Rust leg could ever ask the TS's leftover set. WPB.2's gate
instead PROVES the two hash partitions are one partition (same factory
per bundle position, a bijection between the classes) and then substitutes
the TS's bytes at that single seam (`unpack::gate::inject_ts_hashes`).
Everything after it is the Rust's own decision-making, so the claim
becomes exact: tree byte-identical to the TS unpack stage on all four
pairs, the LLM pass asking the TS's exact prompts (all cache hits). The
un-injected leg still runs, content-joined, so the substitution cannot
hide a Rust-only divergence.

Lesson: when an exemption covers only an INPUT's representation, gate the
consumers by injecting the oracle's representation behind a bijection
check — not by weakening every downstream comparison to classes.

A second WPB.2 find, the same family as lesson 15: WP1.5's
`identify_bun_cjs_factory` counted the TS's 2000-char lookback in BYTES —
a shorter window on non-ASCII text, and a panic when the byte offset fell
inside a multi-byte char (red tests `factory_helper_lookback_*`). Every
JS `slice`/window length is UTF-16 units.

## 17. An IDENTICAL on the oracle pairs can be an identity it never tested

WP5.3's emit gate went IDENTICAL on all four pairs on its first run —
and on every one of them the emission alignment moved ZERO slots: the
`exp050-cold` priors carry the pre-fossil layout, so no fresh fossil file
shares a name with a prior file and every file emits in bundle order. The
augmentation relocation (the 2.1.172 boot fix) never fired either: a
planted "skip relocation" passed all four pairs. The gate as run could not
see the two most intricate paths it was named for. What made it a real
gate: two extra prior regimes on the same shipped texts (each pair's OWN
output ledger, and ANOTHER pair's fossil ledger — the latter moves 210 to
1,388 slots per pair through the aligner and the load-order scheduler),
the per-statement load-order facts dumped by both sides and compared at
scale, and 42 real-TS emits of the unit fixtures (the only place the
relocation runs) replayed byte for byte. Planted perturbations were then
checked against the regime that can see them.

Lesson: after a first-run IDENTICAL, count how often the ported decision
actually CHANGED something on the gate's inputs (moved slots, relocated
statements, declined emits). Zero means the gate proves the identity
function; find or build the input that makes the path fire.

---

Provenance: lessons 1, 3, 6 (gate logs /work/rust-port/gates/wp1.5/),
4 (c93cae2, 2cc35d9), 2 (6f69b62, 8400f3d, 126904b), 5 (the cascade's
module docs), 7 (oracle-dc1a80d's cuts + the handback note
/work/rust-port/handback/wp1.3-1.5-2026-09-20.md), 8/9 (the WP2.2 port
report + probes under test/parity/), 11-14 (b53b3a8/dd0570a/a7cfac3, the
matches.close gate's three debugging rounds), 15 (/work/rust-port/gates/wpb1/ and wpb5/), 16 (/work/rust-port/gates/wpb2/), 17 (/work/rust-port/gates/wp5.3/). The doc grows at each
arc's handback.
