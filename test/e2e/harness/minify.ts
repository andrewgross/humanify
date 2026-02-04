import { readFileSync, writeFileSync, mkdirSync } from "fs";
import { join, basename } from "path";
import { minify } from "terser";
import { getBuildDir, getMinifiedDir, type FixtureConfig } from "./setup.js";

export interface MinifierConfig {
  id: string;
  tool: "terser";
  options: Record<string, unknown>;
}

export interface MinificationResult {
  code: string;
  sourceMap: object; // RawSourceMap
  minifiedPath: string;
}

export const DEFAULT_MINIFIER_CONFIG: MinifierConfig = {
  id: "terser-default",
  tool: "terser",
  options: {
    compress: true,
    mangle: true,
  },
};

export async function minifyFile(
  inputPath: string,
  outputPath: string,
  config: MinifierConfig
): Promise<MinificationResult> {
  const source = readFileSync(inputPath, "utf-8");

  const result = await minify(source, {
    compress: config.options.compress as boolean | undefined,
    mangle: config.options.mangle as boolean | undefined,
    sourceMap: {
      filename: basename(outputPath),
      url: basename(outputPath) + ".map",
    },
  });

  if (!result.code) {
    throw new Error(`Minification produced no output for ${inputPath}`);
  }

  // Parse source map from result
  const sourceMap = typeof result.map === "string" ? JSON.parse(result.map) : result.map;

  // Write minified code and source map
  mkdirSync(join(outputPath, ".."), { recursive: true });
  writeFileSync(outputPath, result.code, "utf-8");
  writeFileSync(outputPath + ".map", JSON.stringify(sourceMap), "utf-8");

  return {
    code: result.code,
    sourceMap,
    minifiedPath: outputPath,
  };
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
