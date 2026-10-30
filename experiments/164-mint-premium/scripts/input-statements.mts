/**
 * exp164: pair the input bundle's top-level factory-body statements with
 * the shipped text's (same order), verify counts, and dump the input
 * statement spans. argv: <inputPath> <label>
 */
import * as fs from "node:fs";
import * as parser from "@babel/parser";

const INPUT = process.argv[2];
const ast = parser.parse(fs.readFileSync(INPUT, "utf8"), {
  sourceType: "unambiguous",
  errorRecovery: true,
});
let body: any[] = ast.program.body;
const p0: any = ast.program.body[0];
if (p0.type === "ExpressionStatement" && p0.expression.type === "FunctionExpression") {
  body = p0.expression.body.body;
} else if (ast.program.body.length === 1 && p0.type === "FunctionDeclaration") {
  body = p0.body.body;
}
console.log("input top-level statements:", body.length);
const spans = body.map((s, i) => ({ i, start: s.start, end: s.end, type: s.type }));
fs.writeFileSync("/work/exp164-mint-census/input-statements.json", JSON.stringify(spans));
console.log("first spans:", JSON.stringify(spans.slice(0, 3)));
