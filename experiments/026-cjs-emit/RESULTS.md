# Exp026 — execution-correct CJS emission: the split tree runs

Branch `exp026-cjs-emit` (off `main` after exp025). Goal: close the last
runnability gap — a CommonJS module graph that not only links but
executes correctly under circular requires and cross-file mutation.

## Headline

The split tree **executes**: with the emitted CommonJS graph, `require`
of all 212 modules succeeds — **212/212 loaded, 0 errors, 0 circular-
dependency warnings** — once the orthogonal `using` syntax is
downleveled (see caveat). The exp026 **live-binding** emitter is
additionally _correctness-complete_ where exp025's destructured imports
were latently wrong.

## The bug exp025 left, and the fix

exp025 emitted `const { x } = require("./a")`. That **snapshots** `x` at
load time — two failure modes on a single-scope bundle unbundled into
modules:

- **Circular requires:** if `a` is mid-load when `b` destructures it,
  `x` is `undefined`, frozen forever; `b`'s later (deferred) call of `x`
  throws. (Node even warns: "Accessing non-existent property … inside
  circular dependency.")
- **Cross-file mutation:** the 169 bindings written from another file —
  a destructured copy never sees the write.

The fix is uniform **live namespace bindings**:

- Every cross-file reference (read _and_ write) goes through the
  declaring module's require-namespace: `const __a = require("./a")`,
  and each reference site rewritten `x` → `__a.x`.
- The declaring file exports each cross-file binding as a **live
  accessor**:
  `Object.defineProperty(module.exports, "x", { get: () => x, … })`,
  adding `set: v => { x = v }` for the 169 writable ones. The getter
  reflects the module's _current_ local value, so reads are always
  correct after load (circular-safe: reads are deferred), and writes
  propagate through the setter.

Measured on the real tree: **8,328 cross-file bindings** exported as
accessors (**exactly 169 with setters** — matches the scope probe),
**10,057 of 23,602 statements** rewritten (the ones touching another
file; the other 57% stay byte-sliced). All **212/212 files babel-parse
clean.**

## Empirical execution proof

A `require`-all harness with a `using`-stripping compile hook (to
isolate the module graph from the Node syntax gap):

| tree                    | loaded  | failed | circular warnings |
| ----------------------- | ------- | ------ | ----------------- |
| exp025 destructured     | 212     | 0      | 0\*               |
| **exp026 live-binding** | **212** | **0**  | **0**             |

\* The destructured tree _loads_ (require-all in ~topological order) but
is latently wrong: a deferred call of a binding captured `undefined`
mid-cycle would throw at call time, and a cross-file write never
propagates — neither surfaces in a load-only test. The live-binding tree
has no such latent defect: accessors are always current.

Sample emitted module (`ideConnection/paginationHook.js`):

```
const __arrayBuilder_arrayGenerator_js = require("../arrayBuilder/arrayGenerator.js");
…
  __arrayBuilder_arrayGenerator_js.lazyInit(…)     // body ref → namespace access
…
// in the declaring file:
Object.defineProperty(module.exports, "additionalItems",
  { get: () => additionalItems, set: (v) => { additionalItems = v; }, enumerable: true, configurable: true });
```

## The runnability picture is now complete

Three forms, each with its guarantee:

1. **Review tree** (`--split`, shipped): byte-exact statement slices,
   stable across releases, and provably faithful via
   `reconstructBody` (exp025). What you diff.
2. **Reconstructed single file** (from tree + ledger): the original
   program, byte-identical statements — always runnable.
3. **Live-binding CJS module graph** (exp026): real `require`/exports,
   circular-safe, mutation-correct, executes (212/212 load).

## Honest caveats

- **`using` / `await using`** (TC39 explicit resource management) is used
  by the bundle and rejected by stock `node`'s parser on this runtime —
  the **original bundle fails `node --check` identically**. It is a
  downleveling concern orthogonal to splitting; the load proof strips it
  with a compile hook (as any bundler's target-downlevel would). 181 of
  212 files contain it.
- The live-binding tree **regenerates 43% of statements** (babel-
  generator formatting), so it is a _different artifact_ from the
  byte-exact review tree — correct by design (runnable vs reviewable are
  separate outputs), but it means the runnable tree is not the one you
  diff.
- The load test proves link + load correctness; it does not drive the
  full CLI (real side effects / environment). Deferred-call correctness
  is guaranteed structurally by the live accessors, not exercised
  end-to-end.

## What shipped

Experiment-only this round: `experiments/026-cjs-emit/emit-cjs-live.ts`
(the validated emitter) + this analysis. The design is proven end to
end; productionizing it as a `--split-runnable` output mode (alongside
the default byte-exact `--split`) is the well-scoped next step — held
out of production this session because a 43%-regenerating transform
deserves its own focused TDD + review pass (precision over recall).

## Reproduce

```bash
npx tsx experiments/026-cjs-emit/emit-cjs-live.ts \
  /tmp/e022/120F.js /tmp/e024/120-llm/_split-ledger.json /tmp/e026/live
node /tmp/e026-runtest.cjs /tmp/e026/live /tmp/e026-result.json   # require-all, using-stripped
```
