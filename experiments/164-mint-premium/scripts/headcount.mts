import * as fs from "node:fs";
import * as parser from "@babel/parser";
import { statementHash } from "../../../experiments/lib/js/statement-hash.js";
const bundlePath = "/Users/andrewgross/Development/claude-code-versions/inputs/claude-code-2.1.216/binary-decompiled/src/entrypoints/index.js";
const raw = fs.readFileSync(bundlePath, "utf8");
const rast = parser.parse(raw, { sourceType: "unambiguous", errorRecovery: true });
const rbody: any[] = (rast.program.body[0] as any).expression.body.body;
const shipped = fs.readFileSync("/tmp/eval-work/exp161-names-hop/2.1.216/.humanify/humanified.js", "utf8");
const ast = parser.parse(shipped, { sourceType: "unambiguous", errorRecovery: true });
const p0: any = ast.program.body[0];
const body: any[] = p0.type === "ExpressionStatement" && p0.expression.type === "FunctionExpression" ? p0.expression.body.body : ast.program.body;
const rawHash = new Map<number, string>();
rbody.forEach((s: any, i: number) => rawHash.set(i, statementHash(s)));
const eraHash: string[] = body.map((s: any) => statementHash(s));
const hashToEra = new Map<string, number[]>();
eraHash.forEach((h, i) => { const a = hashToEra.get(h) ?? []; a.push(i); hashToEra.set(h, a); });
// the head: the first era statements with no raw counterpart at/before them
const firstRaw = rawHash.get(0)!;
console.log("raw stmt0 hash", firstRaw.slice(0, 10), "era positions holding it:", (hashToEra.get(firstRaw) ?? []).slice(0, 5));
// walk: print the first 12 era statements' first 60 chars
body.slice(0, 12).forEach((s: any, i: number) => console.log(`era ${i}: ${shipped.slice(s.start, Math.min(s.end, s.start + 72)).replace(/\n/g, " ")}`));
