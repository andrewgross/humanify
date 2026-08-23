/**
 * Electron app-directory layout resolution — the one owner of "where is this
 * extracted app's code?".
 *
 * Both electron detection (src/detection/electron-app.ts) and the electron
 * unpack adapter need the same answers: where the main entry is and which
 * top-level directory holds the app's own bundles. Electron itself boots an
 * app from `package.json`'s `main` with node-style resolution, so that is
 * the ground truth this mirrors — not a hardcoded `out/` (electron-vite) or
 * `dist/` (webpack builds), which vary per toolchain.
 *
 * The CODE ROOT is the main entry's top-level directory (`out` for
 * electron-vite, `dist` for webpack builds, `.` for flat apps): first-party
 * code is emitted under it, while `node_modules/` holds identifiable
 * third-party packages. Measured on ZCode 3.8.1: every first-party bundle
 * (including workspace `@zcode/*` packages) lands under `out/`; all 441
 * `node_modules` packages carry their own package.json.
 */

import fs from "node:fs";
import path from "node:path";

export interface ElectronAppLayout {
  /** Absolute path of the app dir (the extracted asar root). */
  appRoot: string;
  /** package.json name/version, when present. */
  name?: string;
  version?: string;
  /** Resolved main entry, appRoot-relative. */
  mainRel: string;
  mainAbs: string;
  /** Top-level dir of the main entry ("out", "dist", or "."). */
  codeRootRel: string;
  codeRootAbs: string;
  /** dependencies + devDependencies keys, for detection markers. */
  dependencyNames: string[];
}

interface AppPackageJson {
  name?: string;
  version?: string;
  main?: string;
  dependencies?: Record<string, string>;
  devDependencies?: Record<string, string>;
}

/**
 * Resolve an extracted Electron app directory's layout, or null when the
 * directory is not app-shaped (no parseable package.json, or a main entry
 * that does not resolve to a file inside the directory).
 */
export function resolveElectronAppLayout(
  appDir: string
): ElectronAppLayout | null {
  const appRoot = path.resolve(appDir);
  const pkg = readPackageJson(appRoot);
  if (!pkg) return null;

  const mainAbs = resolveMainEntry(appRoot, pkg.main ?? "index.js");
  if (!mainAbs) return null;

  const mainRel = path.relative(appRoot, mainAbs);
  const codeRootRel = topLevelDirOf(mainRel);
  return {
    appRoot,
    name: pkg.name,
    version: pkg.version,
    mainRel,
    mainAbs,
    codeRootRel,
    codeRootAbs: path.join(appRoot, codeRootRel),
    dependencyNames: [
      ...Object.keys(pkg.dependencies ?? {}),
      ...Object.keys(pkg.devDependencies ?? {})
    ]
  };
}

function readPackageJson(appRoot: string): AppPackageJson | null {
  try {
    const raw = fs.readFileSync(path.join(appRoot, "package.json"), "utf-8");
    const parsed = JSON.parse(raw);
    if (typeof parsed !== "object" || parsed === null) return null;
    return parsed as AppPackageJson;
  } catch {
    return null;
  }
}

/**
 * Node-style main resolution: the path itself, then `<path>.js`, then
 * `<path>/index.js`. Anything resolving outside the app dir is rejected —
 * a package.json is untrusted input here, and an escaping main would make
 * the unpack read (and the caller copy) files the user never pointed at.
 */
function resolveMainEntry(appRoot: string, main: string): string | null {
  const base = path.resolve(appRoot, main);
  const rel = path.relative(appRoot, base);
  if (rel.startsWith("..") || path.isAbsolute(rel)) return null;
  for (const candidate of [base, `${base}.js`, path.join(base, "index.js")]) {
    if (isFile(candidate)) return candidate;
  }
  return null;
}

function isFile(p: string): boolean {
  try {
    return fs.statSync(p).isFile();
  } catch {
    return false;
  }
}

function topLevelDirOf(rel: string): string {
  const [first] = rel.split(path.sep);
  return rel.includes(path.sep) ? first : ".";
}
