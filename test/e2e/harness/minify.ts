import { execSync } from "node:child_process";
import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { join, basename, dirname } from "node:path";
import { minify as terserMinify } from "terser";
import { transform as esbuildTransform } from "esbuild";
import { transform as swcTransform } from "@swc/core";
import { getBuildDir, getMinifiedDir, type FixtureConfig } from "./setup.js";

export interface MinifierConfig {
  id: string;
  tool: "terser" | "esbuild" | "swc" | "bun";
  options: Record<string, unknown>;
}

export interface MinificationResult {
  code: string;
  sourceMap: object; // RawSourceMap
  minifiedPath: string;
}

/**
 * Standard minifier configurations included in all fingerprint test runs.
 */
export const MINIFIER_CONFIGS: MinifierConfig[] = [
  {
    id: "terser-default",
    tool: "terser",
    options: {
      compress: true,
      mangle: true
    }
  },
  {
    id: "esbuild-default",
    tool: "esbuild",
    options: {
      minify: true
      // esbuild mangles by default with minify: true
    }
  },
  {
    id: "swc-default",
    tool: "swc",
    options: {
      compress: true,
      mangle: true
    }
  }
  // bun-default removed: Bun's --no-bundle mode silently drops source maps,
  // making e2e validation impossible. Use bun-bundle (BUN_BUNDLE_CONFIG) instead.
];

/**
 * Bun bundler config — only works on self-contained files (no unresolved imports).
 * Not included in MINIFIER_CONFIGS since not all fixtures support it.
 */
export const BUN_BUNDLE_CONFIG: MinifierConfig = {
  id: "bun-bundle",
  tool: "bun",
  options: {
    minify: true,
    bundle: true
  }
};

/**
 * All configs including specialized ones (for explicit use).
 */
export const ALL_MINIFIER_CONFIGS: MinifierConfig[] = [
  ...MINIFIER_CONFIGS,
  BUN_BUNDLE_CONFIG
];

export const DEFAULT_MINIFIER_CONFIG: MinifierConfig = MINIFIER_CONFIGS[0];

/**
 * Get a minifier config by ID (searches all configs including specialized ones).
 */
export function getMinifierConfig(id: string): MinifierConfig | undefined {
  return ALL_MINIFIER_CONFIGS.find((c) => c.id === id);
}

export async function minifyFile(
  inputPath: string,
  outputPath: string,
  config: MinifierConfig
): Promise<MinificationResult> {
  const source = readFileSync(inputPath, "utf-8");

  let code: string;
  let sourceMap: object;

  switch (config.tool) {
    case "terser":
      ({ code, sourceMap } = await minifyWithTerser(
        source,
        outputPath,
        config
      ));
      break;
    case "esbuild":
      ({ code, sourceMap } = await minifyWithEsbuild(
        source,
        inputPath,
        outputPath,
        config
      ));
      break;
    case "swc":
      ({ code, sourceMap } = await minifyWithSwc(
        source,
        inputPath,
        outputPath,
        config
      ));
      break;
    case "bun":
      ({ code, sourceMap } = minifyWithBun(inputPath, outputPath, config));
      break;
    default:
      throw new Error(`Unknown minifier tool: ${config.tool as string}`);
  }

  // Write minified code and source map
  mkdirSync(join(outputPath, ".."), { recursive: true });
  writeFileSync(outputPath, code, "utf-8");
  writeFileSync(`${outputPath}.map`, JSON.stringify(sourceMap), "utf-8");

  return {
    code,
    sourceMap,
    minifiedPath: outputPath
  };
}

async function minifyWithTerser(
  source: string,
  outputPath: string,
  config: MinifierConfig
): Promise<{ code: string; sourceMap: object }> {
  const result = await terserMinify(source, {
    compress: config.options.compress as boolean | undefined,
    mangle: config.options.mangle as boolean | undefined,
    sourceMap: {
      filename: basename(outputPath),
      url: `${basename(outputPath)}.map`
    }
  });

  if (!result.code) {
    throw new Error(`Terser produced no output`);
  }

  const sourceMap =
    typeof result.map === "string" ? JSON.parse(result.map) : result.map;
  return { code: result.code, sourceMap };
}

