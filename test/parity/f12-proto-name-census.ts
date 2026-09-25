/**
 * 16-findings-queue #12 census: bindings named after an Object.prototype
 * member (toString, constructor, hasOwnProperty, …) are the only ones whose
 * record lookups could fall through to the prototype. Count them per text.
 *   npx tsx test/parity/f12-proto-name-census.ts <file.js>...
 */
import * as fs from "node:fs";
import { parse } from "@babel/parser";
import { traverse } from "../../src/babel-utils.js";

const PROTO = new Set(Object.getOwnPropertyNames(Object.prototype));

for (const file of process.argv.slice(2)) {
  const ast = parse(fs.readFileSync(file, "utf8"), {
    sourceType: "unambiguous",
    errorRecovery: true
  });
  let hits = 0;
  const examples: string[] = [];
  traverse(ast, {
    Scope(p: { scope: { bindings: Record<string, unknown> } }) {
      for (const name of Object.keys(p.scope.bindings)) {
        if (!PROTO.has(name)) continue;
        hits++;
        if (examples.length < 3) examples.push(name);
      }
    }
  });
  console.log(
    `${hits}\t${file.split("/").slice(-3).join("/")}\t${examples.join(",")}`
  );
}
