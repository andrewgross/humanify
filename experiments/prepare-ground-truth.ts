/**
 * Pre-compute and cache ground truth for a fixture.
 *
 * Parses the bundle, uses the source map to map each top-level function
 * to its original source file, and saves the result as JSON. The mapping
 * is keyed by function ordinal (position order in the bundle) since
 * function ordering is preserved through LLM rename + prettier.
 *
 * This cached ground truth can then be loaded for humanified variants
 * where the source map no longer matches (line numbers changed).
 *
 * Usage:
 *   tsx experiments/prepare-ground-truth.ts <fixture-name>
 *
 * Output:
 *   experiments/fixtures/<fixture-name>/ground-truth.json
 */
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { SourceMapConsumer } from "source-map";
import { buildFunctionGraph } from "../src/analysis/function-graph.js";
import { normalizeModulePath } from "../src/split/module-detect.js";
import { parseFile } from "../src/split/index.js";

const FIXTURES_DIR = join(import.meta.dirname, "fixtures");

interface CachedGroundTruth {
  /** Creation timestamp */
  createdAt: string;
  /** Source fixture this was computed from */
  sourceFixture: string;
  /** Total top-level functions found */
  totalFunctions: number;
  /** Functions mapped to original files (by ordinal) */
  functionsByOrdinal: Array<{
    ordinal: number;
    line: number;
    column: number;
    originalFile: string;
  }>;
  /** All unique original source files */
  sourceFiles: string[];
}

async function prepareGroundTruth(fixtureName: string): Promise<void> {
  const fixtureDir = join(FIXTURES_DIR, fixtureName);
  const bundlePath = join(fixtureDir, "bundle.js");
  const mapPath = join(fixtureDir, "bundle.js.map");
  const outputPath = join(fixtureDir, "ground-truth.json");

  if (!existsSync(bundlePath) || !existsSync(mapPath)) {
    throw new Error(
      `Fixture ${fixtureName} missing bundle or source map at ${fixtureDir}`
    );
  }

  console.log(`Preparing ground truth for: ${fixtureName}`);

  // Parse the bundle
  console.log("  Parsing bundle...");
  const { ast } = parseFile(bundlePath);
  const functions = buildFunctionGraph(ast, bundlePath);
  const topLevel = functions.filter((fn) => !fn.scopeParent);

  // Sort by source position (line, then column) for stable ordinals
  topLevel.sort((a, b) => {
    const aLine = a.path.node.loc?.start.line ?? 0;
    const bLine = b.path.node.loc?.start.line ?? 0;
    if (aLine !== bLine) return aLine - bLine;
    const aCol = a.path.node.loc?.start.column ?? 0;
    const bCol = b.path.node.loc?.start.column ?? 0;
    return aCol - bCol;
  });

  console.log(`  ${topLevel.length} top-level functions`);

  // Use source map to map each function to its original file
  console.log("  Mapping via source map...");
  const rawMap = JSON.parse(readFileSync(mapPath, "utf-8"));
  const functionsByOrdinal: CachedGroundTruth["functionsByOrdinal"] = [];
  const fileSet = new Set<string>();

  await SourceMapConsumer.with(rawMap, null, (consumer) => {
    for (let i = 0; i < topLevel.length; i++) {
      const fn = topLevel[i];
      const loc = fn.path.node.loc;
      if (!loc) continue;

      const original = consumer.originalPositionFor({
        line: loc.start.line,
        column: loc.start.column
      });

      if (!original.source) continue;

      const normalizedSource = normalizeModulePath(original.source);
      fileSet.add(normalizedSource);

      functionsByOrdinal.push({
        ordinal: i,
        line: loc.start.line,
        column: loc.start.column,
        originalFile: normalizedSource
      });
    }
  });

  const sourceFiles = Array.from(fileSet).sort();

  const cached: CachedGroundTruth = {
    createdAt: new Date().toISOString(),
    sourceFixture: fixtureName,
    totalFunctions: topLevel.length,
    functionsByOrdinal,
    sourceFiles
  };

  writeFileSync(outputPath, JSON.stringify(cached, null, 2));
  console.log(
    `  ${functionsByOrdinal.length}/${topLevel.length} functions mapped to ${sourceFiles.length} files`
  );
  console.log(`  Saved to ${outputPath}`);
}

const fixtureName = process.argv[2];
if (!fixtureName) {
  console.log("Usage: tsx experiments/prepare-ground-truth.ts <fixture-name>");
  process.exit(1);
}

prepareGroundTruth(fixtureName).catch((err) => {
  console.error(err);
  process.exit(1);
});
