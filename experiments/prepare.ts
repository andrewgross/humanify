/**
 * Fixture preparation: clone, install, and bundle open-source projects
 * for use as ground truth in split experiments.
 *
 * Usage: tsx experiments/prepare.ts <project-name>
 *        tsx experiments/prepare.ts --all
 */
import { execSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative } from "node:path";
import { buildSync } from "esbuild";
import type { FixtureConfig } from "./types.js";

const EXPERIMENTS_DIR = import.meta.dirname;
const FIXTURES_DIR = join(EXPERIMENTS_DIR, "fixtures");

function loadFixtureConfigs(): Record<string, FixtureConfig> {
  return JSON.parse(
    readFileSync(join(EXPERIMENTS_DIR, "fixtures.json"), "utf-8")
  );
}

/** Recursively list all .ts/.js source files in a directory. */
function listSourceFiles(dir: string, base: string): string[] {
  const files: string[] = [];
  const entries = execSync(
    `find ${JSON.stringify(dir)} -type f \\( -name '*.ts' -o -name '*.tsx' -o -name '*.js' -o -name '*.jsx' \\) ! -path '*/node_modules/*' ! -path '*/.git/*' ! -path '*/dist/*' ! -path '*/build/*'`,
    {
      encoding: "utf-8"
    }
  )
    .trim()
    .split("\n")
    .filter(Boolean);

  for (const entry of entries) {
    files.push(relative(base, entry));
  }
  return files.sort();
}

function cloneRepo(config: FixtureConfig, fixtureDir: string): string {
  const cloneDir = join(fixtureDir, "repo");

  if (existsSync(cloneDir)) {
    console.log(`  Using cached clone at ${cloneDir}`);
    // Ensure correct tag
    execSync(`git checkout ${config.tag}`, {
      cwd: cloneDir,
      stdio: "pipe"
    });
    return cloneDir;
  }

  console.log(`  Cloning ${config.repo} @ ${config.tag}...`);
  execSync(
    `git clone --depth 1 --branch ${config.tag} ${config.repo} ${cloneDir}`,
    { stdio: "pipe" }
  );
  return cloneDir;
}

function installDeps(config: FixtureConfig, repoDir: string): void {
  const cmd = config.installCmd ?? "npm install --ignore-scripts";
  console.log(`  Installing dependencies (${cmd})...`);
  execSync(cmd, { cwd: repoDir, stdio: "pipe" });
}

function bundleWithEsbuild(
  config: FixtureConfig,
  repoDir: string,
  fixtureDir: string
): { bundlePath: string; mapPath: string } {
  const entryPoint = join(repoDir, config.entry);
  const outfile = join(fixtureDir, "bundle.js");

  if (!existsSync(entryPoint)) {
    throw new Error(`Entry point not found: ${entryPoint}`);
  }

  console.log(`  Bundling ${config.entry}...`);
  const result = buildSync({
    entryPoints: [entryPoint],
    bundle: true,
    outfile,
    sourcemap: true,
    platform: config.platform,
    format: (config.format as "esm" | "cjs") ?? "esm",
    external: config.external ?? [],
    // Don't minify — we want readable output for splitting experiments
    minify: false,
    // Keep names for better debugging
    keepNames: true,
    // Target modern node
    target: "node18",
    logLevel: "warning"
  });

  if (result.errors.length > 0) {
    throw new Error(
      `esbuild errors: ${result.errors.map((e) => e.text).join(", ")}`
    );
  }

  const bundlePath = outfile;
  const mapPath = `${outfile}.map`;

  if (!existsSync(mapPath)) {
    throw new Error(`Source map not generated at ${mapPath}`);
  }

  return { bundlePath, mapPath };
}

function saveSourceManifest(repoDir: string, fixtureDir: string): void {
  const srcDir = join(repoDir, "src");
  const baseDir = existsSync(srcDir) ? srcDir : repoDir;
  const files = listSourceFiles(baseDir, repoDir);
  writeFileSync(
    join(fixtureDir, "source-manifest.json"),
    JSON.stringify(
      { sourceRoot: relative(fixtureDir, repoDir), files },
      null,
      2
    )
  );
  console.log(`  Source manifest: ${files.length} files`);
}

export async function prepareFixture(name: string): Promise<string> {
  const configs = loadFixtureConfigs();
  const config = configs[name];
  if (!config) {
    throw new Error(
      `Unknown fixture: ${name}. Available: ${Object.keys(configs).join(", ")}`
    );
  }

  const fixtureDir = join(FIXTURES_DIR, name);
  mkdirSync(fixtureDir, { recursive: true });

  console.log(`Preparing fixture: ${name}`);

  const repoDir = cloneRepo(config, fixtureDir);
  installDeps(config, repoDir);
  const { bundlePath, mapPath } = bundleWithEsbuild(
    config,
    repoDir,
    fixtureDir
  );
  saveSourceManifest(repoDir, fixtureDir);

  // Report bundle stats
  const bundleSource = readFileSync(bundlePath, "utf-8");
  const lineCount = bundleSource.split("\n").length;
  const sizeKB = (Buffer.byteLength(bundleSource) / 1024).toFixed(0);
  console.log(`  Bundle: ${lineCount.toLocaleString()} lines, ${sizeKB} KB`);
  console.log(`  Source map: ${mapPath}`);
  console.log(`  Done!`);

  return fixtureDir;
}

async function main(): Promise<void> {
  const args = process.argv.slice(2);

  if (args.length === 0 || args[0] === "--help") {
    console.log("Usage: tsx experiments/prepare.ts <project-name | --all>");
    console.log(`Available: ${Object.keys(loadFixtureConfigs()).join(", ")}`);
    process.exit(0);
  }

  if (args[0] === "--all") {
    for (const name of Object.keys(loadFixtureConfigs())) {
      await prepareFixture(name);
      console.log();
    }
  } else {
    await prepareFixture(args[0]);
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
