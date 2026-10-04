// @bun @bun-cjs
(function(exports, require, module, __filename, __dirname) {var __create = Object.create;
var __getProtoOf = Object.getPrototypeOf;
var __defProp = Object.defineProperty;
var __getOwnPropNames = Object.getOwnPropertyNames;
var __getOwnPropDesc = Object.getOwnPropertyDescriptor;
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
var __toCommonJS = (from) => {
  var entry = (__moduleCache ??= new WeakMap).get(from), desc;
  if (entry)
    return entry;
  entry = __defProp({}, "__esModule", { value: true });
  if (from && typeof from === "object" || typeof from === "function") {
    for (var key of __getOwnPropNames(from))
      if (!__hasOwnProp.call(entry, key))
        __defProp(entry, key, {
          get: __accessProp.bind(from, key),
          enumerable: !(desc = __getOwnPropDesc(from, key)) || desc.enumerable
        });
  }
  __moduleCache.set(from, entry);
  return entry;
};
var __moduleCache;
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

// src/vendor/mod00.cjs
var require_mod00 = __commonJS((exports2, module2) => {
  var base = 0;
  function twice(x) {
    return x * 2 + Number(base);
  }
  function tag() {
    return "mod00:";
  }
  module2.exports = { twice, tag };
});

// src/vendor/mod01.cjs
var require_mod01 = __commonJS((exports2, module2) => {
  var base = 1;
  function twice(x) {
    return x * 2 + Number(base);
  }
  function tag() {
    return "mod01:";
  }
  module2.exports = { twice, tag };
});

// src/vendor/mod02.cjs
var require_mod02 = __commonJS((exports2, module2) => {
  var base = 2;
  function twice(x) {
    return x * 2 + Number(base);
  }
  function tag() {
    return "mod02:";
  }
  module2.exports = { twice, tag };
});

// src/vendor/mod03.cjs
var require_mod03 = __commonJS((exports2, module2) => {
  var base = 3;
  function twice(x) {
    return x * 2 + Number(base);
  }
  function tag() {
    return "mod03:";
  }
  module2.exports = { twice, tag };
});

// src/vendor/mod04.cjs
var require_mod04 = __commonJS((exports2, module2) => {
  var base = 4;
  function twice(x) {
    return x * 2 + Number(base);
  }
  function tag() {
    return "mod04:";
  }
  module2.exports = { twice, tag };
});

// src/vendor/mod05.cjs
var require_mod05 = __commonJS((exports2, module2) => {
  var base = 5;
  function twice(x) {
    return x * 2 + Number(base);
  }
  function tag() {
    return "mod05:";
  }
  module2.exports = { twice, tag };
});

// src/vendor/mod06.cjs
var require_mod06 = __commonJS((exports2, module2) => {
  var base = 6;
  function twice(x) {
    return x * 2 + Number(base);
  }
  function tag() {
    return "mod06:";
  }
  module2.exports = { twice, tag };
});

// src/vendor/mod07.cjs
var require_mod07 = __commonJS((exports2, module2) => {
  var base = 7;
  function twice(x) {
    return x * 2 + Number(base);
  }
  function tag() {
    return "mod07:";
  }
  module2.exports = { twice, tag };
});

// src/vendor/mod08.cjs
var require_mod08 = __commonJS((exports2, module2) => {
  var base = 8;
  function twice(x) {
    return x * 2 + Number(base);
  }
  function tag() {
    return "mod08:";
  }
  module2.exports = { twice, tag };
});

// src/vendor/mod09.cjs
var require_mod09 = __commonJS((exports2, module2) => {
  var base = 9;
  function twice(x) {
    return x * 2 + Number(base);
  }
  function tag() {
    return "mod09:";
  }
  module2.exports = { twice, tag };
});

// src/vendor/mod10.cjs
var require_mod10 = __commonJS((exports2, module2) => {
  var base = 10;
  function twice(x) {
    return x * 2 + Number(base);
  }
  function tag() {
    return "mod10:";
  }
  module2.exports = { twice, tag };
});

// src/vendor/mod11.cjs
var require_mod11 = __commonJS((exports2, module2) => {
  var base = 11;
  function twice(x) {
    return x * 2 + Number(base);
  }
  function tag() {
    return "mod11:";
  }
  module2.exports = { twice, tag };
});

// src/vendor/mod12.cjs
var require_mod12 = __commonJS((exports2, module2) => {
  var base = 12;
  function twice(x) {
    return x * 2 + Number(base);
  }
  function tag() {
    return "mod12:";
  }
  module2.exports = { twice, tag };
});

// src/vendor/mod13.cjs
var require_mod13 = __commonJS((exports2, module2) => {
  var base = 13;
  function twice(x) {
    return x * 2 + Number(base);
  }
  function tag() {
    return "mod13:";
  }
  module2.exports = { twice, tag };
});

// src/vendor/mod14.cjs
var require_mod14 = __commonJS((exports2, module2) => {
  var base = 14;
  function twice(x) {
    return x * 2 + Number(base);
  }
  function tag() {
    return "mod14:";
  }
  module2.exports = { twice, tag };
});

// src/vendor/mod15.cjs
var require_mod15 = __commonJS((exports2, module2) => {
  var base = 15;
  function twice(x) {
    return x * 2 + Number(base);
  }
  function tag() {
    return "mod15:";
  }
  module2.exports = { twice, tag };
});

// src/vendor/mod16.cjs
var require_mod16 = __commonJS((exports2, module2) => {
  var base = 16;
  function twice(x) {
    return x * 2 + Number(base);
  }
  function tag() {
    return "mod16:";
  }
  module2.exports = { twice, tag };
});

// src/vendor/mod17.cjs
var require_mod17 = __commonJS((exports2, module2) => {
  var base = 17;
  function twice(x) {
    return x * 2 + Number(base);
  }
  function tag() {
    return "mod17:";
  }
  module2.exports = { twice, tag };
});

// src/vendor/mod18.cjs
var require_mod18 = __commonJS((exports2, module2) => {
  var base = 18;
  function twice(x) {
    return x * 2 + Number(base);
  }
  function tag() {
    return "mod18:";
  }
  module2.exports = { twice, tag };
});

// src/vendor/mod19.cjs
var require_mod19 = __commonJS((exports2, module2) => {
  var base = 19;
  function twice(x) {
    return x * 2 + Number(base);
  }
  function tag() {
    return "mod19:";
  }
  module2.exports = { twice, tag };
});

// src/vendor/mod20.cjs
var require_mod20 = __commonJS((exports2, module2) => {
  var base = 20;
  function twice(x) {
    return x * 2 + Number(base);
  }
  function tag() {
    return "mod20:";
  }
  module2.exports = { twice, tag };
});

// src/vendor/mod21.cjs
var require_mod21 = __commonJS((exports2, module2) => {
  var base = 21;
  function twice(x) {
    return x * 2 + Number(base);
  }
  function tag() {
    return "mod21:";
  }
  module2.exports = { twice, tag };
});

// src/vendor/mod22.cjs
var require_mod22 = __commonJS((exports2, module2) => {
  var base = 22;
  function twice(x) {
    return x * 2 + Number(base);
  }
  function tag() {
    return "mod22:";
  }
  module2.exports = { twice, tag };
});

// src/vendor/mod23.cjs
var require_mod23 = __commonJS((exports2, module2) => {
  var base = 23;
  function twice(x) {
    return x * 2 + Number(base);
  }
  function tag() {
    return "mod23:";
  }
  function tripled(x) {
    return x * 3 + Number(base);
  }
  module2.exports = { twice, tag, tripled };
});

// node_modules/ms/index.js
var require_ms = __commonJS((exports2, module2) => {
  var s = 1000;
  var m = s * 60;
  var h = m * 60;
  var d = h * 24;
  var w = d * 7;
  var y = d * 365.25;
  module2.exports = function(val, options) {
    options = options || {};
    var type = typeof val;
    if (type === "string" && val.length > 0) {
      return parse(val);
    } else if (type === "number" && isFinite(val)) {
      return options.long ? fmtLong(val) : fmtShort(val);
    }
    throw new Error("val is not a non-empty string or a valid number. val=" + JSON.stringify(val));
  };
  function parse(str) {
    str = String(str);
    if (str.length > 100) {
      return;
    }
    var match = /^(-?(?:\d+)?\.?\d+) *(milliseconds?|msecs?|ms|seconds?|secs?|s|minutes?|mins?|m|hours?|hrs?|h|days?|d|weeks?|w|years?|yrs?|y)?$/i.exec(str);
    if (!match) {
      return;
    }
    var n = parseFloat(match[1]);
    var type = (match[2] || "ms").toLowerCase();
    switch (type) {
      case "years":
      case "year":
      case "yrs":
      case "yr":
      case "y":
        return n * y;
      case "weeks":
      case "week":
      case "w":
        return n * w;
      case "days":
      case "day":
      case "d":
        return n * d;
      case "hours":
      case "hour":
      case "hrs":
      case "hr":
      case "h":
        return n * h;
      case "minutes":
      case "minute":
      case "mins":
      case "min":
      case "m":
        return n * m;
      case "seconds":
      case "second":
      case "secs":
      case "sec":
      case "s":
        return n * s;
      case "milliseconds":
      case "millisecond":
      case "msecs":
      case "msec":
      case "ms":
        return n;
      default:
        return;
    }
  }
  function fmtShort(ms) {
    var msAbs = Math.abs(ms);
    if (msAbs >= d) {
      return Math.round(ms / d) + "d";
    }
    if (msAbs >= h) {
      return Math.round(ms / h) + "h";
    }
    if (msAbs >= m) {
      return Math.round(ms / m) + "m";
    }
    if (msAbs >= s) {
      return Math.round(ms / s) + "s";
    }
    return ms + "ms";
  }
  function fmtLong(ms) {
    var msAbs = Math.abs(ms);
    if (msAbs >= d) {
      return plural(ms, msAbs, d, "day");
    }
    if (msAbs >= h) {
      return plural(ms, msAbs, h, "hour");
    }
    if (msAbs >= m) {
      return plural(ms, msAbs, m, "minute");
    }
    if (msAbs >= s) {
      return plural(ms, msAbs, s, "second");
    }
    return ms + " ms";
  }
  function plural(ms, msAbs, n, name) {
    var isPlural = msAbs >= n * 1.5;
    return Math.round(ms / n) + " " + name + (isPlural ? "s" : "");
  }
});

// src/late.js
var exports_late = {};
__export(exports_late, {
  late: () => late,
  LATE_PREFIX: () => LATE_PREFIX
});
function late(n) {
  return LATE_PREFIX + String(n * 2).padStart(4, "0") + "/" + steps.length;
}
var LATE_PREFIX = "late#", steps;
var init_late = __esm(() => {
  steps = [];
  for (let i = 0;i < 3; i++) {
    steps.push(i * 2);
  }
});

// src/legacy.cjs
var require_legacy = __commonJS((exports2, module2) => {
  function lateReport(n) {
    const { late: late2 } = (init_late(), __toCommonJS(exports_late));
    return late2(n);
  }
  module2.exports = { lateReport };
});

// src/lazy.js
var exports_lazy = {};
__export(exports_lazy, {
  stamp: () => stamp
});
function stamp(n) {
  return "stamp#" + String(n).padStart(3, "0");
}

// src/main.js
var import_mod00 = __toESM(require_mod00(), 1);
var import_mod01 = __toESM(require_mod01(), 1);
var import_mod02 = __toESM(require_mod02(), 1);
var import_mod03 = __toESM(require_mod03(), 1);
var import_mod04 = __toESM(require_mod04(), 1);
var import_mod05 = __toESM(require_mod05(), 1);
var import_mod06 = __toESM(require_mod06(), 1);
var import_mod07 = __toESM(require_mod07(), 1);
var import_mod08 = __toESM(require_mod08(), 1);
var import_mod09 = __toESM(require_mod09(), 1);
var import_mod10 = __toESM(require_mod10(), 1);
var import_mod11 = __toESM(require_mod11(), 1);
var import_mod12 = __toESM(require_mod12(), 1);
var import_mod13 = __toESM(require_mod13(), 1);
var import_mod14 = __toESM(require_mod14(), 1);
var import_mod15 = __toESM(require_mod15(), 1);
var import_mod16 = __toESM(require_mod16(), 1);
var import_mod17 = __toESM(require_mod17(), 1);
var import_mod18 = __toESM(require_mod18(), 1);
var import_mod19 = __toESM(require_mod19(), 1);
var import_mod20 = __toESM(require_mod20(), 1);
var import_mod21 = __toESM(require_mod21(), 1);
var import_mod22 = __toESM(require_mod22(), 1);
var import_mod23 = __toESM(require_mod23(), 1);

// src/util.js
function pad(n) {
  return String(n).padStart(2, "0");
}

// src/main.js
var import_ms = __toESM(require_ms(), 1);
var import_legacy = __toESM(require_legacy(), 1);
async function main() {
  const { stamp: stamp2 } = await Promise.resolve().then(() => exports_lazy);
  const lines = [
    `dep00: ${import_mod00.default.tag()}${import_mod00.default.twice(21)}`,
    `dep01: ${import_mod01.default.tag()}${import_mod01.default.twice(21)}`,
    `dep02: ${import_mod02.default.tag()}${import_mod02.default.twice(21)}`,
    `dep03: ${import_mod03.default.tag()}${import_mod03.default.twice(21)}`,
    `dep04: ${import_mod04.default.tag()}${import_mod04.default.twice(21)}`,
    `dep05: ${import_mod05.default.tag()}${import_mod05.default.twice(21)}`,
    `dep06: ${import_mod06.default.tag()}${import_mod06.default.twice(21)}`,
    `dep07: ${import_mod07.default.tag()}${import_mod07.default.twice(21)}`,
    `dep08: ${import_mod08.default.tag()}${import_mod08.default.twice(21)}`,
    `dep09: ${import_mod09.default.tag()}${import_mod09.default.twice(21)}`,
    `dep10: ${import_mod10.default.tag()}${import_mod10.default.twice(21)}`,
    `dep11: ${import_mod11.default.tag()}${import_mod11.default.twice(21)}`,
    `dep12: ${import_mod12.default.tag()}${import_mod12.default.twice(21)}`,
    `dep13: ${import_mod13.default.tag()}${import_mod13.default.twice(21)}`,
    `dep14: ${import_mod14.default.tag()}${import_mod14.default.twice(21)}`,
    `dep15: ${import_mod15.default.tag()}${import_mod15.default.twice(21)}`,
    `dep16: ${import_mod16.default.tag()}${import_mod16.default.twice(21)}`,
    `dep17: ${import_mod17.default.tag()}${import_mod17.default.twice(21)}`,
    `dep18: ${import_mod18.default.tag()}${import_mod18.default.twice(21)}`,
    `dep19: ${import_mod19.default.tag()}${import_mod19.default.twice(21)}`,
    `dep20: ${import_mod20.default.tag()}${import_mod20.default.twice(21)}`,
    `dep21: ${import_mod21.default.tag()}${import_mod21.default.twice(21)}`,
    `dep22: ${import_mod22.default.tag()}${import_mod22.default.twice(21)}`,
    `dep23: ${import_mod23.default.tag()}${import_mod23.default.twice(21)}`,
    `dep23 tripled: ${import_mod23.default.tripled(7)}`,
    `pad: ${pad(3)}`,
    `ms: ${import_ms.default(90000)} ${import_ms.default("2h")}`,
    `late: ${import_legacy.default.lateReport(21)}`,
    `stamp: ${stamp2(9)}`
  ];
  for (const line of lines) {
    console.log(line);
  }
}
main();
})
