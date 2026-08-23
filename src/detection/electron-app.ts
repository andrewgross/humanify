/**
 * Electron app-directory detection — the directory-shaped counterpart of
 * `detectBundle`.
 *
 * A directory input cannot be identified from a 16KB code slice; it is
 * identified from its shape (a resolvable package.json main entry — what
 * Electron itself boots from) plus electron markers. The MINIFIER is still
 * detected from code, by running the ordinary signal detectors over the
 * main entry, so downstream skip-lists and eligibility see the real
 * toolchain. The entry's own bundler signals are kept in the signal list
 * for provenance, but the bundler is reported as "electron": packaging
 * routes the unpack adapter, exactly as "bun" does for Bun executables.
 */

import fs from "node:fs";
import { resolveElectronAppLayout } from "../unpack/electron-layout.js";
import { detectBundle } from "./detect.js";
import type { BundlerDetectionResult, DetectionSignal } from "./types.js";

/** An electron-family dependency name: electron, electron-*, @electron/*. */
const ELECTRON_DEP_RE = /^(electron(-.+)?|@electron\/.+)$/;

/** The entry importing/requiring the `electron` module (survives minification
 * because the module specifier is a string literal). */
const ELECTRON_MODULE_REF_RE =
  /(?:require\(|from\s*|import\()\s*["']electron["']/;

/**
 * Detect an extracted Electron app directory. Returns null when the
 * directory has no resolvable app layout; otherwise the bundler is
 * "electron" at tier "definitive" (an electron marker was found) or
 * "likely" (app-shaped layout only).
 */
export function detectElectronApp(
  appDir: string
): BundlerDetectionResult | null {
  const layout = resolveElectronAppLayout(appDir);
  if (!layout) return null;

  const electronSignals = collectElectronSignals(
    layout.dependencyNames,
    layout.mainAbs
  );
  // Minifier (and provenance signals) from the entry's actual code.
  const codeDetection = detectBundle(readEntry(layout.mainAbs));

  return {
    bundler: {
      type: "electron",
      tier: electronSignals.length > 0 ? "definitive" : "likely"
    },
    minifier: codeDetection.minifier,
    signals: [
      layoutSignal(layout.mainRel),
      ...electronSignals,
      ...codeDetection.signals
    ]
  };
}

function layoutSignal(mainRel: string): DetectionSignal {
  return {
    source: "electron-app",
    pattern: `package.json main resolves (${mainRel})`,
    bundler: "electron",
    tier: "likely"
  };
}

function collectElectronSignals(
  dependencyNames: string[],
  mainAbs: string
): DetectionSignal[] {
  const signals: DetectionSignal[] = [];
  const dep = dependencyNames.find((name) => ELECTRON_DEP_RE.test(name));
  if (dep) {
    signals.push({
      source: "electron-app",
      pattern: `${dep} in dependencies`,
      bundler: "electron",
      tier: "definitive"
    });
  }
  if (ELECTRON_MODULE_REF_RE.test(readEntry(mainAbs))) {
    signals.push({
      source: "electron-app",
      pattern: 'entry references the "electron" module',
      bundler: "electron",
      tier: "definitive"
    });
  }
  return signals;
}

/** Maximum bytes of the entry to scan, matching detectBundle's own limit. */
const ENTRY_SCAN_LIMIT = 16 * 1024;

function readEntry(mainAbs: string): string {
  try {
    const fd = fs.openSync(mainAbs, "r");
    try {
      const buf = Buffer.alloc(ENTRY_SCAN_LIMIT);
      const read = fs.readSync(fd, buf, 0, ENTRY_SCAN_LIMIT, 0);
      return buf.toString("utf-8", 0, read);
    } finally {
      fs.closeSync(fd);
    }
  } catch {
    return "";
  }
}
