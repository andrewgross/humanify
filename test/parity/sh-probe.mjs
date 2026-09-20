// probe: the TS statementHash on the colliding statements
import { readFileSync } from "node:fs";
const mod = await import("../../src/split/statement-hash.js");
const shipped = readFileSync(
  "/work/oracle/oracle-0294b28/dumps/2.1.85-2.1.86/text/shipped.js",
  "utf8"
);
const stmts = [
  shipped.slice(2610081, 2610102),
  shipped.slice(68183, 68204),
  shipped.slice(2952560, 2952582)
];
for (const code of stmts) {
  const { parseSync } = await import("@babel/core");
  const ast = parseSync(code, {
    sourceType: "script",
    configFile: false,
    babelrc: false
  });
  const h = mod.statementHash(ast.program.body[0]);
  console.log(JSON.stringify(code), "->", h);
}
