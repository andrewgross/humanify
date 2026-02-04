import { execSync } from "child_process";
import { existsSync, mkdirSync, cpSync, readFileSync } from "fs";
import { join, dirname } from "path";

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
  }>;
}

const FIXTURES_DIR = join(dirname(import.meta.url.replace("file://", "")), "..", "fixtures");

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
    execSync(`git clone --quiet ${config.repo} "${tempDir}"`, { stdio: "inherit" });
  } else {
    console.log(`Using existing clone at ${tempDir}`);
  }

  for (const version of versions) {
    const sourceDir = getSourceDir(pkg, version);

    if (existsSync(sourceDir)) {
      console.log(`Source for v${version} already exists, skipping`);
      continue;
    }

    // Determine the git ref
    let ref: string;
    if (config.sourceStrategy.type === "git-tag") {
      ref = config.sourceStrategy.tagPattern.replace("{version}", version);
    } else {
      ref = config.sourceStrategy.commits[version];
      if (!ref) {
        throw new Error(`No commit SHA configured for version ${version}`);
      }
    }

    console.log(`Checking out ${ref}...`);
    execSync(`git checkout --quiet "${ref}"`, { cwd: tempDir, stdio: "inherit" });

    // Copy entry point files to versioned source dir
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
    console.log(`Copied source for v${version}`);

    // Run build command if configured
    if (config.buildCommand) {
      const buildDir = getBuildDir(pkg, version);
      mkdirSync(buildDir, { recursive: true });

      // Copy source to build dir for compilation
      for (const entry of config.entryPoints) {
        const src = join(sourceDir, entry);
        const dest = join(buildDir, entry);
        mkdirSync(dirname(dest), { recursive: true });
        cpSync(src, dest);
      }

      console.log(`Building v${version}...`);
      execSync(config.buildCommand, { cwd: buildDir, stdio: "inherit" });
      console.log(`Built v${version}`);
    }
  }

  console.log(`\nFixture "${pkg}" setup complete.`);
}
