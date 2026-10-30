import * as parser from "@babel/parser";
import * as fs from "node:fs";
const raw = fs.readFileSync("/Users/andrewgross/Development/claude-code-versions/inputs/claude-code-2.1.216/binary-decompiled/src/entrypoints/index.js", "utf8");
const ast = parser.parse(raw, { sourceType: "unambiguous", errorRecovery: true });
const p0: any = ast.program.body[0];
const body = p0.expression.body.body;
let extra = 0, joined = 0;
for (const st of body) {
  if (st.type === "VariableDeclaration" && st.declarations.length > 1) {
    joined++;
    extra += st.declarations.length - 1;
  }
}
console.log("raw container statements:", body.length, "joined var declarations:", joined, "extra declarators:", extra);
