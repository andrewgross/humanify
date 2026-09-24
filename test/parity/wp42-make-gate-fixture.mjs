// Cut the committed prompt-gate fixture (test/parity/wp42-gate-fixture/capture/;
// ONE subdir so the rust:parity stage reads it as a one-sided fixture)
// from one pair's oracle dump + WP4.2 capture: a handful of rows per
// section, chosen so every gated path is present (first-round, retry with
// body, module first-round + retry, a windowed and a
// capped code window, a context with callees and one with context vars).
// Rows are copied byte-for-byte; scope snapshots are kept only when small.
//
//   node test/parity/wp42-make-gate-fixture.mjs <dumpDir> <rowsDir> <outDir>
import fs from "node:fs";
import path from "node:path";

const [dumpDir, rowsDir, outDir] = process.argv.slice(2);
const lines = (f) => fs.readFileSync(f, "utf8").split("\n").filter(Boolean);
const MODULE_PREFIX =
  "You are an expert JavaScript developer helping to deobfuscate minified code.\n\nYour task is to analyze top-level";

const prompts = lines(path.join(dumpDir, "prompts.jsonl"));
const keys = lines(path.join(dumpDir, "cache-keys.jsonl"));
const requests = lines(path.join(rowsDir, "requests.jsonl"));

const picked = new Set();
const want = {
  first: (p, k) => !p.isRetry && !k.request.userPrompt,
  retryBody: (p, k) =>
    p.isRetry && k.request.promptBody && !k.request.userPrompt,
  moduleFirst: (p, k) =>
    k.request.systemPrompt?.startsWith(MODULE_PREFIX) && !p.isRetry,
  moduleRetryBody: (p, k) =>
    k.request.systemPrompt?.startsWith(MODULE_PREFIX) &&
    p.isRetry &&
    k.request.promptBody
};
// The SMALLEST matching row per path (keeps the fixture small).
for (const test of Object.values(want)) {
  let best = -1;
  for (let i = 0; i < prompts.length; i++) {
    const p = JSON.parse(prompts[i]);
    const k = JSON.parse(keys[i]);
    if (test(p, k) && (best < 0 || keys[i].length < keys[best].length))
      best = i;
  }
  if (best >= 0) picked.add(best);
}
const idx = [...picked].sort((a, b) => a - b);
fs.mkdirSync(path.join(outDir, "dump"), { recursive: true });
fs.mkdirSync(path.join(outDir, "rows"), { recursive: true });
const write = (f, rows) =>
  fs.writeFileSync(f, rows.map((r) => `${r}\n`).join(""));
write(
  path.join(outDir, "dump/prompts.jsonl"),
  idx.map((i) => prompts[i])
);
write(
  path.join(outDir, "dump/cache-keys.jsonl"),
  idx.map((i) => keys[i])
);
write(
  path.join(outDir, "rows/requests.jsonl"),
  idx.map((i) => requests[i])
);

const builders = lines(path.join(rowsDir, "module-builders.jsonl"));
const byFn = (fn) =>
  builders.find((l) => JSON.parse(l).fn === fn && l.length < 20000);
write(
  path.join(outDir, "rows/module-builders.jsonl"),
  [
    "buildModuleLevelRenamePrompt",
    "buildModuleLevelRenameBody",
    "buildModuleLevelRetryPrefix"
  ].map(byFn)
);

const windows = lines(path.join(rowsDir, "code-window.jsonl")).map((l) => [
  l,
  JSON.parse(l)
]);
const small = windows.find(
  ([, d]) => d.fn === "selectFunctionCode" && d.out === d.sel.code
)[0];
const windowed = windows
  .filter(([, d]) => d.fn === "selectFunctionCode" && d.out !== d.sel.code)
  .sort((a, b) => a[0].length - b[0].length)[0][0];
const capped = windows
  .filter(([, d]) => d.fn === "capContextCode" && d.out !== d.code)
  .sort((a, b) => a[0].length - b[0].length)[0][0];
write(path.join(outDir, "rows/code-window.jsonl"), [small, windowed, capped]);

const scopes = new Map(
  lines(path.join(rowsDir, "scopes.jsonl")).map((l) => [JSON.parse(l).id, l])
);
const refsOf = (v) => [
  v.programScope,
  ...v.scopeChain.filter((e) => !Array.isArray(e)).map((e) => e.ref)
];
// Every function inside the Bun module wrapper references the wrapper's
// ~25k-name scope (one ~0.5 MB snapshot): pick two rows sharing ONE.
const contexts = lines(path.join(rowsDir, "context.jsonl")).map((l) => [
  l,
  JSON.parse(l)
]);
const withVars = contexts.find(([, d]) => d.out.contextVars);
const shared = new Set(withVars ? refsOf(withVars[1].view) : []);
const withCallees = contexts.find(
  ([, d]) =>
    d.out.calleeSignatures.length > 0 &&
    refsOf(d.view).every((id) => shared.has(id))
);
const ctxRows = [withCallees, withVars].filter(Boolean);
write(
  path.join(outDir, "rows/context.jsonl"),
  ctxRows.map(([l]) => l)
);
const ids = new Set(ctxRows.flatMap(([, d]) => refsOf(d.view)));
write(
  path.join(outDir, "rows/scopes.jsonl"),
  [...ids].sort((a, b) => a - b).map((id) => scopes.get(id))
);
console.log(
  `prompts ${idx.length}, contexts ${ctxRows.length} (callees ${!!withCallees}, vars ${!!withVars}), scopes ${ids.size}`
);