async function minifyWithEsbuild(
  source: string,
  inputPath: string,
  outputPath: string,
  _config: MinifierConfig
): Promise<{ code: string; sourceMap: object }> {
  const result = await esbuildTransform(source, {
    minify: true,
    sourcemap: true,
    sourcefile: basename(inputPath),
    loader: "js"
  });

  // esbuild returns sourcemap as a string
  const sourceMap = JSON.parse(result.map);
  // Add the source map URL comment
  const code = `${result.code}\n//# sourceMappingURL=${basename(outputPath)}.map`;

  return { code, sourceMap };
}

async function minifyWithSwc(
  source: string,
  inputPath: string,
  outputPath: string,
  config: MinifierConfig
): Promise<{ code: string; sourceMap: object }> {
  const result = await swcTransform(source, {
    filename: basename(inputPath),
    sourceMaps: true,
    minify: true,
    jsc: {
      minify: {
        compress: config.options.compress as boolean | undefined,
        mangle: config.options.mangle as boolean | undefined
      },
      parser: {
        syntax: "ecmascript"
      },
      target: "es2020"
    }
  });

  if (!result.code) {
    throw new Error(`SWC produced no output`);
  }

  // Add the source map URL comment
  const code = `${result.code}\n//# sourceMappingURL=${basename(outputPath)}.map`;
  const sourceMap =
    typeof result.map === "string" ? JSON.parse(result.map) : result.map;

  return { code, sourceMap };
}

function minifyWithBun(
  inputPath: string,
  outputPath: string,
  config: MinifierConfig
): { code: string; sourceMap: object } {
  const outname = basename(outputPath);
  const tmpFile = `${outputPath}.bun-tmp.js`;
  const bundle = config.options.bundle ? "" : "--no-bundle";

  mkdirSync(dirname(outputPath), { recursive: true });

  execSync(
    `bun build ${JSON.stringify(inputPath)} ${bundle} --minify --sourcemap=inline --outfile=${JSON.stringify(tmpFile)}`,
    { stdio: "pipe" }
  );

  const raw = readFileSync(tmpFile, "utf-8");
  execSync(`rm -f ${JSON.stringify(tmpFile)}`);

  // Extract inline source map
  const dataUrlMatch = raw.match(
    /\/\/[#@]\s*sourceMappingURL=data:application\/json;base64,(.+)/
  );
  let sourceMap: object;
  let codeBody: string;

  if (dataUrlMatch) {
    sourceMap = JSON.parse(Buffer.from(dataUrlMatch[1], "base64").toString());
    codeBody = raw.slice(0, dataUrlMatch.index).trimEnd();
  } else {
    // Fallback: no inline map found — create an empty one
    sourceMap = { version: 3, sources: [], mappings: "" };
    codeBody = raw;
  }

  // Strip Bun's debugId comment and add standard sourceMappingURL
  const code =
    codeBody.replace(/\/\/#\s*debugId=\S+/g, "").trimEnd() +
    `\n//# sourceMappingURL=${outname}.map`;

  return { code, sourceMap };
}

export async function minifyFixtureVersion(
  pkg: string,
  version: string,
  config: FixtureConfig,
  minifierConfig: MinifierConfig = DEFAULT_MINIFIER_CONFIG
): Promise<MinificationResult[]> {
  const results: MinificationResult[] = [];

  for (const entry of config.entryPoints) {
    // Build the input path: use the compiled JS from buildDir
    // The tsc build command with --outDir build puts output directly in build/
    // e.g., src/index.ts -> build/index.js
    const jsEntry = basename(entry).replace(/\.ts$/, ".js");
    const inputPath = join(getBuildDir(pkg, version), "build", jsEntry);

    // Output to minified dir
    const outputPath = join(
      getMinifiedDir(pkg, version),
      `${minifierConfig.id}.js`
    );

    console.log(`Minifying v${version} (${minifierConfig.id})...`);
    const result = await minifyFile(inputPath, outputPath, minifierConfig);
    results.push(result);
  }

  return results;
}
