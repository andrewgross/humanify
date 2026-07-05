/**
 * Mechanism classifier for the close-match anomaly (task 2 of the
 * investigation). For every prior function whose structural hash CHANGED
 * between BEAUTIFIED v119 and humanified v119 (same beautified AST, the
 * only difference is renaming; aligned by traversal index), diff the
 * identifier-occurrence sequences and classify:
 *
 *   SPLIT — two occurrences share a name in beautified but not humanified
 *           (minifier name-reuse diversified by renaming)
 *   MERGE — two occurrences differ in beautified but share in humanified
 *           (LLM gave distinct bindings the same name, or a renamed
 *           binding collided with a stable property/object-key name)
 *
 * Placeholder assignment is name-keyed (structural-hash.ts normalizeAST),
 * so either direction changes the placeholder sequence → different hash.
 * Property/object-key positions are tracked to size the collision class.
 *
 * Usage:
 *   node --max-old-space-size=16384 --import tsx/esm \
 *     experiments/013-bun-cjs-classification/classify-anomaly-mechanisms.ts
 */
import fs from "node:fs";
import { parseSync } from "@babel/core";
import type { NodePath } from "@babel/traverse";
import * as t from "@babel/types";
import { computeStructuralHash } from "../../src/analysis/structural-hash.js";
import { traverse } from "../../src/babel-utils.js";

// Beautified (pipeline-shaped) v119 — produced by measure-close-match-anomaly.ts prep
const BEAUTIFIED_V119 = "/tmp/exp013-anomaly/v119-beautified.js";
const HUMANIFIED_V119 = "/tmp/exp013-phase2/cc-119/runtime.js";

interface Occurrence {
  name: string;
  /** Non-computed member property or non-computed object key position */
  propertyPosition: boolean;
}

/** Collect all function paths in deterministic traversal order. */
function collectFunctions(code: string): NodePath<t.Function>[] {
  const ast = parseSync(code, { sourceType: "unambiguous" });
  if (!ast) throw new Error("parse failed");
  const fns: NodePath<t.Function>[] = [];
  traverse(ast, {
    Function(p: NodePath<t.Function>) {
      fns.push(p);
    }
  });
  return fns;
}

/**
 * Identifier occurrences in normalizeAST's visit order (visitChildren =
 * Object.keys insertion order), tagging property/object-key positions.
 */
function occurrenceSequence(fn: t.Function): Occurrence[] {
  const out: Occurrence[] = [];
  const SKIP = new Set(["type", "loc", "start", "end"]);
  function visit(node: t.Node, propertyPosition: boolean): void {
    if (t.isIdentifier(node)) {
      out.push({ name: node.name, propertyPosition });
      return;
    }
    for (const key of Object.keys(node)) {
      if (SKIP.has(key)) continue;
      const value = (node as unknown as Record<string, unknown>)[key];
      const childIsPropertyPos =
        (t.isMemberExpression(node) && !node.computed && key === "property") ||
        (t.isObjectProperty(node) && !node.computed && key === "key") ||
        (t.isObjectMethod(node) && !node.computed && key === "key") ||
        (t.isClassMethod(node) && !node.computed && key === "key") ||
        (t.isClassProperty(node) && !node.computed && key === "key") ||
        (t.isOptionalMemberExpression(node) &&
          !node.computed &&
          key === "property");
      if (Array.isArray(value)) {
        for (const item of value) {
          if (item && typeof item === "object" && "type" in item) {
            visit(item as t.Node, false);
          }
        }
      } else if (value && typeof value === "object" && "type" in value) {
        visit(value as t.Node, childIsPropertyPos);
      }
    }
  }
  visit(fn, false);
  return out;
}

interface Classification {
  splits: number;
  merges: number;
  mergesInvolvingPropertyPosition: number;
  structureDiffers: boolean;
}

