// probe: WP1.5 ground truth — run the TS module classification over a real
// oracle pair's minified text and print the records the Rust port must
// reproduce byte-for-byte (record shapes + banner semantics + helper scan).
import { readFileSync } from "node:fs";
const dumpDir = process.argv[2];
if (!dumpDir) {
  console.error("usage: node test/parity/wp1.5-probe.mjs <ts-dump-dir>");
  process.exit(1);
}
const code = readFileSync(`${dumpDir}/text/minified.js`, "utf8");

const { identifyBunCjsFactory } = await import(
  "../../src/shared/bun-helpers.js"
);
const { findWrapperFunction } = await import(
  "../../src/analysis/wrapper-detection.js"
);
const {
  classifyBunModules,
  nameCjsFactories,
  hashFallbackName,
  isHashFallbackName
} = await import("../../src/analysis/bun-module-classification.js");
const { parseSourceAst } = await import("../../src/babel-utils.js");

const helper = identifyBunCjsFactory(code);
console.log("helper:", JSON.stringify(helper));

const ast = parseSourceAst(code, { errorRecovery: true });
const wrapper = findWrapperFunction(ast);
console.log(
  "wrapper:",
  wrapper
    ? JSON.stringify({
        start: wrapper.functionPath.node.start,
        end: wrapper.functionPath.node.end,
        bodyStart: wrapper.functionPath.node.body.start,
        bodyEnd: wrapper.functionPath.node.body.end,
        bindings: Object.keys(wrapper.scope.bindings).length
      })
    : "null"
);

if (helper) {
  const classification = classifyBunModules(ast, code, wrapper);
  const counts = nameCjsFactories(classification, code);
  console.log("nameCounts:", JSON.stringify(counts));
  for (const f of classification.factories.slice(0, 3)) {
    console.log(
      "factory:",
      JSON.stringify({
        factoryVar: f.factoryVar,
        byteRange: f.byteRange,
        lineRange: f.lineRange,
        contentHash: f.contentHash,
        structuralHash: f.structuralHash,
        bannerText: f.bannerText ?? null,
        bannerPackage: f.bannerPackage ?? null,
        bannerVersion: f.bannerVersion ?? null,
        name: f.name ?? null,
        nameSource: f.nameSource ?? null
      })
    );
  }
  console.log("factoryCount:", classification.factories.length);
}

// tricky-window cases for the helper scan (leftmost-match semantics)
const cases = [
  [
    "var a=1,b=2,x=(A,q)=>()=>(A||I((A = {exports:{}}).exports, A), A.exports);",
    "marker with leading decls"
  ],
  [
    "var a=1;var x=(A,q)=>()=>(A||I((A = {exports:  {}}).exports, A), A.exports);",
    "two spaces after colon"
  ],
  [
    "f();var x=(A,q)=>()=>(A||I((A = {exports:{}}).exports, A), A.exports);",
    "statement before"
  ],
  ["var a=1,b=2,c=3;", "no marker"]
];
for (const [src, label] of cases) {
  console.log(`window[${label}]:`, JSON.stringify(identifyBunCjsFactory(src)));
}
// banner parse vectors (the TS parseBanner, via a classified single factory)
const bannerCases = [
  "/*! @azure/msal-common v15.13.1 */",
  "/*! Sharp */",
  "/*! Copyright 2013 */",
  "/*! highlight.js */",
  "/*! license */",
  "/*! see license */",
  "/*! pkg */",
  "/*! pkg; */",
  "/*! react-dom 18.2.0 */"
];
// The helper itself, then one factory calling it (the classification's shape:
// a declarator whose init is CALLEE(fn) with callee == the helper var).
const HELPER = `var x=(I,A)=>()=>(A||I((A = {exports:{}}).exports, A), A.exports);`;
for (const banner of bannerCases) {
  // Leading-comment form (collectBanner): the banner precedes the statement.
  const lead = `${HELPER} ${banner} var tO8=x((q,m)=>{module.exports=1;});`;
  const cl = classifyBunModules(parseSourceAst(lead), lead, null);
  const fl = cl?.factories[0];
  console.log(
    `banner[${banner.slice(4, -3)}]:`,
    JSON.stringify({
      bannerText: fl?.bannerText ?? null,
      bannerPackage: fl?.bannerPackage ?? null,
      bannerVersion: fl?.bannerVersion ?? null
    })
  );
  // In-body form (findBannerInsideBody): the banner sits inside a block body.
  const body = `${HELPER} var tO8=x((q,m)=>{${banner} module.exports=1;});`;
  const cb = classifyBunModules(parseSourceAst(body), body, null);
  const fb = cb?.factories[0];
  console.log(
    `inbody[${banner.slice(4, -3)}]:`,
    JSON.stringify({
      bannerText: fb?.bannerText ?? null,
      bannerPackage: fb?.bannerPackage ?? null,
      bannerVersion: fb?.bannerVersion ?? null
    })
  );
}
console.log("hashFallbackName:", hashFallbackName("abcdef1234567890"));
console.log("isHashFallbackName:", [
  isHashFallbackName("lib_abcdef12"),
  isHashFallbackName("lib_abc"),
  isHashFallbackName("react")
]);
