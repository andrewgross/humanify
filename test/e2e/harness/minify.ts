import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { join, basename } from "node:path";
import { minify as terserMinify } from "terser";
import { transform as esbuildTransform } from "esbuild";
import { transform as swcTransform } from "@swc/core";
import { getBuildDir, getMinifiedDir, type FixtureConfig } from "./setup.js";

export interface MinifierConfig {
  id: string;
  tool: "terser" | "esbuild" | "swc";
  options: Record<string, unknown>;
}

export interface MinificationResult {
  code: string;
  sourceMap: object; // RawSourceMap
  minifiedPath: string;
}

/**
 * All available minifier configurations.
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
];

export const DEFAULT_MINIFIER_CONFIG: MinifierConfig = MINIFIER_CONFIGS[0];

/**
 * Get a minifier config by ID.
 */
export function getMinifierConfig(id: string): MinifierConfig | undefined {
  return MINIFIER_CONFIGS.find((c) => c.id === id);
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
    default:
      throw new Error(`Unknown minifier tool: ${(config as any).tool}`);
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
