import * as fs from "node:fs";
import * as parser from "@babel/parser";
const raw = fs.readFileSync("/Users/andrewgross/Development/claude-code-versions/inputs/claude-code-2.1.216/binary-decompiled/src/entrypoints/index.js", "utf8");
const ast = parser.parse(raw, { sourceType: "unambiguous", errorRecovery: true });
const body: any[] = (ast.program.body[0] as any).expression.body.body;
// for every VariableDeclaration in the container (top-level), era statements per the beautifier rule:
// init-less declarators first as one var, then one declaration per initialized declarator
let growthNoRemoval = 0, joinedCount = 0, initLess = 0, withInit = 0;
for (const s of body) {
  if (s.type !== "VariableDeclaration") continue;
  const k = s.declarations.length;
  if (k < 2) continue;
  joinedCount++;
  const i = s.declarations.filter((d: any) => d.init).length;
  const j = k - i;
  initLess += j; withInit += i;
  const era = (i > 0 ? i : 0) + (j > 0 ? 1 : 0);
  growthNoRemoval += era - 1;
}
console.log(`container joined declarations ${joinedCount}, declarators: init-less ${initLess}, initialized ${withInit}`);
console.log(`predicted growth if nothing removed: ${growthNoRemoval}`);
// observed: era body 35903 vs raw 32544 = +3359, and era_of's removed set R unknown.
// also single-declarator declarations with init stay 1:1.
// the head rewrite: shipped head has helpers not in raw (exportAll, interopRequire, getCachedModule, moduleWrapper, hasOwnPropCheck, weakMapA/B/C)
