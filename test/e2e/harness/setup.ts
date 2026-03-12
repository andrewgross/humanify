import { execSync } from "node:child_process";
import { existsSync, mkdirSync, cpSync, readFileSync } from "node:fs";
import { join, dirname, basename } from "node:path";

export interface FixtureConfig {
  package: string;
  repo: string;
  sourceStrategy:
    | { type: "git-tag"; tagPattern: string }
    | { type: "git-commit"; commits: Record<string, string> };
  entryPoints: string[];
  buildCommand?: string;
  versionPairs: Array<{
    v1: string;
    v2: string;
    description?: string;
    /**
     * Functions where a source-level "modified" classification should be
     * treated as an expected fingerprint match. This happens when a
     * syntactic change (e.g. arrow→function declaration) doesn't alter
     * the minified structural output.
     */
    expectMatchDespiteModification?: Array<{
      function: string;
      reason: string;
    }>;
  }>;
}

const FIXTURES_DIR = join(
  dirname(import.meta.url.replace("file://", "")),
  "..",
  "fixtures"
);

export function getFixtureDir(pkg: string): string {
  return join(FIXTURES_DIR, pkg);
}

export function loadFixtureConfig(pkg: string): FixtureConfig {
  const configPath = join(getFixtureDir(pkg), "fixture.config.json");
  if (!existsSync(configPath)) {
    throw new Error(`No fixture config found at ${configPath}`);
  }
  return JSON.parse(readFileSync(configPath, "utf-8"));
}

export function getSourceDir(pkg: string, version: string): string {
  return join(getFixtureDir(pkg), "source", `v${version}`);
}

export function getBuildDir(pkg: string, version: string): string {
  return join(getFixtureDir(pkg), "build", `v${version}`);
}

export function getMinifiedDir(pkg: string, version: string): string {
  return join(getFixtureDir(pkg), "minified", `v${version}`);
}

/**
 * Resolve the git ref for a given version based on the source strategy.
 */
function resolveGitRef(config: FixtureConfig, version: string): string {
  if (config.sourceStrategy.type === "git-tag") {
    return config.sourceStrategy.tagPattern.replace("{version}", version);
  }
  const ref = config.sourceStrategy.commits[version];
  if (!ref) {
    throw new Error(`No commit SHA configured for version ${version}`);
  }
  return ref;
}

/**
 * Copy entry points from the cloned repo to the versioned source directory.
 */
function copyEntryPoints(
  config: FixtureConfig,
  tempDir: string,
  sourceDir: string
): void {
  mkdirSync(sourceDir, { recursive: true });
  for (const entry of config.entryPoints) {
    const src = join(tempDir, entry);
    const dest = join(sourceDir, entry);
    if (!existsSync(src)) {
      throw new Error(`Entry point not found: ${src}`);
    }
    mkdirSync(dirname(dest), { recursive: true });
    cpSync(src, dest);
  }
}

/**
 * Set up the build directory for a version, either compiling or copying source.
 */
function setupBuildDir(
  config: FixtureConfig,
  pkg: string,
  version: string,
  sourceDir: string
): void {
  const buildDir = getBuildDir(pkg, version);
  mkdirSync(buildDir, { recursive: true });

  if (config.buildCommand) {
    for (const entry of config.entryPoints) {
      const src = join(sourceDir, entry);
      const dest = join(buildDir, entry);
      mkdirSync(dirname(dest), { recursive: true });
      cpSync(src, dest);
    }
    console.log(`Building v${version}...`);
    execSync(config.buildCommand, { cwd: buildDir, stdio: "inherit" });
    console.log(`Built v${version}`);
  } else {
    const buildOutputDir = join(buildDir, "build");
    mkdirSync(buildOutputDir, { recursive: true });
    for (const entry of config.entryPoints) {
      const src = join(sourceDir, entry);
      const dest = join(buildOutputDir, basename(entry));
      cpSync(src, dest);
    }
    console.log(`Copied JS source for v${version} (no build step needed)`);
  }
}

export async function setupFixture(pkg: string): Promise<void> {
  const config = loadFixtureConfig(pkg);
  const fixtureDir = getFixtureDir(pkg);
  const tempDir = join(fixtureDir, ".tmp-clone");

  // Collect all versions needed
  const versions = new Set<string>();
  for (const pair of config.versionPairs) {
    versions.add(pair.v1);
    versions.add(pair.v2);
  }

  // Clone repo if not already cloned
  if (!existsSync(tempDir)) {
    console.log(`Cloning ${config.repo}...`);
    execSync(`git clone --quiet ${config.repo} "${tempDir}"`, {
      stdio: "inherit"
    });
  } else {
    console.log(`Using existing clone at ${tempDir}`);
  }

  for (const version of versions) {
    const sourceDir = getSourceDir(pkg, version);

    if (existsSync(sourceDir)) {
      console.log(`Source for v${version} already exists, skipping`);
      continue;
    }

    const ref = resolveGitRef(config, version);
    console.log(`Checking out ${ref}...`);
    execSync(`git checkout --quiet "${ref}"`, {
      cwd: tempDir,
      stdio: "inherit"
    });

    copyEntryPoints(config, tempDir, sourceDir);
    console.log(`Copied source for v${version}`);

    setupBuildDir(config, pkg, version, sourceDir);
  }

  console.log(`\nFixture "${pkg}" setup complete.`);
}
