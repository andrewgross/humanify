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

## 18. A cache clear can split one scope model into two epochs

The TS naming waves run after `clearBabelCacheAfterPriorMatch`. Every
path created before the clear keeps its scope objects (the graph's
`fn.path` and everything its `.scope.parent` chain reaches); every
traversal after it builds NEW paths, and a new path for an already-scoped
node gets a NEW Scope that crawls its subtree — registration order, the
current names — and resets every nested table on the way. So one AST
carries two tables per block: the graph-era one a function's CONTEXT
reads (its used names, its lane's collision check) and the fresh-era one
its own TRAVERSAL collects and renames through. A rename through one never
reaches the other. The prompts showed it three ways on the four pairs:
shadowed-binding order (the fresh table re-crawled at the function's
first traversal, not at the clear), a child's used names still listing a
parent block's minified name, and one retry suffixed in Rust but applied
in the TS. Found by the WP4.3 prompt gate over ~9,500 prompts; fixed by
modelling both epochs (graph-era views + per-traversal re-crawl).

Lesson: when the TS clears a framework cache mid-run, list which objects
survive it and which are rebuilt on demand — the survivors are a second
state, and every read must be classified by which state it touches.

## 19. The slot order is the TS walk's, not the Rust serializer's

`collectPriorNames` returns the placeholder mapping's values in slot
order, and the prompt shows them. The Rust canonical serializer assigns
slots by first occurrence in ITS walk (02 §4a: bytes differ by design, so
the partition is gated, not the ordinals). The prompt needs the TS walk's
order — `Object.keys` of the babel node, i.e. the PARSED field order
(`SwitchCase`: consequent before test), which the close tier's
`BABEL_CHILD_KEYS` already carries. Lesson: a value the partition gate
blessed may still carry an ORDER nobody gated; a new consumer that shows
it must re-derive it from the TS's own walk.

## 20. A rename nobody records is invisible to every row gate

The statement twins rewrite PRIVATE names (`#f` → `#A`) by mutating the
PrivateName nodes: not a scope binding, so no validated rename, no trail
row, no names.json row. Phase 3's transfers gate, WP4.3's names gate and
every row compare since were blind to it by construction — the Rust
carried the sets (`TransferOutcome::private_renames`, "the render applies
them") and nothing rendered them. The first gate that compared a TEXT the
naming era produced (WP4.5's `generated.js` sha) went red on its first run
on three lines of 2.1.86 (`#f`/`#A`), with all 109,974 trail rows IDENTICAL
(render fix + red test `private_rename_sets_apply_in_order`).

Lesson: for every stage, list the writes that bypass the recorder (here:
private names, uniquify/identity renames, library prefix) and gate at
least one artifact that CONTAINS them — a byte compare of the stage's
output text is the cheapest such artifact.

## 21. The TS's own unit tests are the regime generator — record them

The four oracle pairs exercise none of the reconcile's mixed-hunk or
import-alias options, no decoration-retry apply, no pre-generate sweep, no
carried-name exemption, no ESM. The passes' TS suites exercise all of
them. Instead of hand-copying fixtures, WP4.5 instrumented a SCRATCH tree
of the oracle commit (`test/parity/wp445-harvest-hooks.py`: each ported
function renamed to `__inner` behind a wrapper that records inputs,
outputs, the trail rows written and the LLM requests/responses of every
TOP-LEVEL call) and ran the unmodified suites: 183 tests still pass (the
hooks are inert), 474 distinct calls recorded, replayed by the Rust tests.
The first replay found a render form no CJS bundle can contain (a renamed
shorthand `export { x }` prints `local as x`).

Lesson: when a pass fires rarely at scale, its unit tests are the densest
map of its regimes — record what the real TS does on them rather than
re-deriving expectations by hand.

## 22. Gate a reformatting pass on every file it COULD print, and look for an old experiment's pre-state

Two WP5.4 paths could not be seen by the oracle pairs as run. The `using`
desugar regenerates a file through `@babel/generator` under `retainLines`,
and only 21 files across the four pairs declare `using` — a printer port
gated on those 21 would have exercised a handful of node types. The printer
was instead gated on its output over EVERY file of the four final trees
(22,526 files: the TS's `transformSync` with no plugins vs
`humanify retain-lines`); the plugin on the 21 real files plus 16
constructed cases (`test/parity/wp54-desugar.json`). The post-split
reconcile considered ZERO files on every oracle pair (the exp050 priors
share no path with the fossil layout); its natural input turned out to be
the exp054 A/B's `off` trees — the PRE-reconcile state of four real hops,
kept on disk since August — plus cross-lineage pairings of walk trees
(`/work/wp54/regimes.sh`: 6,831 renames, 2,621 carried into bundles).

Lesson: when the transform touches few inputs, gate the machinery it runs
on (the printer) over the whole corpus; when a pass never fires on the
oracle, search `/work` for an old experiment that kept the pass's INPUT
state (an A/B `off` leg is exactly that). And plant perturbations against
each: one plant (vendor-inherit keyed on the BLURRED literal policy)
passed every corpus gate — the 17 not-inherited vendor files per pair
differ structurally, so only the unit test's same-length-literal case can
see it.

## 23. The oracle pairs are one bundler; the fixture corpus is the ESM regime

Every oracle pair is a Bun CJS bundle, so nothing on them ever produced an
`export` statement. oxc 0.150 splits babel's single `ExportNamedDeclaration`
into THREE kinds (`ExportDeclaration` for `export const …`,
`ExportFromDeclaration` for `export … from`, `ExportNamedDeclaration` for a
bare specifier list); four babel-statement predicates written against the
bundles listed only the last, so a call inside an `export const` arrow had
no statement parent and recorded no call site. The first WP4.6 run over the
e2e fixtures (warm replay from the standing cache — their first-version
prompts were cached by the WP0.4 cut) found it plus four more ESM-only
shapes in minutes: an import specifier's declaration text, the shorthand
babel prints for an aliased specifier renamed to its other side, the
`export const` split (#16's render half) and the unambiguous source type
(finding #38).

Lesson: grep the port for every `AstKind::` alternative list that names
one babel node kind oxc models as several, and run the fixture corpus as a
regime of every gate that can take a first version.

## 24. A diagnostic count can carry the scope-epoch model

`refCount` (the exp059 instrument on every `llm` trail row) is
`referencePaths.length + constantViolations.length` of a Babel `Binding` —
deduped by PATH object. After the prior-match cache clear, a fresh-era crawl
whose chain reaches a graph-era Binding re-registers each reference in its
BLOCK through new paths, so 921 of 3,254 counts on 2.1.85→86 are inflated
(finding #35). Reproducing it needed the traversal model lesson 18 built for
the prompts, one level finer: which function's traversal first created each
nested scope's paths (the path cache is keyed by parent node, so the first
traversal wins), block containment rather than semantic scope (a switch
discriminant), and `registerBinding`'s skip of a binding's own declaration.
The first-run gate's IDENTICAL on the no-prior fixtures, and 921 diffs on
the pairs, is what localized it: one epoch vs two.

Lesson: a field recorded "for debugging" is still output; when it reads an
object the TS has two copies of, port the copy model, not the field.

## 25. A replay gate's "zero misses" meets the requests the oracle also failed

M3's first end-to-end run was byte-identical on every file of all four
pairs — and every pair logged one request the cache could not answer. It
was the fossil mint namer's single all-mints prompt (759K–1.2M chars),
which the model refused with a 400 context-length error during the oracle
cut (finding #39). An error is never cached, so that request is a miss in
EVERY replay, forever; "0 misses" was unattainable by construction, and a
gate demanding it would have had to exclude the pair or the check. The
check that holds is set equality: every request the Rust could not replay
must be byte-identical (system + user prompt) to a request that ERRORED in
the oracle's `-vv` log, and the TS's errored set must equal the Rust's
(`m3/miss-audit.py`). The endpoint is a dead port, so a Rust-only miss can
never be answered by a model and silently repair itself.

Lesson: when a cache is the replay mechanism, audit what it could not
answer against what the oracle could not get answered — do not count to
zero.

## 26. Prove a wiring gate with plants at the handoffs, and prove each plant fires

The per-stage gates were all green, so M3's gate had to show it can see a
WIRING mistake — a value handed from one stage to the next wrongly. Five
one-line plants went red on 85→86: the split fed the GENERATED text
instead of the shipped one (the hash injection's bijection check refused
it before any tree was written), the finish handed no runnable list
(2,597 files differ), two adjacent statements swapped files at the
placement handoff (5 files: the three sources, the ledger, the stage
hashes), the prior carry not handed on (ONLY `prior-match-map.json`
differs — the `-vv` debug file, which the WP5.4 gate had excluded), and the
close-match contexts dropped before the waves (1,369 Rust-only unanswered
prompts). Two subtler plants — ONE function's transferred-name record, ONE
close context — left the tree and every prompt identical: the waves never
re-read those records. They are kept as controls, not counted as proof.

Lesson: an exclusion list is where a wiring bug hides (the carry plant is
visible in exactly one excluded-by-precedent file), and a plant that
changes nothing proves nothing — count its effect before citing it
(lesson 17's rule, applied to the plants themselves).

## 27. A reused walk table is only as wide as its first consumer's regime

The library carry needed babel's pre-order `Function` walk over WHOLE
programs, and `place::babel_walk` already owned the ESTree→babel shape
question — but its `VISITOR_KEYS` table had been cut for the split's
walks over CJS statement bodies: no `Program`, no import/export
declarations. An unknown type answers "no children", silently: the walk
stopped at the root, and an `export default function` hid its function.
Nothing on the four pairs or the split's gates could see it. Vectors
frozen from the REAL TS beautify (test/parity/library-carry.json — ESM,
methods, classes, a transform that reorders functions) caught it on the
first run, and a planted removal of one key re-proved the red. The same
vectors carry the port's second proof: a carry over the RAW parse matches
the TS output-tree carry on every vector except the one flipComparisons
reorders — the reason the 5b lane must carry on the transform's output
tree, recorded as a test rather than a comment.

Lesson: when a new consumer reuses a shared table-driven walk, generate
vectors in the NEW consumer's regime from the TS; "no children" is a
truncation, not an answer. And when a classification needs data the
ingesting leg cannot have (raw starts vs beautified text), consume it and
check what can be checked (the binary proves the TS's banner regions equal
its own before joining a single span).

## 28. A framework's bookkeeping is part of its output

The beautify's plan of record said "one merged traversal with Babel's
requeue semantics". The bytes turned out to depend on much more of
`@babel/traverse`: the per-(parent, node) path cache, contexts, the
`visited` set of each queue, `updateSiblingKeys`, the wrap-in-a-block
branch of `insertBefore`, and — found by the fuzzer, invisible on 29,175
real files — scope creation. A statement wrapped in a new block gets a new
Scope, whose `init()` crawls the block with `NodePath.get`, and `get`
RE-PARENTS every cached path it reaches; a path requeued before the crawl is
visited after it with the crawl's parent. `while (x) a, `u`.concat(y) ?? z;`
only becomes an `if` because of that. The emulation replays the crawl as a
read-only traversal over the same machinery (`format::traverse`); the
`no-crawl` plant is red on 1.6 % of fuzz programs and on nothing else.

Lesson: when the port has to reproduce what a framework DOES to a tree,
list the framework's side effects on its own bookkeeping (caches,
back-pointers, contexts), not only the operations the plugin calls — a
"read-only" helper that fetches paths is a writer.

## 29. Validators and builders are behavior; the TS can crash on valid input

`t.templateElement`'s validator recomputes `cooked` from `raw` and throws on
a raw that would end the template; `_replaceWith` validates against the
path's RESYNCED parent while writing into its old container. Both change
what the TS does — a recomputed cooked value decides whether the `.concat`
fold appends once or twice, and both validators make the TS stage-6
beautify THROW on valid programs (finding #44). A parity port reproduces
the throw (the Rust errors on the same inputs). The goldens record the TS's
error, not only its text, and a both-error case counts as agreement.

Lesson: port the validation the TS runs on the way (builders, `validate`,
parse early errors), and put error cases in the golden set.

## 30. A differential fuzzer reaches regimes no corpus has

G1/G2 were green on the first full run over 202 real inputs, and stayed
green on 28,973 more. A 60-line grammar fuzzer (test/parity/format-fuzz.mjs,
module-valid programs dense in the constructs the visitors rewrite, comments
sprinkled) then found in its first 2,000 programs a traversal-order
divergence (the crawl, lesson 28), a Rust crash where the TS throws, and two
latent TS bugs (#44, #45) that 29,175 real files never trigger. Its first
draft was 75 % syntax errors (both sides reject them, so they prove
nothing); parenthesizing compound operands brought it to 0.5 %. And one
plant stayed green everywhere — requeuing to the END of the sibling queue
instead of the priority queue — because the stage-6 visitors are confluent
under that reordering; it is kept as a control, not counted (lesson 26).

Lesson: after a first-run IDENTICAL on real data, fuzz the grammar the
transform reacts to, and measure the fuzzer's own validity rate before
believing its zero.

---

Provenance: lessons 1, 3, 6 (gate logs /work/rust-port/gates/wp1.5/),
4 (c93cae2, 2cc35d9), 2 (6f69b62, 8400f3d, 126904b), 5 (the cascade's
module docs), 7 (oracle-dc1a80d's cuts + the handback note
/work/rust-port/handback/wp1.3-1.5-2026-09-20.md), 8/9 (the WP2.2 port
report + probes under test/parity/), 11-14 (b53b3a8/dd0570a/a7cfac3, the
matches.close gate's three debugging rounds), 15 (/work/rust-port/gates/wpb1/ and wpb5/), 16 (/work/rust-port/gates/wpb2/), 17 (/work/rust-port/gates/wp5.3/), 18-19 (/work/rust-port/gates/wp4.3/), 20-21 (/work/rust-port/gates/wp4.45/), 22 (/work/rust-port/gates/wp5.4/), 23-24 (/work/rust-port/gates/wp4.6/), 25-26 (/work/rust-port/gates/m3/), 27 (/work/rust-port/gates/library-freeze/), 28-30 (/work/rust-port/gates/wp5.6/). The doc grows at each
arc's handback.
