# 060 — Identity and name allocation: a design brief

> ## STATUS: **BRIEF ONLY. No code, no measurement, no verdict.** This is what was believed on 2026-08-04 before any of it was built.
>
> Written to hand to fresh-context reviewers, deliberately stating what is
> NEGOTIABLE and what is MEASURED. Three independent reviews were run against
> it; their convergent findings, and the parts of this brief they REFUTED, are
> in [`FINDINGS.md`](./FINDINGS.md) — read that second, and trust it over this.
>
> Nothing here is a commitment to build anything.

You are being asked to derive a better data model and data flow. **You are not
constrained by the current design.** Everything below marked NEGOTIABLE can be
thrown away. Things marked MEASURED are facts that cost real experiments to
learn; a proposal that violates one must say so explicitly and argue why.

---

## 1. What the system is trying to do

Take a **minified JavaScript bundle** (10–32 MB, one file, names like `e`, `r`,
`n`) and emit a **readable source tree** (many files, meaningful names).

The hard part is not one release. It is that this runs on **every release of the
same product**, and the output is read as a **diff against the previous
release**. So the real goal is:

> The diff between humanified release N-1 and humanified release N should
> contain **only the changes that were actually made to the source**, and
> nothing caused by our own pipeline making a different arbitrary choice.

Every arbitrary choice — which name a variable gets, which file a function lands
in, what order statements appear in — must be **stable across releases unless
the underlying code changed**. Instability is called "noise" and it is the
thing the whole project measures.

## 2. The pipeline as it exists

Twelve stages, in order (from `docs/pipeline-stages.md`, which was checked
against the code):

1. Detect bundler/minifier
2. Select unpack adapter
3. Unpack the bundle
4. Detect third-party libraries
5. Name vendor files
6. Format (fixed point — output shape is deliberately not pluggable)
7. **Build the function graph**
8. **Match against the prior release**
9. **Name identifiers** (LLM + transfer from prior)
10. Place statements into files
11. Select split adapter
12. Emit to disk

Then three passes that run _after_ the tree looks finished:

- **post-split reconcile** — renames inside the emitted files
- **carry into bundle** — writes names back into `.humanify/humanified.js`,
  which becomes the NEXT release's prior. The only stage whose output is
  consumed by a future run.
- **finish on disk** — scaffold, relink, ledgers, stats

Stages 7–9 and the post-passes are where this brief is aimed.

## 3. The core problem to solve

**We use Babel's derived, per-crawl caches as our source of truth for two
questions they were never meant to answer:**

- _what IS this thing_ (identity of a binding, stable across time and across
  releases)
- _is this name already taken here_ (name allocation)

Babel gives us `Scope`, `Binding` and `NodePath` objects. These are **caches
built by a traversal**, keyed off the AST. Re-traversing builds _new objects for
the same source constructs_. They are mutable and we mutate them.

Concretely, today:

- the graph (stage 7) captures `NodePath`, `Scope`, and `Map<slot, Binding>`
- stage 8 (prior matching) parses the PREVIOUS release's bundle, then drops it
- dropping it leaves dead entries in Babel's global cache, which MEASURED
  causes an O(n²) 100%-CPU multi-hour hang on the next big traversal
  (exp030/031/032), so the cache is cleared
- clearing rebuilds every `Scope`/`Binding`/`NodePath` for the CURRENT AST too
- stage 9 then renames using a mix of stage-7 objects (pre-clear) and freshly
  crawled ones (post-clear)

So **two parallel object graphs describe the same source file at the same
time**, and a write through one is invisible through the other.

### Symptoms this has produced (all real, all measured)

- **A correctness bug in shipped output.** Two different variables in nested
  scopes both got the name `dirPath`, because each rename's safety check
  consulted a different object graph and neither saw the other. `a !== b`
  became `b !== b`. Fires on ~20% of runs of one release pair. Just fixed with
  a guard (a claim ledger keyed by the AST block node, which both graphs
  share) — but the guard treats the symptom, not the duplication.
- **Name churn**, still live: the mirror of the same staleness renames a
  binding twice, losing the first chosen name. Emits correct code, but the
  name differs from last release for no reason.
- Memory: the scope-resolved graph is ~GBs; the split stage must DROP the AST
  and re-parse the emitted code to avoid holding two graphs at once. Peak RSS
  is 21–29 GB and 14 GB OOMs.

## 4. What data actually has to move

This is the part to redesign. Currently much of it moves as _live Babel
objects_; it does not have to.

**Within a run, stage to stage:**

- for every renameable binding: where it is declared, everywhere it is used
  (reads AND writes — note `x |= 1` is tracked separately from a plain read),
  which scope owns it, which scopes nest inside it
- a structural fingerprint per function/binding, used to match against the
  prior release. MEASURED: it is deliberately NOT literal-preserving (string
  length + number magnitude only)
- the call graph between functions (drives both processing order and matching)
- for each name decision: which strategy decided it, and why (there are ~14
  strategy tiers; a diagnostic trail already records every attempt)
- which statements belong in which output file

**Across releases (the previous run's output is this run's input):**

- the previous release's emitted bundle, used to recover names for things that
  did not change
- a "ledger" recording which statement went to which file, so placement is
  stable
- MEASURED: top-level names must NOT be carried across releases — the export
  key is a string and 238/238 attempts drifted

**Constraints that are MEASURED, not preferences:**

- do not hold two full scope-resolved graphs live (memory)
- leaving a dropped AST's entries in Babel's global cache causes the O(n²) hang
- the LLM is nondeterministic: two identical cold runs disagree on ~33% of the
  bindings it decides, so anything LLM-derived cannot be assumed reproducible.
  Only ~3.6% of bindings are decided by the LLM at all
- names must be legal, not reserved, not a global builtin, and must not capture
  a reference that previously resolved elsewhere
- refusing a name does not make a problem go away — it relocates the collision.
  One attempt at a stricter rule cost +3,742 lines of noise

**NEGOTIABLE — assume nothing:**

- that Babel `Scope`/`Binding` objects are the working representation
- that renames mutate the AST in place as they are decided
- that the graph holds live Babel handles at all
- the stage boundaries themselves, and what is shared between them
- how prior-release information is represented and transported
- whether naming, matching and placement need the same representation

## 5. What to produce

1. **A data model** for program identity and name allocation that does not
   depend on which traversal built which object. What is the stable identity of
   a binding, within a run and across releases?
2. **A flow**: what is computed when, what crosses each boundary, what is
   allowed to be mutable.
3. **Where the current design's accidental complexity collapses** — name
   specific things that merge or disappear.
4. **The migration**, in shippable increments, each independently verifiable.
5. **What your design does NOT fix**, and what it would cost.

Be concrete and name the tradeoffs. A proposal that is merely "cleaner" without
naming what it makes measurably better, and what it makes worse, is not useful.

Ground every claim about the CURRENT code by reading it (repo root
`/Users/andrewgross/Development/humanify`). Do not trust this brief's summary
of the code over the code itself — if you find this brief is wrong about
something, say so, that is a valuable finding on its own.
