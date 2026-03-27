/**
 * E2E Split Validation
 *
 * Verifies that split output is syntactically valid JavaScript that can
 * be parsed and (optionally) imported.
 *
 * Usage:
 *   tsx experiments/validate-split.ts <fixture-name> [--split-strategy <s>]
 */
import { existsSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { parseSync } from "@babel/core";
import type { SplitStrategyType } from "../src/split/adapters/types.js";
import { buildFileContents } from "../src/split/emitter.js";
import { parseFile, splitDryRun } from "../src/split/index.js";
import { prepareFixture } from "./prepare.js";

const FIXTURES_DIR = join(import.meta.dirname, "fixtures");

interface ValidationResult {
  /** Total output files generated */
  totalFiles: number;
  /** Files that parse without syntax errors */
  syntaxValid: number;
  /** Files with syntax errors */
  syntaxErrors: Array<{ file: string; error: string }>;
  /** Whether the barrel index.js was generated */
  hasBarrelIndex: boolean;
  /** Whether import() on index.js succeeds (if run) */
  importsResolve: boolean | null;
}

/** Parse a JS string to verify syntax validity. */
function checkSyntax(
  code: string,
  fileName: string
): { valid: boolean; error?: string } {
  try {
    const ast = parseSync(code, {
      sourceType: "module",
      filename: fileName
    });
    return { valid: !!ast };
  } catch (err) {
    return {
      valid: false,
      error: err instanceof Error ? err.message : String(err)
    };
  }
}

export async function validateSplit(
  fixtureName: string,
  options?: { splitStrategy?: SplitStrategyType }
): Promise<ValidationResult> {
  const fixtureDir = join(FIXTURES_DIR, fixtureName);
  const bundlePath = join(fixtureDir, "bundle.js");

  // Auto-prepare if fixture doesn't exist
  if (!existsSync(bundlePath)) {
    console.log(`Fixture ${fixtureName} not found, preparing...`);
    await prepareFixture(fixtureName);
  }

  console.log(`\n=== Validating Split: ${fixtureName} ===\n`);

  // 1. Run splitDryRun to get the plan
  console.log("Running split...");
  const splitOpts: { splitStrategy?: SplitStrategyType } = {};
  if (options?.splitStrategy) {
    splitOpts.splitStrategy = options.splitStrategy;
  }
  const plan = splitDryRun([bundlePath], splitOpts);

  // 2. Build file contents
  const parsedFile = parseFile(bundlePath);
  const parsedFiles = [parsedFile];
  const fileContents = buildFileContents(plan, parsedFiles);

  console.log(`  ${fileContents.size} output files generated\n`);

  // 3. Validate each file
  const result: ValidationResult = {
    totalFiles: fileContents.size,
    syntaxValid: 0,
    syntaxErrors: [],
    hasBarrelIndex: fileContents.has("index.js"),
    importsResolve: null
  };

  for (const [fileName, content] of fileContents) {
    const check = checkSyntax(content, fileName);
    if (check.valid) {
      result.syntaxValid++;
    } else {
      result.syntaxErrors.push({
        file: fileName,
        error: check.error ?? "unknown"
      });
    }
  }

  // 4. Report
  console.log(`  Syntax validation:`);
  console.log(
    `    ${result.syntaxValid}/${result.totalFiles} files parse successfully`
  );

  if (result.syntaxErrors.length > 0) {
    console.log(`    ${result.syntaxErrors.length} files with errors:`);
    for (const { file, error } of result.syntaxErrors.slice(0, 10)) {
      const shortError = error.length > 80 ? error.slice(0, 80) + "..." : error;
      console.log(`      ${file}: ${shortError}`);
    }
  }

  console.log(`  Barrel index: ${result.hasBarrelIndex ? "YES" : "NO"}`);

  // 5. Optional: write to temp dir and try dynamic import
  const tempDir = join(fixtureDir, ".validate-temp");
  try {
    mkdirSync(tempDir, { recursive: true });
    for (const [fileName, content] of fileContents) {
      const filePath = join(tempDir, fileName);
      const dir = filePath.slice(0, filePath.lastIndexOf("/"));
      if (dir !== tempDir) mkdirSync(dir, { recursive: true });
      writeFileSync(filePath, content);
    }

    if (result.hasBarrelIndex) {
      try {
        const indexPath = join(tempDir, "index.js");
        await import(indexPath);
        result.importsResolve = true;
        console.log(`  Import test: PASS`);
      } catch (err) {
        result.importsResolve = false;
        const msg = err instanceof Error ? err.message : String(err);
        console.log(
          `  Import test: FAIL - ${msg.length > 100 ? msg.slice(0, 100) + "..." : msg}`
        );
      }
    }
  } finally {
    // Cleanup
    rmSync(tempDir, { recursive: true, force: true });
  }

  // Summary
  const allValid = result.syntaxErrors.length === 0;
  console.log(
    `\n  Result: ${allValid ? "PASS" : "FAIL"} (${result.syntaxValid}/${result.totalFiles} syntax valid${result.importsResolve === true ? ", imports resolve" : ""})\n`
  );

  return result;
}

// CLI entry point
async function main(): Promise<void> {
  const args = process.argv.slice(2);
  const fixtureName = args[0];
  if (!fixtureName || fixtureName.startsWith("-")) {
    console.log(
      "Usage: tsx experiments/validate-split.ts <fixture-name> [--split-strategy <s>]"
    );
    process.exit(1);
  }

  let splitStrategy: SplitStrategyType | undefined;
  const stratIdx = args.indexOf("--split-strategy");
  if (stratIdx !== -1) {
    splitStrategy = args[stratIdx + 1] as SplitStrategyType;
  }

  const result = await validateSplit(fixtureName, { splitStrategy });
  process.exit(result.syntaxErrors.length > 0 ? 1 : 0);
}

if (
  import.meta.url === `file://${process.argv[1]}` ||
  process.argv[1]?.endsWith("/validate-split.ts")
) {
  main().catch((err) => {
    console.error(err);
    process.exit(1);
  });
}
