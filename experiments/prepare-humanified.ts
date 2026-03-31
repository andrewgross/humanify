/**
 * Prepare a humanified (LLM-renamed) version of an existing fixture.
 *
 * Takes an existing fixture bundle, runs the full humanify rename pipeline,
 * and caches the result. The humanified bundle can then be used as input
 * for split experiments that test the realistic workflow:
 *
 *   minified bundle → humanify rename → split into files
 *
 * Source map ground truth: We preserve the pre-rename source map alongside
 * the humanified bundle. Since LLM renaming doesn't change function boundaries
 * (only identifier names), the function-to-original-file mapping from the
 * source map still holds — we just need to match by AST position rather than
 * by name.
 *
 * Usage:
 *   tsx experiments/prepare-humanified.ts <source-fixture> <output-name> [options]
 *
 * Options:
 *   --endpoint <url>   LLM endpoint (default: HUMANIFY_ENDPOINT env var)
 *   --model <model>    Model name (default: HUMANIFY_MODEL env var)
 *   --api-key <key>    API key (default: HUMANIFY_API_KEY env var)
 *
 * Examples:
 *   # From unminified fixture (renames original names)
 *   tsx experiments/prepare-humanified.ts zod zod-humanified
 *
 *   # From minified fixture (renames mangled names — the realistic case)
 *   tsx experiments/prepare-humanified.ts zod-minified zod-minified-humanified
 */
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readFileSync,
  statSync,
  writeFileSync
} from "node:fs";
import { execSync } from "node:child_process";
import { join } from "node:path";
import { env } from "../src/env.js";
import { OpenAICompatibleProvider } from "../src/llm/openai-compatible.js";
import { withRateLimit } from "../src/llm/rate-limiter.js";
import { detectBundle } from "../src/detection/index.js";
import { buildPipelineConfig } from "../src/pipeline/config.js";
import type { FileContext } from "../src/pipeline/types.js";
import { createBabelPlugin } from "../src/plugins/babel/babel.js";
import { createPrettierPlugin } from "../src/plugins/prettier.js";
import { createRenamePlugin } from "../src/rename/plugin.js";
import { unminify } from "../src/unminify.js";

const FIXTURES_DIR = join(import.meta.dirname, "fixtures");

interface Options {
  endpoint?: string;
  apiKey?: string;
  model?: string;
}

