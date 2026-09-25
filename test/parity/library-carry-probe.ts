/**
 * Library-freeze carry probe (findings #32, Rust port): runs the REAL TS
 * beautify with the function carry armed on a set of snippets and freezes,
 * per snippet, everything the Rust owner (`libdetect::function_carry`)
 * must reproduce:
 *
 *   - `regions`: findCommentRegions over the RAW text;
 *   - `beautified`: the formatter's output text;
 *   - `carry`: the per-ordinal (library, node type) recorded on beautify's
 *     OUTPUT tree (raw starts);
 *   - `walk`: functionsInTreeOrder over the RE-PARSED beautified text —
 *     (type, start, end), UTF-8 byte offsets;
 *   - `resolved`: resolveFunctionLibraries on that tree — (start, end,
 *     library) of every library function, UTF-8 bytes;
 *   - `rawWalk`: the same walk over the RAW parse, with each function's
 *     library by its raw start (what a carry over an UNTRANSFORMED tree
 *     would record — differs from `carry` exactly where a transform
 *     reorders functions).
 *
 *   npx tsx test/parity/library-carry-probe.ts > test/parity/library-carry.json
 */
import * as fs from "node:fs";
import * as path from "node:path";
import { fileURLToPath } from "node:url";
import type * as t from "@babel/types";
import { parseSourceAst } from "../../src/babel-utils.js";
import {
  findCommentRegions,
  libraryAtOffset
} from "../../src/library-detection/comment-regions.js";
import {
  functionsInTreeOrder,
  resolveFunctionLibraries
} from "../../src/library-detection/function-carry.js";
import type { FileContext } from "../../src/pipeline/types.js";
import { createBabelPlugin } from "../../src/plugins/babel/babel.js";

const SNIPPETS: Record<string, string> = {
  "mid-sequence":
    "var a=function(app){return app},b=(0,/*! tinylib v1.2.3 */function(lib){return lib});",
  reorder:
    // biome-ignore lint/suspicious/noTemplateCurlyInString: the INPUT is JS source holding a template literal
    "`${function(early){return early}}`===f(/*! tinylib v1.2.3 */function(late){return late});",
  "conditional-logical":
    "x?function(p1){}:function(p2){};y&&function(p3){return function(p4){}};/*! tinylib v1.2.3 */z||function(p5){};",
  "two-banners":
    "var app=function(a){return a};/*! alpha v1.0.0 */var al=function(b){return function(c){return b+c}};/*! beta v2.0.0 */var be=(d)=>d*2;console.log(app,al,be);",
  "methods-and-classes":
    'var o={m(a){return a},get g(){return 1},set s(v){},[function(k){return k}()]:1,"q":function(w){}};/*! tinylib v1.2.3 */class C{constructor(x){}#p(y){}static st(z){}get gg(){return 2}f=(u)=>u;static{(function(sb){})()}}var o2={n(){}};',
  "esm-exports":
    "export default function(ed){return ed};/*! tinylib v1.2.3 */export const ex=(e1)=>e1;export function ef(e2){return function(e3){}};export{ex as ey};",
  "nesting-and-params":
    "function outer(a=function(d1){},{b}={b:()=>1}){return[1,2].map(function(e){return e})}/*! tinylib v1.2.3 */async function*g(){yield async()=>{}};new F(function(n1){});tag`${()=>2}`;label:do{(function(l1){})()}while(0);for(const f of[()=>1]){}",
  "non-ascii":
    'var s="é漢字😀";var app=function(a){return s+a};/*! tinylib v1.2.3 */var lib=function(z){return "ü"+z};'
};

/** UTF-16 index → UTF-8 byte offset. */
function bytes(text: string, index: number): number {
  return Buffer.byteLength(text.slice(0, index), "utf8");
}

function span(text: string, node: t.Node): { start: number; end: number } {
  return {
    start: bytes(text, node.start ?? -1),
    end: bytes(text, node.end ?? -1)
  };
}

async function probe(raw: string) {
  const regions = findCommentRegions(raw);
  const context: FileContext = { commentRegions: regions };
  const beautified = await createBabelPlugin()(raw, context);
  const carry = context.functionLibraries ?? null;
  const reparsed = parseSourceAst(beautified);
  const rawAst = parseSourceAst(raw);
  if (!reparsed || !rawAst) throw new Error("parse failed");
  const walk = functionsInTreeOrder(reparsed).map((fn) => ({
    type: fn.type,
    ...span(beautified, fn)
  }));
  const resolved = carry
    ? [...resolveFunctionLibraries(reparsed, carry)].map(([fn, library]) => ({
        ...span(beautified, fn),
        library
      }))
    : [];
  const rawWalk = functionsInTreeOrder(rawAst).map((fn) => ({
    type: fn.type,
    ...span(raw, fn),
    library: libraryAtOffset(regions, fn.start ?? -1)
  }));
  return {
    raw,
    regions: regions.map((r) => ({
      library: r.libraryName,
      start: bytes(raw, r.startOffset),
      end: r.endOffset === null ? null : bytes(raw, r.endOffset)
    })),
    beautified,
    carry,
    walk,
    resolved,
    rawWalk
  };
}

// The gate regimes' raw texts (/work/lf/cases/<case>/fresh.js, copied):
// real-sized mixed files — minified and commented, one to three regions.
const REGIME_DIR = path.join(
  path.dirname(fileURLToPath(import.meta.url)),
  "library-carry"
);
for (const file of fs.readdirSync(REGIME_DIR).sort()) {
  SNIPPETS[`regime:${file}`] = fs.readFileSync(
    path.join(REGIME_DIR, file),
    "utf8"
  );
}

const out: Record<string, unknown> = {};
for (const [name, raw] of Object.entries(SNIPPETS)) {
  out[name] = await probe(raw);
}
process.stdout.write(`${JSON.stringify(out, null, 1)}\n`);
