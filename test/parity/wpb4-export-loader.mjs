// WPB.4 probe loader: makes the pipeline driver's MODULE-PRIVATE report and
// writer functions callable by test/parity/wpb4-probe.ts, so the vectors
// come from the real TS functions instead of re-typed copies of their
// format strings. Registered by wpb4-export-hook.mjs (`tsx --import`).
//
// Observation only: the named functions are re-exported unchanged (tsx
// keeps declaration names; a second `export {}` clause is legal ESM).
const EXPORT = {
  "/src/commands/unified.ts": [
    "reportParseFailures",
    "reportSemanticFailures",
    "reportInternalErrors",
    "reportVendorNaming",
    "vendorNamingAttempted",
    "writeEvalStats",
    "writeStageHashes",
    "writePlacementStats",
    "writeSplitLedger"
  ],
  "/src/output-validation.ts": [
    "compareSemantics",
    "buildExcerpt",
    "describeParseError",
    "programTokens"
  ]
};

export async function load(url, context, nextLoad) {
  const result = await nextLoad(url, context);
  const suffix = Object.keys(EXPORT).find((s) => url.endsWith(s));
  if (!suffix || result.source == null) return result;
  let source =
    typeof result.source === "string"
      ? result.source
      : Buffer.from(result.source).toString("utf8");
  for (const name of EXPORT[suffix]) {
    if (source.split(`function ${name}(`).length !== 2) {
      throw new Error(
        `wpb4 loader: expected one "function ${name}(" in ${url}`
      );
    }
  }
  source += `\nexport { ${EXPORT[suffix].join(", ")} };\n`;
  return { ...result, source };
}
