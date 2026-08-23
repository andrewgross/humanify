/**
 * Electron unpack adapter.
 *
 * Input is an EXTRACTED app directory — the upfront step (installer →
 * app.asar → files, see electron-unpack/README.md) runs before humanify,
 * the same way Bun-executable JS is extracted before a Bun run. Unlike the
 * single-bundle adapters there is nothing to unbundle: the app ships as a
 * file tree already, so unpack means selecting the humanify target and
 * copying it where the pipeline may mutate it.
 *
 * What is copied: every .js/.cjs/.mjs under the CODE ROOT (the main
 * entry's top-level dir — see electron-layout.ts), preserving relative
 * layout. What is not: `node_modules/` (identifiable third-party packages,
 * recorded in the manifest's vendored census instead), non-JS assets, and
 * anything outside the code root. The output tree is a rename target, not
 * a runnable app.
 */

import fs from "node:fs/promises";
import path from "node:path";
import type { BundlerDetectionResult } from "../../detection/types.js";
import { listJsFilesRecursive } from "../../file-utils.js";
import {
  type ElectronAppLayout,
  resolveElectronAppLayout
} from "../electron-layout.js";
import type { UnpackAdapter, UnpackInput, UnpackResult } from "../types.js";

/** JS extensions that enter the rename pipeline. */
const JS_EXTS = [".js", ".cjs", ".mjs"] as const;

/** Sidecar manifest recording what this adapter selected and skipped. */
export interface ElectronAppManifest {
  adapter: "electron";
  app: { name?: string; version?: string; main: string };
  codeRoot: string;
  fileCount: number;
  /** Third-party packages left behind in the app's node_modules. */
  vendored: Array<{ name: string; version?: string }>;
}

export class ElectronUnpackAdapter implements UnpackAdapter {
  name = "electron";

  supports(detection: BundlerDetectionResult): boolean {
    return detection.bundler?.type === "electron";
  }

  async unpack(input: UnpackInput, outputDir: string): Promise<UnpackResult> {
    const layout = resolveLayoutOrThrow(input);
    guardOutputDir(layout.appRoot, outputDir);

    const relFiles = listJsFilesRecursive(
      layout.codeRootAbs,
      layout.codeRootAbs,
      JS_EXTS
    ).sort();
    if (relFiles.length === 0) {
      throw new Error(
        `no JS under the app's code root ${layout.codeRootAbs} — nothing to process`
      );
    }

    const files: Array<{ path: string }> = [];
    for (const rel of relFiles) {
      const dest = path.join(outputDir, layout.codeRootRel, rel);
      await fs.mkdir(path.dirname(dest), { recursive: true });
      await fs.copyFile(path.join(layout.codeRootAbs, rel), dest);
      files.push({ path: dest });
    }

    await writeManifest(outputDir, layout, files.length);
    return { files };
  }
}

function resolveLayoutOrThrow(input: UnpackInput): ElectronAppLayout {
  if (input.kind !== "directory") {
    throw new Error(
      "the electron adapter unpacks an extracted app directory — for an " +
        "installer or app.asar, run the extraction first " +
        "(electron-unpack/README.md)"
    );
  }
  const layout = resolveElectronAppLayout(input.path);
  if (!layout) {
    throw new Error(
      `${input.path} has no resolvable Electron app layout ` +
        "(package.json with a main entry that resolves to a file inside it)"
    );
  }
  return layout;
}

/**
 * The pipeline mutates outputDir copies in place; an output dir inside the
 * app dir could make a copy resolve onto its own source and clobber the
 * user's extracted app. Refuse upfront.
 */
function guardOutputDir(appRoot: string, outputDir: string): void {
  const rel = path.relative(appRoot, path.resolve(outputDir));
  if (!rel.startsWith("..") && !path.isAbsolute(rel)) {
    throw new Error(
      `output dir ${outputDir} is inside the app directory ${appRoot} — ` +
        "choose one outside it, so processing never overwrites the extracted app"
    );
  }
}

async function writeManifest(
  outputDir: string,
  layout: ElectronAppLayout,
  fileCount: number
): Promise<void> {
  const manifest: ElectronAppManifest = {
    adapter: "electron",
    app: {
      name: layout.name,
      version: layout.version,
      // POSIX separators like the bun manifest, on every platform.
      main: layout.mainRel.split(path.sep).join("/")
    },
    codeRoot: layout.codeRootRel,
    fileCount,
    vendored: await collectVendoredPackages(layout.appRoot)
  };
  const dest = path.join(outputDir, ".humanify", "electron-app.json");
  await fs.mkdir(path.dirname(dest), { recursive: true });
  await fs.writeFile(dest, `${JSON.stringify(manifest, null, 2)}\n`);
}

/**
 * Census of the app's node_modules — every package with its own
 * package.json (scoped packages one level deeper). These stay behind on
 * purpose: they are identifiable third-party code, humanify's job is the
 * app's own bundles.
 */
async function collectVendoredPackages(
  appRoot: string
): Promise<Array<{ name: string; version?: string }>> {
  const root = path.join(appRoot, "node_modules");
  const found: Array<{ name: string; version?: string }> = [];
  for (const dir of await packageDirs(root)) {
    const pkg = await readPackageIdentity(path.join(root, dir));
    if (pkg) found.push(pkg);
  }
  return found.sort((a, b) => a.name.localeCompare(b.name));
}

/** node_modules entries that can hold a package: pkg or @scope/pkg. */
async function packageDirs(root: string): Promise<string[]> {
  const dirs: string[] = [];
  for (const entry of await readdirOrEmpty(root)) {
    if (entry.startsWith(".")) continue;
    if (entry.startsWith("@")) {
      for (const scoped of await readdirOrEmpty(path.join(root, entry))) {
        dirs.push(path.join(entry, scoped));
      }
    } else {
      dirs.push(entry);
    }
  }
  return dirs;
}

async function readdirOrEmpty(dir: string): Promise<string[]> {
  try {
    return await fs.readdir(dir);
  } catch {
    return [];
  }
}

async function readPackageIdentity(
  dir: string
): Promise<{ name: string; version?: string } | null> {
  try {
    const raw = await fs.readFile(path.join(dir, "package.json"), "utf-8");
    const pkg = JSON.parse(raw) as { name?: string; version?: string };
    if (!pkg.name) return null;
    return { name: pkg.name, version: pkg.version };
  } catch {
    return null;
  }
}