async function prepareHumanified(
  sourceFixture: string,
  outputName: string,
  options: Options
): Promise<void> {
  const sourceDir = join(FIXTURES_DIR, sourceFixture);
  const outputDir = join(FIXTURES_DIR, outputName);

  const sourceBundlePath = join(sourceDir, "bundle.js");
  const sourceMapPath = join(sourceDir, "bundle.js.map");

  if (!existsSync(sourceBundlePath)) {
    throw new Error(
      `Source fixture ${sourceFixture} missing bundle. Run: tsx experiments/prepare.ts ${sourceFixture}`
    );
  }

  // Check for cached result
  const outputBundlePath = join(outputDir, "bundle.js");
  if (existsSync(outputBundlePath)) {
    const outputStat = statSync(outputBundlePath);
    const sourceStat = statSync(sourceBundlePath);
    if (outputStat.mtimeMs > sourceStat.mtimeMs) {
      console.log(
        `Humanified fixture ${outputName} already exists and is up-to-date. Skipping.`
      );
      console.log(`  Delete ${outputDir} to force re-generation.`);
      return;
    }
  }

  // Resolve LLM config
  const endpoint =
    options.endpoint ??
    env("HUMANIFY_ENDPOINT") ??
    env("OPENAI_BASE_URL") ??
    "https://api.openai.com/v1";
  const apiKey =
    options.apiKey ?? env("HUMANIFY_API_KEY") ?? env("OPENAI_API_KEY");
  const model = options.model ?? env("HUMANIFY_MODEL") ?? "gpt-4o-mini";

  if (!apiKey) {
    throw new Error(
      "API key required. Set --api-key, HUMANIFY_API_KEY, or OPENAI_API_KEY."
    );
  }

  mkdirSync(outputDir, { recursive: true });

  console.log(`Preparing humanified fixture: ${outputName}`);
  console.log(`  Source: ${sourceFixture}`);
  console.log(`  LLM: ${model} @ ${endpoint}`);

  const sourceCode = readFileSync(sourceBundlePath, "utf-8");
  const sourceLines = sourceCode.split("\n").length;
  console.log(`  Source bundle: ${sourceLines.toLocaleString()} lines`);

  // Set up the LLM provider
  const baseProvider = new OpenAICompatibleProvider({
    endpoint,
    apiKey,
    model,
    timeout: 300000
  });
  const provider = withRateLimit(baseProvider, {
    maxConcurrent: 10,
    retryAttempts: 3
  });

  // Set up plugins
  const renamePlugin = createRenamePlugin({
    provider,
    concurrency: 10,
    skipLibraries: true,
    onProgress: (metrics) => {
      if (metrics.functions.completed % 20 === 0) {
        console.log(
          `  Renamed ${metrics.functions.completed}/${metrics.functions.total} functions...`
        );
      }
    }
  });

  // The unminify function writes to outputDir. It expects the bundle to be
  // at the input path and writes the unpacked+renamed files to outputDir.
  // For bundles without detectable module boundaries (minified), it will
  // produce a single file.
  console.log("  Running humanify pipeline (this may take a while)...");
  const startTime = Date.now();

  const bundledCode = readFileSync(sourceBundlePath, "utf-8");
  const detection = detectBundle(bundledCode);
  const config = buildPipelineConfig(detection);

  await unminify(
    sourceBundlePath,
    outputDir,
    config,
    [
      (code: string, _ctx: FileContext) => createBabelPlugin({})(code),
      async (code: string, _ctx: FileContext) => {
        const result = await renamePlugin(code);
        return result.code;
      },
      (code: string, _ctx: FileContext) => createPrettierPlugin({})(code)
    ],
    {
      skipLibraries: true,
      log: (msg: string) => console.log(`  ${msg}`)
    }
  );

  const elapsed = ((Date.now() - startTime) / 1000).toFixed(1);
  console.log(`  Rename completed in ${elapsed}s`);

  // The unminify pipeline may have unpacked the bundle into multiple files.
  // For split experiments we need a single bundle.js. If multiple files were
  // created, concatenate them back (preserving the multi-file output too).
  const outputFiles = execSync(
    `find ${JSON.stringify(outputDir)} -name '*.js' -not -name 'bundle.js'`,
    {
      encoding: "utf-8"
    }
  )
    .trim()
    .split("\n")
    .filter(Boolean);

  if (outputFiles.length === 1) {
    // Single file output — rename to bundle.js if needed
    const singleFile = outputFiles[0];
    if (singleFile !== outputBundlePath) {
      const content = readFileSync(singleFile, "utf-8");
      writeFileSync(outputBundlePath, content);
    }
  } else if (outputFiles.length > 1) {
    // Multiple files — concatenate into bundle.js for split experiments
    console.log(
      `  Unpacked to ${outputFiles.length} files, concatenating to bundle.js`
    );
    const parts: string[] = [];
    for (const f of outputFiles.sort()) {
      parts.push(`// --- ${f.replace(outputDir + "/", "")} ---`);
      parts.push(readFileSync(f, "utf-8"));
    }
    writeFileSync(outputBundlePath, parts.join("\n\n"));
  }

  // Copy source map from source fixture (for ground truth extraction).
  // The source map maps original bundle positions to original source files.
  // After rename, function boundaries haven't moved — only names changed.
  // So the source map still provides valid ground truth for which functions
  // came from which original file.
  if (existsSync(sourceMapPath)) {
    copyFileSync(sourceMapPath, join(outputDir, "bundle.js.map"));
    console.log("  Copied source map for ground truth");
  }

  // Copy source manifest
  const manifestPath = join(sourceDir, "source-manifest.json");
  if (existsSync(manifestPath)) {
    copyFileSync(manifestPath, join(outputDir, "source-manifest.json"));
  }

  // Link repo directory
  const repoDir = join(sourceDir, "repo");
  const outputRepoLink = join(outputDir, "repo");
  if (existsSync(repoDir) && !existsSync(outputRepoLink)) {
    execSync(
      `ln -s ${JSON.stringify(repoDir)} ${JSON.stringify(outputRepoLink)}`,
      { stdio: "pipe" }
    );
  }

  // Report output stats
  if (existsSync(outputBundlePath)) {
    const outputCode = readFileSync(outputBundlePath, "utf-8");
    const outputLines = outputCode.split("\n").length;
    console.log(`  Humanified bundle: ${outputLines.toLocaleString()} lines`);
  }

  console.log("  Done!");
}

function parseArgs(): {
  source: string;
  output: string;
  options: Options;
} {
  const args = process.argv.slice(2);
  if (args.length < 2 || args[0].startsWith("-")) {
    console.log(
      "Usage: tsx experiments/prepare-humanified.ts <source-fixture> <output-name> [options]"
    );
    console.log("Options:");
    console.log("  --endpoint <url>   LLM endpoint");
    console.log("  --model <model>    Model name");
    console.log("  --api-key <key>    API key");
    console.log(
      "\nExample: tsx experiments/prepare-humanified.ts zod-minified zod-minified-humanified"
    );
    process.exit(1);
  }

  const source = args[0];
  const output = args[1];
  const options: Options = {};

  for (let i = 2; i < args.length; i++) {
    switch (args[i]) {
      case "--endpoint":
        options.endpoint = args[++i];
        break;
      case "--model":
        options.model = args[++i];
        break;
      case "--api-key":
        options.apiKey = args[++i];
        break;
    }
  }

  return { source, output, options };
}

const { source, output, options } = parseArgs();
prepareHumanified(source, output, options).catch((err) => {
  console.error(err);
  process.exit(1);
});
