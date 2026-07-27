/**
 * Task 1, the decisive instrument — match added/removed vendor files by the
 * identity the FILE ITSELF declares, not by a name humanify chose.
 *
 * highlight.js language grammars are `{ name: "ARM Assembly", aliases: ["arm"],
 * ... }`. That name/alias pair is written by the library author, survives
 * minification (it is a string literal), and is stable across highlight.js
 * versions. It is the closest thing to a ground-truth identity this tree has.
 *
 * What this predicate actually tests (rule 3, in one sentence): a removed and an
 * added file are the SAME MODULE iff they declare the same `name:` literal, or —
 * when neither declares a name — the same non-empty `aliases:` list.
 *
 * That is an identity claim about the library, and it is independent of every
 * name humanify assigns (`lib_<structuralHash8>`, the vendor namer's guess, and
 * the singleton-vs-directory layout), which is what makes it able to answer the
 * question exp046 left open.
 *
 * Files with no declared identity are reported separately and NOT guessed at.
 */
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, relative } from "node:path";
import { jaccard, literalSet } from "./literals.js";

function walk(dir: string): string[] {
  const out: string[] = [];
  for (const e of readdirSync(dir)) {
    const p = join(dir, e);
    if (statSync(p).isDirectory()) out.push(...walk(p));
    else if (p.endsWith(".js")) out.push(p);
  }
  return out;
}

/** The grammar's self-declared `name:"..."`, if it has one. */
function declaredName(text: string): string | null {
  const m = /\bname:\s*"((?:[^"\\]|\\.)+)"/.exec(text);
  return m?.[1] ?? null;
}

/** The grammar's self-declared `aliases:[...]` string list, sorted+joined. */
function declaredAliases(text: string): string | null {
  const m = /\baliases:\s*\[([^\]]*)\]/.exec(text);
  if (!m?.[1]) return null;
  const items = [...m[1].matchAll(/"((?:[^"\\]|\\.)*)"/g)].map((x) => x[1]);
  if (items.length === 0) return null;
  return items.sort().join(",");
}

type Entry = {
  rel: string;
  name: string | null;
  aliases: string | null;
  bytes: number;
  lits: Set<string>;
};

function load(root: string): Map<string, Entry> {
  const m = new Map<string, Entry>();
  for (const p of walk(root)) {
    const text = readFileSync(p, "utf8");
    m.set(relative(root, p), {
      rel: relative(root, p),
      name: declaredName(text),
      aliases: declaredAliases(text),
      bytes: text.length,
      lits: literalSet(text)
    });
  }
  return m;
}

/** Identity key: declared name wins, else the alias list, else none. */
function idKey(e: Entry): string | null {
  if (e.name !== null) return `name:${e.name}`;
  if (e.aliases !== null) return `alias:${e.aliases}`;
  return null;
}

const [aRoot, bRoot, label = "hop"] = process.argv.slice(2);
if (!aRoot || !bRoot) {
  console.error(
    "usage: grammar-identity.ts <priorVendorDir> <freshVendorDir> [label]"
  );
  process.exit(1);
}

const A = load(aRoot);
const B = load(bRoot);
const removed = [...A.values()].filter((e) => !B.has(e.rel));
const added = [...B.values()].filter((e) => !A.has(e.rel));

// Index the ADDED files by declared identity. Duplicate identities are kept as
// lists -- a grammar can legitimately appear twice in one bundle.
const addedById = new Map<string, Entry[]>();
for (const e of added) {
  const k = idKey(e);
  if (k === null) continue;
  const list = addedById.get(k);
  if (list) list.push(e);
  else addedById.set(k, [e]);
}

type Row = { from: Entry; to: Entry; key: string; jac: number };
const matched: Row[] = [];
const unmatchedRemoved: Entry[] = [];
for (const r of removed) {
  const k = idKey(r);
  const pool = k === null ? undefined : addedById.get(k);
  if (!k || !pool || pool.length === 0) {
    unmatchedRemoved.push(r);
    continue;
  }
  // Among same-identity candidates, take the closest by literal overlap.
  let bestIdx = 0;
  let bestJac = -1;
  for (let i = 0; i < pool.length; i++) {
    const j = jaccard(r.lits, pool[i]!.lits);
    if (j > bestJac) {
      bestJac = j;
      bestIdx = i;
    }
  }
  const to = pool.splice(bestIdx, 1)[0]!;
  matched.push({ from: r, to, key: k, jac: bestJac });
}
const unmatchedAdded = added.filter((e) => {
  const k = idKey(e);
  return !matched.some((m) => m.to.rel === e.rel);
});

console.log(
  `# ${label} — added/removed vendor files matched by DECLARED identity`
);
console.log("");
console.log(`| metric | files |`);
console.log(`| ------ | ----: |`);
console.log(`| vendor files, prior | ${A.size} |`);
console.log(`| vendor files, fresh | ${B.size} |`);
console.log(`| removed (path-keyed) | ${removed.length} |`);
console.log(`| added (path-keyed) | ${added.length} |`);
console.log(
  `| **matched: SAME declared identity, renamed** | **${matched.length}** |`
);
console.log(`| removed with no counterpart | ${unmatchedRemoved.length} |`);
console.log(`| added with no counterpart | ${unmatchedAdded.length} |`);
console.log("");

// Of the matched pairs, how many are byte-identical payloads vs edited?
const identical = matched.filter(
  (m) => m.jac === 1 && m.from.bytes === m.to.bytes
);
const litIdentical = matched.filter((m) => m.jac === 1);
console.log(`Of the ${matched.length} renamed-same-module pairs:`);
console.log(
  `  identical literals AND identical byte length: ${identical.length}`
);
console.log(
  `  identical literal set (payload edited, same strings): ${litIdentical.length}`
);
console.log(
  `  literal jaccard >= 0.9: ${matched.filter((m) => m.jac >= 0.9).length}`
);
console.log(
  `  literal jaccard >= 0.5: ${matched.filter((m) => m.jac >= 0.5).length}`
);
console.log("");

console.log(
  "## matched pairs (same declared identity, different humanify filename)"
);
console.log("");
console.log("| jac | priorB | freshB | identity | removed -> added |");
console.log("| --: | -----: | -----: | -------- | ---------------- |");
for (const m of matched.sort((x, y) => y.jac - x.jac)) {
  console.log(
    `| ${m.jac.toFixed(2)} | ${m.from.bytes} | ${m.to.bytes} | ${m.key} | ${m.from.rel} -> ${m.to.rel} |`
  );
}
console.log("");
console.log("## removed, NO counterpart by declared identity");
console.log("");
for (const e of unmatchedRemoved.sort((x, y) => y.bytes - x.bytes)) {
  console.log(`- ${e.rel} (${e.bytes} B, id=${idKey(e) ?? "NONE"})`);
}
console.log("");
console.log("## added, NO counterpart by declared identity");
console.log("");
for (const e of unmatchedAdded.sort((x, y) => y.bytes - x.bytes)) {
  console.log(`- ${e.rel} (${e.bytes} B, id=${idKey(e) ?? "NONE"})`);
}
