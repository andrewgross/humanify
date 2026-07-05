/**
 * Print every bang-block banner the classifier sees and what name we
 * derive from it. Used to validate regex and false-positive filtering.
 */

import { readFileSync } from "node:fs";
import { parseSync } from "@babel/core";
import {
  classifyBunModules,
  nameCjsFactories
} from "../../src/analysis/bun-module-classification.js";
import { findWrapperFunction } from "../../src/analysis/wrapper-detection.js";

const src = readFileSync(process.argv[2], "utf-8");
const ast = parseSync(src, {
  sourceType: "unambiguous",
  parserOpts: { errorRecovery: true }
});
if (!ast || ast.type !== "File") throw new Error("parse failed");

const wrapper = findWrapperFunction(ast);
const cls = classifyBunModules(ast, src, wrapper);
if (!cls) {
  console.log("no Bun CJS");
  process.exit(0);
}

nameCjsFactories(cls, src);

console.log(`Factories with banner text:`);
for (const f of cls.factories) {
  if (!f.bannerText) continue;
  console.log(
    `  pkg=${JSON.stringify(f.bannerPackage)} ver=${JSON.stringify(
      f.bannerVersion
    )}  raw=${JSON.stringify(f.bannerText.slice(0, 80))}`
  );
}
