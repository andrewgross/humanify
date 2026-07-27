/**
 * Task 1 helper — put two vendor files side by side so a human can answer
 * "is this the same library?" It does NOT decide anything.
 *
 * Prints, for each side: byte size, literal-set size, the literals unique to
 * that side (the actual evidence), and the head of the body. The unique-literal
 * lists are the thing to read: identical payload + different name is rotation,
 * different payload is real change.
 */
import { readFileSync } from "node:fs";
import { literalSet } from "./literals.js";

const [aPath, bPath, limRaw] = process.argv.slice(2);
if (!aPath || !bPath) {
  console.error("usage: read-pair.ts <fileA> <fileB> [uniqueLiteralsToShow]");
  process.exit(1);
}
const lim = limRaw ? Number(limRaw) : 25;

const aText = readFileSync(aPath, "utf8");
const bText = readFileSync(bPath, "utf8");
const aL = literalSet(aText);
const bL = literalSet(bText);

const onlyA = [...aL].filter((x) => !bL.has(x));
const onlyB = [...bL].filter((x) => !aL.has(x));
let inter = 0;
for (const x of aL) if (bL.has(x)) inter++;

console.log(`A ${aPath}`);
console.log(`  ${aText.length} bytes, ${aL.size} literals`);
console.log(`B ${bPath}`);
console.log(`  ${bText.length} bytes, ${bL.size} literals`);
console.log(
  `shared ${inter}   onlyA ${onlyA.length}   onlyB ${onlyB.length}   jaccard ${(inter / (aL.size + bL.size - inter)).toFixed(3)}`
);
console.log("");
console.log(
  `--- literals ONLY in A (${onlyA.length}, showing ${Math.min(lim, onlyA.length)}) ---`
);
for (const x of onlyA.slice(0, lim)) console.log(`  ${JSON.stringify(x)}`);
console.log(
  `--- literals ONLY in B (${onlyB.length}, showing ${Math.min(lim, onlyB.length)}) ---`
);
for (const x of onlyB.slice(0, lim)) console.log(`  ${JSON.stringify(x)}`);
console.log("");
console.log("--- A head ---");
console.log(aText.slice(0, 400));
console.log("--- B head ---");
console.log(bText.slice(0, 400));
