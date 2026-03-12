import type { DetectionSignal } from "../types.js";

/**
 * Detect browserify bundles. We look for the classic browserify outer IIFE
 * pattern: a function with (e,n,t) or (require,module,exports) parameters
 * called with a module map object, plus `installedModules` without
 * `__webpack_require__` (to avoid false positives on webpack).
 */
export function detectBrowserify(code: string): DetectionSignal[] {
  // Browserify and webpack both use installedModules, so we exclude webpack
  const hasWebpackRequire = /__webpack_require__/.test(code);
  if (hasWebpackRequire) return [];

  const signals: DetectionSignal[] = [];

  // Classic browserify outer pattern: function r(e,n,t){...}
  // with `.call(p.exports,function(r){` or `[0].call(`
  if (/\[0\]\.call\(/.test(code) && /\.exports\s*\}/.test(code)) {
    signals.push({
      source: "browserify",
      pattern: "browserify module call pattern",
      bundler: "browserify",
      tier: "definitive"
    });
  }

  if (/installedModules/.test(code)) {
    signals.push({
      source: "browserify",
      pattern: "installedModules (no webpack)",
      bundler: "browserify",
      tier: "definitive"
    });
  }

  return signals;
}
