# Run B parse failures — detailed breakdown

`/tmp/exp013/cc-120/runtime.js` is the output of Run B (humanify v120 with
`--prior-version /tmp/exp013/cc-119/runtime.js`). Humanify reported
"Done!" cleanly. The diagnostics file confirms ~99.4% function rename
coverage with the prior-version cache.

But the output fails to parse with Babel. This document enumerates the
specific failures, the source pattern that produced each, and the
mechanism in the rename pipeline that let them through.

## TL;DR

| Failure                                | Mechanism                                                                             |
| -------------------------------------- | ------------------------------------------------------------------------------------- |
| Duplicate identifier `NH`              | Two structurally distinct arrow functions in the same scope both renamed to `NH`.     |
| Reserved keyword `delete` as parameter | A name carried over from the prior version was applied without a reserved-word check. |

Both failures originate in the cross-version transfer path
(`src/cache/prior-version.ts` + downstream), not in the LLM-rename path.
Run A (fresh LLM, no prior version) produced 0 module-binding failures
and a parseable output. Run B's transfer applies names without the
same identifier-validity and scope-collision checks the LLM path runs.

## Failure 1 — Duplicate identifier `NH`

### Location

`/tmp/exp013/cc-120/runtime.js`, the same lexical block contains:

```js
// line 350554
let NH = () => {
  setCurrentView({
    type: "mcp-tools",
    client: iH
  });
};
let goBackToPluginList = () => {
  setCurrentView("plugin-list");
};
// line 350564
let NH = (pluginKey) => {
  if (pluginKey) {
    setResultFn(pluginKey);
  }
  setCurrentView("plugin-list");
};
```

### Source pattern in v120

Both arrow functions are nested inside an `if (typeof currentView === "object" && currentView.type === "mcp-detail")` block. They are structurally distinct:

- The first takes **no arguments** and navigates to the MCP tools view.
- The second takes **one argument** (`pluginKey`) and conditionally navigates to the plugin list.

A correct humanification would have given them distinct names — e.g.,
`openMcpTools` and `selectPluginByKey`.

### Mechanism

The most likely cause is **prior-version structural matching that
considered both v120 callbacks "close enough" to a single v119 function
called `NH`**. The transfer code path:

1. v119 had at least one function whose final humanified name was `NH`.
2. v120 has two callbacks in the same scope that both match that v119
   function's structural fingerprint (or close-match shingle).
3. The transfer applied `NH` to both without checking whether the same
   target name had already been used in the destination scope.

The fresh-LLM path catches this because the rename processor runs
`scope.rename(oldName, newName)` which would error on collision, and
the processor retries with an alternate name. The transfer path appears
to set names directly on the binding, bypassing the rename machinery's
collision check.

### Why it slipped past validation

- Diagnostics report `failed: 0` for module bindings and 3 failures for
  functions in Run A. Run B's diagnostics show `notRenamed: 2` for
  module bindings — but a duplicate is not a "not renamed" outcome, it's
  a wrong-name outcome that wasn't tracked.
- Humanify uses `generate(ast)` to write the output without re-parsing
  it. The duplicate `let` is a static error that only surfaces at parse
  time of the generated string.

## Failure 2 — Reserved keyword `delete` as parameter name

### Location

`/tmp/exp013/cc-120/runtime.js` around line 416094:

```js
function collection(key, delete) {
  AGENT_MAP.set(key, delete);
}
function removeAgent(agent) {
  AGENT_MAP.delete(agent);
}
```

### Source pattern in v120

The body uses `AGENT_MAP.set(key, value)` — a standard `Map.set` call.
The original minified parameter names were obfuscated. The function was
correctly recognized as "store an agent in the map" — the immediate
neighbor `removeAgent` clarifies the intent.

A correct humanification would have used a name like `agent`,
`subagentId`, or `value` for the second parameter.

### Mechanism

The transferred name `delete` almost certainly came from a v119 function
where `delete` appeared as a property/method name (e.g.,
`AGENT_MAP.delete(key)` — the legitimate `Map.prototype.delete` method
reference, which v119 might have humanified as a function called
`delete` because the LLM/transfer saw it as a method-like identifier).

The transfer path lifted that name and applied it to v120's parameter
position — where `delete` is a reserved keyword and not a legal
identifier.

### Why it slipped past validation

- The LLM-rename path explicitly bans `delete` and other reserved words
  via the system prompt ("Never shadow global built-in names" — and
  reserved-word validation likely lives in the response validator too).
- The transfer path doesn't run names through the same validator. It
  trusts the prior-version output to be already-valid identifiers, but
  doesn't account for the fact that a valid identifier in one binding
  position (e.g., as a property name in `obj.delete`) may not be valid
  in another position (e.g., as a parameter name).

## What we'll do next

This document is the spec for two follow-up fixes, both in the
prior-version transfer path:

1. **Reserved-word check**: before applying a transferred name to any
   binding, validate it with the same identifier-validity check the
   LLM-rename path uses.
2. **Scope-collision check**: before applying a transferred name to a
   binding's scope, verify the target name is not already in use in
   that scope. On collision, either skip the transfer (leave the
   binding for the LLM pass) or apply a suffix.

