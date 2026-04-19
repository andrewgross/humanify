import { mkdtempSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  minifyFile,
  type MinifierConfig,
  type MinificationResult
} from "../../test/e2e/harness/minify.js";

/**
 * Minify a source string using the existing minifier infrastructure.
 * Writes to a temp dir to satisfy minifiers that need file paths (Bun).
 */
export async function minifyString(
  source: string,
  config: MinifierConfig,
  basename = "input.js"
): Promise<MinificationResult> {
  const dir = mkdtempSync(join(tmpdir(), "perturb-lab-"));
  try {
    const inputPath = join(dir, basename);
    const outputPath = join(dir, `out-${config.id}.js`);
    writeFileSync(inputPath, source, "utf-8");
    return await minifyFile(inputPath, outputPath, config);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}