/** Compare same-structure occurrence sequences; classify divergences. */
function classifyPair(raw: Occurrence[], hum: Occurrence[]): Classification {
  const res: Classification = {
    splits: 0,
    merges: 0,
    mergesInvolvingPropertyPosition: 0,
    structureDiffers: raw.length !== hum.length
  };
  if (res.structureDiffers) return res;

  // First-occurrence partner maps in both directions. A name-keyed
  // placeholder scheme is exactly "same name ⇔ same placeholder", so:
  //   raw same, hum different → split; raw different, hum same → merge.
  const rawFirst = new Map<string, number>();
  const humFirst = new Map<string, number>();
  for (let i = 0; i < raw.length; i++) {
    const r = raw[i].name;
    const h = hum[i].name;
    const rPrev = rawFirst.get(r);
    const hPrev = humFirst.get(h);
    if (rPrev === undefined) rawFirst.set(r, i);
    if (hPrev === undefined) humFirst.set(h, i);

    // Occurrence i shares a raw name with occurrence rPrev — do they share
    // in humanified too?
    if (rPrev !== undefined && hum[rPrev].name !== h) res.splits++;
    // Occurrence i shares a humanified name with occurrence hPrev — did
    // they share in raw?
    if (hPrev !== undefined && raw[hPrev].name !== r) {
      res.merges++;
      if (raw[i].propertyPosition || raw[hPrev].propertyPosition) {
        res.mergesInvolvingPropertyPosition++;
      }
    }
  }
  return res;
}

async function main() {
  const t0 = Date.now();
  console.log("collecting beautified v119 functions...");
  const beauFns = collectFunctions(fs.readFileSync(BEAUTIFIED_V119, "utf-8"));
  console.log(`  ${beauFns.length}`);
  console.log("collecting humanified v119 functions...");
  const humFns = collectFunctions(fs.readFileSync(HUMANIFIED_V119, "utf-8"));
  console.log(`  ${humFns.length}`);

  if (beauFns.length !== humFns.length) {
    console.log(
      "FUNCTION COUNTS DIFFER — index alignment unreliable, aborting. " +
        "Renaming should preserve function count; investigate first."
    );
    process.exit(2);
  }

  let hashSame = 0;
  const changed: { idx: number; cls: Classification }[] = [];
  let structureDiffers = 0;
  let splitOnly = 0;
  let mergeOnly = 0;
  let both = 0;
  let neither = 0;
  let propInvolved = 0;

  for (let i = 0; i < beauFns.length; i++) {
    const hRaw = computeStructuralHash(beauFns[i]);
    const hHum = computeStructuralHash(humFns[i]);
    if (hRaw === hHum) {
      hashSame++;
      continue;
    }
    const cls = classifyPair(
      occurrenceSequence(beauFns[i].node),
      occurrenceSequence(humFns[i].node)
    );
    changed.push({ idx: i, cls });
    if (cls.structureDiffers) {
      structureDiffers++;
      continue;
    }
    const hasSplit = cls.splits > 0;
    const hasMerge = cls.merges > 0;
    if (hasSplit && hasMerge) both++;
    else if (hasSplit) splitOnly++;
    else if (hasMerge) mergeOnly++;
    else neither++;
    if (cls.mergesInvolvingPropertyPosition > 0) propInvolved++;
    if ((i & 0xfff) === 0) process.stdout.write(".");
  }

  console.log(
    `\n\n=== HASH STABILITY beautified↔humanified v119 (${beauFns.length} fns) ===`
  );
  console.log(`  hash same:    ${hashSame}`);
  console.log(
    `  hash changed: ${changed.length} (${((100 * changed.length) / beauFns.length).toFixed(1)}%)`
  );
  console.log("\n=== MECHANISM among changed ===");
  console.log(`  split only:            ${splitOnly}`);
  console.log(`  merge only:            ${mergeOnly}`);
  console.log(`  both split and merge:  ${both}`);
  console.log(`  neither (unexplained): ${neither}`);
  console.log(
    `  structure differs (occurrence count changed): ${structureDiffers}`
  );
  console.log(
    `  merges involving property/object-key position: ${propInvolved}`
  );

  // Samples of each class for manual inspection
  const sample = (pred: (c: Classification) => boolean, n: number) =>
    changed.filter((c) => pred(c.cls)).slice(0, n);
  console.log("\n=== SAMPLE indices (traversal order) ===");
  console.log(
    "  split-only:",
    sample((c) => c.splits > 0 && c.merges === 0 && !c.structureDiffers, 8).map(
      (c) => c.idx
    )
  );
  console.log(
    "  merge-only:",
    sample((c) => c.merges > 0 && c.splits === 0 && !c.structureDiffers, 8).map(
      (c) => c.idx
    )
  );
  console.log(
    "  neither:",
    sample(
      (c) => c.merges === 0 && c.splits === 0 && !c.structureDiffers,
      8
    ).map((c) => c.idx)
  );
  console.log(
    "  structure-differs:",
    sample((c) => c.structureDiffers, 8).map((c) => c.idx)
  );
  console.log(`\ntotal ${((Date.now() - t0) / 1000).toFixed(1)}s`);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