But before fixing the underlying code, we will **post-hoc patch the
two known issues** in the existing output file so we can perform the
`diff -r` analysis and learn what the rest of the cross-version diff
looks like. The patched file is a one-off — the real fix happens in the
code afterward.

## Patches applied to make the output parse

Original output: `/tmp/exp013/cc-120/runtime.js` (untouched, fails to parse).
Patched copy: `/tmp/exp013/cc-120/runtime.patched.js` (parses OK).

Two minimal text substitutions. No additional parse errors were hiding
behind the two known ones — the patched file parses cleanly at
21,813,371 bytes.

### Patch 1 — `delete` keyword

```diff
   function collection(key, delete) {
-    AGENT_MAP.set(key, delete);
+  function collection(key, _delete) {
+    AGENT_MAP.set(key, _delete);
   }
```

Underscore-prefixed the parameter and its single use in the body. The
new name is structurally close to the original v119 transfer choice
(so we keep the "this came from a delete-named binding" signal in the
diff) while being a legal identifier.

### Patch 2 — Duplicate `NH`

```diff
   let goBackToPluginList = () => {
     setCurrentView("plugin-list");
   };
-  let NH = pluginKey => {
+  let NH_dup = pluginKey => {
     if (pluginKey) {
       setResultFn(pluginKey);
     }
     setCurrentView("plugin-list");
   };
```

Renamed only the second `let NH = ...` declaration. The nine downstream
references `onViewTools: NH` and `onComplete: NH` in the surrounding
`if`-block are LEFT POINTING AT THE FIRST `NH` (the no-arg
mcp-tools-navigator). This may be semantically wrong for `onComplete`
(which probably should have bound to the `pluginKey =>` arrow) but it's
parseable and the diff will surface the intent ambiguity directly.

This is a minimal post-hoc patch, **not** a fix in the rename pipeline.
The underlying transfer-validation gaps still need code fixes — see
"What we'll do next" above.

## Cross-version diff analysis (after patch)

The patched `runtime.js` parses cleanly, so we can read the diff.

```
Total v119 lines: 524,604
Total v120 lines: 527,714
diff -r runtime.js:  167,944 lines  /  30,745 hunks
```

### Hunk classification

| Hunk type                                                     | Count  | % of hunks |
| ------------------------------------------------------------- | ------ | ---------- |
| 1↔1 line, **pure cosmetic rename** (only identifiers differ) | 20,503 | 66.7 %     |
| 1↔1 line, real code change                                   | 509    | 1.7 %      |
| Multi-line hunks (mixed)                                      | ~9,733 | ~31.6 %    |

**At least two-thirds of the diff is pure cosmetic rename noise.** The
real-signal hunk count is on the order of 1,000–5,000 once the noise
is subtracted.

### Where the noise comes from

Run B's diagnostics tell the story precisely:

| Category        | Total  | Cache-reused | Fresh LLM | Cache rate |
| --------------- | ------ | ------------ | --------- | ---------- |
| Functions       | 43,198 | 42,930       | 6,255     | **99.4 %** |
| Module bindings | 17,976 | 13,302       | 4,672     | **74 %**   |

The 4,672 module bindings that got fresh LLM names in v120 are the
source of nearly all the noise. Sample diff hunks confirm this:

```diff
< var initializeArrayHelpers = lazyInitializer(() => {
> var configureArrayHelpers = lazyInitializer(() => {

< var getEntry;
< var initializeKey = lazyInitializer(() => {
> var get;
> var exportEbK = lazyInitializer(() => {

< var hasEntry;
> var has;
```

The two sides are identical polyfill code for some Set/Map helper.
v119 named these `initializeArrayHelpers`, `getEntry`, `initializeKey`,
`hasEntry`. v120 named them `configureArrayHelpers`, `get`, `exportEbK`,
`has`. The structural-hash + close-match machinery that gives functions
99.4% cross-version stability is **not currently applied to module
bindings** (or applied at a much weaker level), so the LLM gets a
fresh shot at each unmatched binding and produces different choices.

### Real signal that IS visible

The diff is still actionable. Example hunks that ARE real source changes:

- v120 adds `getVersionString`, `exceptionConstructor`, `ensureAIAgentEnv` —
  containing literal `VERSION: "2.1.120"` and `BUILD_TIME: "2026-04-24T19:00:49Z"`.
- v119 had Symbol.dispose/asyncDispose helpers (`pushDisposable`,
  `runFinalizers`) that v120 removed. Probably a TypeScript-target /
  esbuild-helper-emit change between releases.

### The real follow-up work for noise reduction

The transfer-validation bugs from the first half of this document
(`NH` collision, `delete` keyword) are correctness fixes — they unblock
parsing. They are NOT what's driving the 167K-line diff.

The noise-floor reduction needs:

1. **Apply structural-hash matching to module bindings.** Today's
   match cascade works for FunctionNode but is weaker for
   ModuleBindingNode. Bringing module-binding match parity up to 99 %+
   would slash the diff size by ~10x.
2. **Encourage close-match transfer for module bindings.** Even when
   the structural hash doesn't match exactly, callee/caller-shape
   propagation should bring the right name across versions.

This is bigger than exp013 — file follow-up issues for both, then
plan a Phase 2 experiment after they land.
