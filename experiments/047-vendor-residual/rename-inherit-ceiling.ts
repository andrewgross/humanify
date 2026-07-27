/**
 * Task 1 / task 3 ceiling — of the same-module pairs whose humanify FILENAME
 * rotated, how many are the same PROGRAM?
 *
 * `vendor-body-inherit.ts` is strictly path-keyed: `bytesFor` reads
 * `path.join(priorRoot, relPath)`, so when a filename rotates the prior body is
 * never found and the file is charged as a full delete plus a full add. A pair
 * that is the same program and merely renamed is therefore 100% noise that the
 * shipped lever cannot currently see.
 *
 * This applies the SHIPPED decision function -- the skeleton pre-filter and
 * `computeStructuralSignature`, literal-PRESERVING, the same key
 * `vendor-body-inherit` uses -- to each matched pair, so the ceiling is measured
 * with the instrument that would do the work rather than a proxy for it
 * (measurement-pitfalls rule 4).
 *
 * Require paths are IN that signature by design, so a pair that also changed
 * DEPTH (`vendor/x.js` <-> `vendor/x/lib_h.js`) differs in its require header
 * even when the payload is identical. Those are reported as their own class:
 * inheritable only if the header is rewritten, which is a different and larger
 * change than reusing bytes.
 */
import { readFileSync } from "node:fs";
import type { NodePath } from "@babel/traverse";
import { computeStructuralSignature } from "../../src/analysis/structural-hash.js";
import { parseSourceAst, traverse } from "../../src/babel-utils.js";

const IDENTIFIER_RUN = /[A-Za-z_$][A-Za-z0-9_$]*/g;
const skeleton = (code: string): string => code.replace(IDENTIFIER_RUN, "");

function fileSignature(code: string): string | null {
  try {
    const ast = parseSourceAst(code);
    if (!ast) return null;
    let sig: string | null = null;
    traverse(ast, {
      Program(p: NodePath) {
        sig = computeStructuralSignature(p);
        p.stop();
      }
    });
    return sig;
  } catch {
    return null;
  }
}

/** Depth of a vendor-relative path, i.e. how many `../` its require header uses. */
const depth = (rel: string): number => rel.split("/").length;

/** Normalize the intra-tree require header away, to separate payload from depth. */
const maskRequireDepth = (code: string): string =>
  code.replace(/(\.\.\/)+\.humanify\//g, "__HUMANIFY__/");

const [aRoot, bRoot, pairsFile, label = "hop"] = process.argv.slice(2);
if (!aRoot || !bRoot || !pairsFile) {
  console.error(
    "usage: rename-inherit-ceiling.ts <priorVendorDir> <freshVendorDir> <pairsTsv> [label]"
  );
  process.exit(1);
}

const pairs = readFileSync(pairsFile, "utf8")
  .split("\n")
  .map((l) => l.trim())
  .filter((l) => l.length > 0 && !l.startsWith("#"))
  .map((l) => l.split("\t"))
  .filter((p): p is [string, string] => p.length === 2);

const lineCount = (t: string) =>
  t.split("\n").filter((l, i, a) => i < a.length - 1 || l !== "").length;

type Class =
  | "same program, same depth"
  | "same program, depth changed"
  | "different program";

const rows: { a: string; b: string; cls: Class; lines: number }[] = [];

for (const [aRel, bRel] of pairs) {
  let aText: string;
  let bText: string;
  try {
    aText = readFileSync(`${aRoot}/${aRel}`, "utf8");
    bText = readFileSync(`${bRoot}/${bRel}`, "utf8");
  } catch {
    continue;
  }
  const charged = lineCount(aText) + lineCount(bText);

  // The shipped key, verbatim.
  const shippedMatch =
    skeleton(aText) === skeleton(bText) &&
    (() => {
      const s = fileSignature(aText);
      return s !== null && s === fileSignature(bText);
    })();

  if (shippedMatch) {
    rows.push({
      a: aRel,
      b: bRel,
      cls: "same program, same depth",
      lines: charged
    });
    continue;
  }

  // Same payload but a different require depth?
  const am = maskRequireDepth(aText);
  const bm = maskRequireDepth(bText);
  const maskedMatch =
    skeleton(am) === skeleton(bm) &&
    (() => {
      const s = fileSignature(am);
      return s !== null && s === fileSignature(bm);
    })();

  if (maskedMatch && depth(aRel) !== depth(bRel)) {
    rows.push({
      a: aRel,
      b: bRel,
      cls: "same program, depth changed",
      lines: charged
    });
  } else if (maskedMatch) {
    rows.push({
      a: aRel,
      b: bRel,
      cls: "same program, same depth",
      lines: charged
    });
  } else {
    rows.push({ a: aRel, b: bRel, cls: "different program", lines: charged });
  }
}

const by = (c: Class) => rows.filter((r) => r.cls === c);
const sum = (rs: typeof rows) => rs.reduce((n, r) => n + r.lines, 0);

console.log(
  `# ${label} — rename/inherit ceiling over ${pairs.length} same-module pairs`
);
console.log("");
console.log("| class | pairs | lines charged |");
console.log("| ----- | ----: | ------------: |");
for (const c of [
  "same program, same depth",
  "same program, depth changed",
  "different program"
] as Class[]) {
  console.log(`| ${c} | ${by(c).length} | ${sum(by(c))} |`);
}
console.log(`| **TOTAL** | **${rows.length}** | **${sum(rows)}** |`);
console.log("");
console.log(
  `Recoverable by making inheritance CONTENT-keyed instead of path-keyed: ` +
    `${sum(by("same program, same depth"))} lines (${by("same program, same depth").length} pairs).`
);
console.log(
  `Additionally recoverable only by also rewriting the require header: ` +
    `${sum(by("same program, depth changed"))} lines (${by("same program, depth changed").length} pairs).`
);
console.log("");
for (const c of [
  "same program, same depth",
  "same program, depth changed"
] as Class[]) {
  const rs = by(c);
  if (rs.length === 0) continue;
  console.log(`## ${c} (${rs.length})`);
  console.log("");
  for (const r of rs.sort((x, y) => y.lines - x.lines)) {
    console.log(`- ${r.lines} ln  ${r.a} -> ${r.b}`);
  }
  console.log("");
}
