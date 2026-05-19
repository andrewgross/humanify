/**
 * For each bang-block banner with `chokidar`/`msal-common`/`msal-node`,
 * print the helper var used by the enclosing wrapper (CJS factory `Q` vs
 * lazy init `Z` vs neither). Tells us why banners aren't being matched.
 */

import { readFileSync } from "node:fs";

const src = readFileSync(process.argv[2], "utf-8");
const targets = ["chokidar", "msal-common", "msal-node", "safe-buffer"];

for (const target of targets) {
  const banner = `/*! ${target}`;
  let idx = src.indexOf(banner);
  while (idx !== -1) {
    // Walk backward to find the nearest preceding `var X = Y(` pattern
    // and report Y (the helper var). We cap the lookback at 10K chars.
    const lookback = src.slice(Math.max(0, idx - 10000), idx);
    const matches = [
      ...lookback.matchAll(
        /(?:var|let|const|,)\s*([$\w]+)\s*=\s*([$\w]+)\s*\(/g
      )
    ];
    const last = matches[matches.length - 1];
    const after = src.slice(idx, idx + 100).replace(/\n/g, " ");
    console.log(
      `${target}@${idx}: helper=${last?.[2] ?? "?"}  ${JSON.stringify(after.slice(0, 80))}`
    );
    idx = src.indexOf(banner, idx + 1);
  }
}
