// @bun @bun-cjs
(function(exports, require, module, __filename, __dirname) {var __create = Object.create;
var __getProtoOf = Object.getPrototypeOf;
var __defProp = Object.defineProperty;
var __getOwnPropNames = Object.getOwnPropertyNames;
var __hasOwnProp = Object.prototype.hasOwnProperty;
function __accessProp(key) {
  return this[key];
}
var __toESMCache_node;
var __toESMCache_esm;
var __toESM = (mod, isNodeMode, target) => {
  var canCache = mod != null && typeof mod === "object";
  if (canCache) {
    var cache = isNodeMode ? __toESMCache_node ??= new WeakMap : __toESMCache_esm ??= new WeakMap;
    var cached = cache.get(mod);
    if (cached)
      return cached;
  }
  target = mod != null ? __create(__getProtoOf(mod)) : {};
  const to = isNodeMode || !mod || !mod.__esModule ? __defProp(target, "default", { value: mod, enumerable: true }) : target;
  for (let key of __getOwnPropNames(mod))
    if (!__hasOwnProp.call(to, key))
      __defProp(to, key, {
        get: __accessProp.bind(mod, key),
        enumerable: true
      });
  if (canCache)
    cache.set(mod, to);
  return to;
};
var __commonJS = (cb, mod) => () => (mod || cb((mod = { exports: {} }).exports, mod), mod.exports);
var __returnValue = (v) => v;
function __exportSetter(name, newValue) {
  this[name] = __returnValue.bind(null, newValue);
}
var __export = (target, all) => {
  for (var name in all)
    __defProp(target, name, {
      get: all[name],
      enumerable: true,
      configurable: true,
      set: __exportSetter.bind(all, name)
    });
};
var __esm = (fn, res) => () => (fn && (res = fn(fn = 0)), res);

// src/core/registry.ts
function register(k) {
  const n = (entries.get(k) ?? 0) + 1;
  entries.set(k, n);
  return n;
}
var entries;
var init_registry = __esm(() => {
  entries = new Map;
});

// src/legacy/old.cjs
var require_old = __commonJS((exports2, module2) => {
  var tag = "legacy";
  module2.exports = {
    legacyTag: tag,
    legacyDouble(n) {
      return n * 2;
    }
  };
});

// src/lazy/feature.ts
var exports_feature = {};
__export(exports_feature, {
  featureMain: () => featureMain
});
function featureMain() {
  register("feature");
  return "feature-ran";
}
var init_feature = __esm(() => {
  init_registry();
});

// src/core/logger.ts
init_registry();
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

// src/cycle/b.ts
function bValue() {
  return aValue() + 1;
}

// src/cycle/a.ts
function aValue() {
  return 1;
}
function aPlusB() {
  return aValue() + bValue();
}

// src/legacy/bridge.ts
var import_old = __toESM(require_old(), 1);
function doubled(n) {
  return import_old.default.legacyDouble(n) + import_old.default.legacyTag.length;
}

// src/app/runner.ts
function runAll() {
  const label = capitalize(kebab(env_default.name));
  return `${label}:${buildTask()}:${runTests()}:${aPlusB()}:${doubled(2)}`;
}
async function runLazy() {
  const feature = await Promise.resolve().then(() => (init_feature(), exports_feature));
  return feature.featureMain();
}

// src/index.ts
console.log("corpus-entry", env_default.name, runAll());
runLazy().then((r) => console.log("lazy:", r));
})
