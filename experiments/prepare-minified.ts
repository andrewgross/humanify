/**
 * Prepare a minified version of an existing fixture.
 *
 * Takes an existing bundle (with source map) and runs terser on it,
 * stripping comments and mangling local variables while preserving
 * the source map chain for ground truth extraction.
 *
 * Usage:
 *   tsx experiments/prepare-minified.ts <source-fixture> <output-name>
 *
 * Example:
 *   tsx experiments/prepare-minified.ts zod zod-minified
 */
import { execSync } from "node:child_process";
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readFileSync,
  statSync,
  writeFileSync
} from "node:fs";
import { join } from "node:path";

const FIXTURES_DIR = join(import.meta.dirname, "fixtures");

function prepareMinified(sourceFixture: string, outputName: string): void {
  const sourceDir = join(FIXTURES_DIR, sourceFixture);
  const outputDir = join(FIXTURES_DIR, outputName);

  const sourceBundlePath = join(sourceDir, "bundle.js");
  const sourceMapPath = join(sourceDir, "bundle.js.map");

  if (!existsSync(sourceBundlePath) || !existsSync(sourceMapPath)) {
    throw new Error(
      `Source fixture ${sourceFixture} missing bundle or map. Run: tsx experiments/prepare.ts ${sourceFixture}`
    );
  }

  mkdirSync(outputDir, { recursive: true });

  const outputBundlePath = join(outputDir, "bundle.js");
  const outputMapPath = join(outputDir, "bundle.js.map");

  console.log(`Preparing minified fixture: ${outputName}`);
  console.log(`  Source: ${sourceFixture}`);

  // Report source stats
  const sourceCode = readFileSync(sourceBundlePath, "utf-8");
  const sourceLines = sourceCode.split("\n").length;
  const sourceSize = statSync(sourceBundlePath).size;
  console.log(
    `  Source bundle: ${sourceLines.toLocaleString()} lines, ${(sourceSize / 1024).toFixed(0)} KB`
  );

  // Run terser with source map input chaining
  console.log("  Running terser...");
  execSync(
    [
      "npx terser",
      JSON.stringify(sourceBundlePath),
      "--compress",
      "--mangle",
      `--source-map "content='${sourceMapPath}',url='bundle.js.map'"`,
      "-o",
      JSON.stringify(outputBundlePath)
    ].join(" "),
    { stdio: "pipe" }
  );

  // Verify output
  if (!existsSync(outputBundlePath) || !existsSync(outputMapPath)) {
    throw new Error("Terser did not generate output files");
  }

  const outputCode = readFileSync(outputBundlePath, "utf-8");
  const outputLines = outputCode.split("\n").length;
  const outputSize = statSync(outputBundlePath).size;
  console.log(
    `  Minified bundle: ${outputLines.toLocaleString()} lines, ${(outputSize / 1024).toFixed(0)} KB`
  );
  console.log(
    `  Compression: ${((1 - outputSize / sourceSize) * 100).toFixed(1)}% reduction`
  );

  // Copy source manifest if it exists
  const manifestPath = join(sourceDir, "source-manifest.json");
  if (existsSync(manifestPath)) {
    copyFileSync(manifestPath, join(outputDir, "source-manifest.json"));
  }

  // Symlink the repo directory to save space
  const repoDir = join(sourceDir, "repo");
  const outputRepoLink = join(outputDir, "repo");
  if (existsSync(repoDir) && !existsSync(outputRepoLink)) {
    execSync(
      `ln -s ${JSON.stringify(repoDir)} ${JSON.stringify(outputRepoLink)}`,
      {
        stdio: "pipe"
      }
    );
    console.log("  Linked repo directory from source fixture");
  }

  console.log("  Done!");
}

const args = process.argv.slice(2);
if (args.length < 2) {
  console.log(
    "Usage: tsx experiments/prepare-minified.ts <source-fixture> <output-name>"
  );
  console.log("Example: tsx experiments/prepare-minified.ts zod zod-minified");
  process.exit(1);
}

prepareMinified(args[0], args[1]);
