// @bun @bun-cjs
(function(exports, require, module, __filename, __dirname) {// src/core/registry.ts
var entries = new Map;
function register(k) {
  const n = (entries.get(k) ?? 0) + 1;
  entries.set(k, n);
  return n;
}

// src/core/logger.ts
register("logger-loaded");

class Logger {
  tag;
  constructor(tag) {
    this.tag = tag;
  }
  log(msg) {
    console.log(`[${this.tag}] ${msg}`);
  }
}
var rootLogger = new Logger("root");

// src/util/strings.ts
function capitalize(s) {
  return s.charAt(0).toUpperCase() + s.slice(1);
}
function kebab(s) {
  return s.replace(/\s+/g, "-").toLowerCase();
}
// src/util/math.ts
var clamp = (n, lo, hi) => Math.min(hi, Math.max(lo, n));
// src/config/constants.ts
var APP_NAME = "corpus-app";
var MAX_RETRIES = 7;
var TIMEOUT_MS = 1500;

// src/config/env.ts
var env = { name: APP_NAME, timeout: TIMEOUT_MS, debug: false };
var env_default = env;

// src/app/tasks/build.ts
function buildTask() {
  return clamp(MAX_RETRIES, 0, 5);
}

// src/vendorish/left-pad.ts
function leftPad(s, n) {
  while (s.length < n)
    s = ` ${s}`;
  return s;
}

// src/app/tasks/test.ts
function runTests() {
  return leftPad(String(buildTask()), 3);
}

// src/app/runner.ts
function runAll() {
  const label = capitalize(kebab(env_default.name));
  return `${label}:${buildTask()}:${runTests()}`;
}

// src/index.ts
console.log("corpus-entry", env_default.name, runAll());
})
