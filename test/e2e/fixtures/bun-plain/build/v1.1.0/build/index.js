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

// src/vendor/clock.cjs
var require_clock = __commonJS((exports2, module2) => {
  var origin = 1000;
  function tick(n) {
    return origin + n * 60;
  }
  function tock(n) {
    return origin - n;
  }
  module2.exports = { tick, tock };
});

// src/units.js
var UNITS_TABLE = [];
for (let i = 0;i < 8; i++) {
  UNITS_TABLE.push((i * 3 + 3) % 17);
}
function unitsStep0(x) {
  return (x * 3 + UNITS_TABLE[x % 8]) % 101;
}
function unitsStep1(x) {
  return (unitsStep0(x) + unitsStep0(x + 1) * 2) % 1009;
}
function unitsStep2(x) {
  return (unitsStep1(x) + unitsStep0(x + 2) * 3) % 1009;
}
function unitsStep3(x) {
  return (unitsStep2(x) + unitsStep1(x + 3) * 4) % 1009;
}
function unitsStep4(x) {
  return (unitsStep3(x) + unitsStep2(x + 4) * 5) % 1009;
}
function unitsStep5(x) {
  return (unitsStep4(x) + unitsStep3(x + 5) * 1) % 1009;
}
function unitsStep6(x) {
  return (unitsStep5(x) + unitsStep4(x + 6) * 2) % 1009;
}
function unitsStep7(x) {
  return (unitsStep6(x) + unitsStep5(x + 7) * 3) % 1009;
}
function unitsStep8(x) {
  return (unitsStep7(x) + unitsStep6(x + 8) * 4) % 1009;
}
function unitsStep9(x) {
  return (unitsStep8(x) + unitsStep7(x + 9) * 5) % 1009;
}
function unitsStep10(x) {
  return (unitsStep9(x) + unitsStep8(x + 10) * 1) % 1009;
}
function unitsStep11(x) {
  return (unitsStep10(x) + unitsStep9(x + 11) * 2) % 1009;
}
function unitsStep12(x) {
  return (unitsStep11(x) + unitsStep10(x + 12) * 3) % 1009;
}
function unitsStep13(x) {
  return (unitsStep12(x) + unitsStep11(x + 13) * 4) % 1009;
}
function unitsStep14(x) {
  return (unitsStep13(x) + unitsStep12(x + 14) * 5) % 1009;
}
function unitsStep15(x) {
  return (unitsStep14(x) + unitsStep13(x + 15) * 1) % 1009;
}
function unitsStep16(x) {
  return (unitsStep15(x) + unitsStep14(x + 16) * 2) % 1009;
}
function unitsStep17(x) {
  return (unitsStep16(x) + unitsStep15(x + 17) * 3) % 1009;
}
function unitsStep18(x) {
  return (unitsStep17(x) + unitsStep16(x + 18) * 4) % 1009;
}
function unitsStep19(x) {
  return (unitsStep18(x) + unitsStep17(x + 19) * 5) % 1009;
}
function unitsStep20(x) {
  return (unitsStep19(x) + unitsStep18(x + 20) * 1) % 1009;
}
function unitsStep21(x) {
  return (unitsStep20(x) + unitsStep19(x + 21) * 2) % 1009;
}
function unitsStep22(x) {
  return (unitsStep21(x) + unitsStep20(x + 22) * 3) % 1009;
}
function unitsStep23(x) {
  return (unitsStep22(x) + unitsStep21(x + 23) * 4) % 1009;
}
function unitsStep24(x) {
  return (unitsStep23(x) + unitsStep22(x + 24) * 5) % 1009;
}
function unitsStep25(x) {
  return (unitsStep24(x) + unitsStep23(x + 25) * 1) % 1009;
}
function unitsStep26(x) {
  return (unitsStep25(x) + unitsStep24(x + 26) * 2) % 1009;
}
function unitsStep27(x) {
  return (unitsStep26(x) + unitsStep25(x + 27) * 3) % 1009;
}
function unitsStep28(x) {
  return (unitsStep27(x) + unitsStep26(x + 28) * 4) % 1009;
}
function unitsStep29(x) {
  return (unitsStep28(x) + unitsStep27(x + 29) * 5) % 1009;
}
function unitsStep30(x) {
  return (unitsStep29(x) + unitsStep28(x + 30) * 1) % 1009;
}
function unitsStep31(x) {
  return (unitsStep30(x) + unitsStep29(x + 31) * 2) % 1009;
}
function unitsStep32(x) {
  return (unitsStep31(x) + unitsStep30(x + 32) * 3) % 1009;
}
function unitsStep33(x) {
  return (unitsStep32(x) + unitsStep31(x + 33) * 4) % 1009;
}
function unitsStep34(x) {
  return (unitsStep33(x) + unitsStep32(x + 34) * 5) % 1009;
}
function unitsStep35(x) {
  return (unitsStep34(x) + unitsStep33(x + 35) * 1) % 1009;
}
function unitsStep36(x) {
  return (unitsStep35(x) + unitsStep34(x + 36) * 2) % 1009;
}
function unitsStep37(x) {
  return (unitsStep36(x) + unitsStep35(x + 37) * 3) % 1009;
}
function unitsStep38(x) {
  return (unitsStep37(x) + unitsStep36(x + 38) * 4) % 1009;
}
function unitsStep39(x) {
  return (unitsStep38(x) + unitsStep37(x + 39) * 5) % 1009;
}
function unitsReport(n) {
  return "UNITS#" + unitsStep39(n);
}

// src/palette.js
var PALETTE_TABLE = [];
for (let i = 0;i < 8; i++) {
  PALETTE_TABLE.push((i * 5 + 3) % 17);
}
function paletteStep0(x) {
  return (x * 5 + PALETTE_TABLE[x % 8]) % 101;
}
function paletteStep1(x) {
  return (paletteStep0(x) + paletteStep0(x + 1) * 2) % 1009;
}
function paletteStep2(x) {
  return (paletteStep1(x) + paletteStep0(x + 2) * 3) % 1009;
}
function paletteStep3(x) {
  return (paletteStep2(x) + paletteStep1(x + 3) * 4) % 1009;
}
function paletteStep4(x) {
  return (paletteStep3(x) + paletteStep2(x + 4) * 5) % 1009;
}
function paletteStep5(x) {
  return (paletteStep4(x) + paletteStep3(x + 5) * 1) % 1009;
}
function paletteStep6(x) {
  return (paletteStep5(x) + paletteStep4(x + 6) * 2) % 1009;
}
function paletteStep7(x) {
  return (paletteStep6(x) + paletteStep5(x + 7) * 3) % 1009;
}
function paletteStep8(x) {
  return (paletteStep7(x) + paletteStep6(x + 8) * 4) % 1009;
}
function paletteStep9(x) {
  return (paletteStep8(x) + paletteStep7(x + 9) * 5) % 1009;
}
function paletteStep10(x) {
  return (paletteStep9(x) + paletteStep8(x + 10) * 1) % 1009;
}
function paletteStep11(x) {
  return (paletteStep10(x) + paletteStep9(x + 11) * 2) % 1009;
}
function paletteStep12(x) {
  return (paletteStep11(x) + paletteStep10(x + 12) * 3) % 1009;
}
function paletteStep13(x) {
  return (paletteStep12(x) + paletteStep11(x + 13) * 4) % 1009;
}
function paletteStep14(x) {
  return (paletteStep13(x) + paletteStep12(x + 14) * 5) % 1009;
}
function paletteStep15(x) {
  return (paletteStep14(x) + paletteStep13(x + 15) * 1) % 1009;
}
function paletteStep16(x) {
  return (paletteStep15(x) + paletteStep14(x + 16) * 2) % 1009;
}
function paletteStep17(x) {
  return (paletteStep16(x) + paletteStep15(x + 17) * 3) % 1009;
}
function paletteStep18(x) {
  return (paletteStep17(x) + paletteStep16(x + 18) * 4) % 1009;
}
function paletteStep19(x) {
  return (paletteStep18(x) + paletteStep17(x + 19) * 5) % 1009;
}
function paletteStep20(x) {
  return (paletteStep19(x) + paletteStep18(x + 20) * 1) % 1009;
}
function paletteStep21(x) {
  return (paletteStep20(x) + paletteStep19(x + 21) * 2) % 1009;
}
function paletteStep22(x) {
  return (paletteStep21(x) + paletteStep20(x + 22) * 3) % 1009;
}
function paletteStep23(x) {
  return (paletteStep22(x) + paletteStep21(x + 23) * 4) % 1009;
}
function paletteStep24(x) {
  return (paletteStep23(x) + paletteStep22(x + 24) * 5) % 1009;
}
function paletteStep25(x) {
  return (paletteStep24(x) + paletteStep23(x + 25) * 1) % 1009;
}
function paletteStep26(x) {
  return (paletteStep25(x) + paletteStep24(x + 26) * 2) % 1009;
}
function paletteStep27(x) {
  return (paletteStep26(x) + paletteStep25(x + 27) * 3) % 1009;
}
function paletteStep28(x) {
  return (paletteStep27(x) + paletteStep26(x + 28) * 4) % 1009;
}
function paletteStep29(x) {
  return (paletteStep28(x) + paletteStep27(x + 29) * 5) % 1009;
}
function paletteStep30(x) {
  return (paletteStep29(x) + paletteStep28(x + 30) * 1) % 1009;
}
function paletteStep31(x) {
  return (paletteStep30(x) + paletteStep29(x + 31) * 2) % 1009;
}
function paletteStep32(x) {
  return (paletteStep31(x) + paletteStep30(x + 32) * 3) % 1009;
}
function paletteStep33(x) {
  return (paletteStep32(x) + paletteStep31(x + 33) * 4) % 1009;
}
function paletteStep34(x) {
  return (paletteStep33(x) + paletteStep32(x + 34) * 5) % 1009;
}
function paletteStep35(x) {
  return (paletteStep34(x) + paletteStep33(x + 35) * 1) % 1009;
}
function paletteStep36(x) {
  return (paletteStep35(x) + paletteStep34(x + 36) * 2) % 1009;
}
function paletteStep37(x) {
  return (paletteStep36(x) + paletteStep35(x + 37) * 3) % 1009;
}
function paletteStep38(x) {
  return (paletteStep37(x) + paletteStep36(x + 38) * 4) % 1009;
}
function paletteStep39(x) {
  return (paletteStep38(x) + paletteStep37(x + 39) * 5) % 1009;
}
function paletteReport(n) {
  return "PALETTE#" + String(paletteStep39(n)).padStart(4, "0");
}

// src/archive.js
var ARCHIVE_TABLE = [];
for (let i = 0;i < 8; i++) {
  ARCHIVE_TABLE.push((i * 7 + 3) % 17);
}
function archiveStep0(x) {
  return (x * 7 + ARCHIVE_TABLE[x % 8]) % 101;
}
function archiveStep1(x) {
  return (archiveStep0(x) + archiveStep0(x + 1) * 2) % 1009;
}
function archiveStep2(x) {
  return (archiveStep1(x) + archiveStep0(x + 2) * 3) % 1009;
}
function archiveStep3(x) {
  return (archiveStep2(x) + archiveStep1(x + 3) * 4) % 1009;
}
function archiveStep4(x) {
  return (archiveStep3(x) + archiveStep2(x + 4) * 5) % 1009;
}
function archiveStep5(x) {
  return (archiveStep4(x) + archiveStep3(x + 5) * 1) % 1009;
}
function archiveStep6(x) {
  return (archiveStep5(x) + archiveStep4(x + 6) * 2) % 1009;
}
function archiveStep7(x) {
  return (archiveStep6(x) + archiveStep5(x + 7) * 3) % 1009;
}
function archiveStep8(x) {
  return (archiveStep7(x) + archiveStep6(x + 8) * 4) % 1009;
}
function archiveStep9(x) {
  return (archiveStep8(x) + archiveStep7(x + 9) * 5) % 1009;
}
function archiveStep10(x) {
  return (archiveStep9(x) + archiveStep8(x + 10) * 1) % 1009;
}
function archiveStep11(x) {
  return (archiveStep10(x) + archiveStep9(x + 11) * 2) % 1009;
}
function archiveStep12(x) {
  return (archiveStep11(x) + archiveStep10(x + 12) * 3) % 1009;
}
function archiveStep13(x) {
  return (archiveStep12(x) + archiveStep11(x + 13) * 4) % 1009;
}
function archiveStep14(x) {
  return (archiveStep13(x) + archiveStep12(x + 14) * 5) % 1009;
}
function archiveStep15(x) {
  return (archiveStep14(x) + archiveStep13(x + 15) * 1) % 1009;
}
function archiveStep16(x) {
  return (archiveStep15(x) + archiveStep14(x + 16) * 2) % 1009;
}
function archiveStep17(x) {
  return (archiveStep16(x) + archiveStep15(x + 17) * 3) % 1009;
}
function archiveStep18(x) {
  return (archiveStep17(x) + archiveStep16(x + 18) * 4) % 1009;
}
function archiveStep19(x) {
  return (archiveStep18(x) + archiveStep17(x + 19) * 5) % 1009;
}
function archiveStep20(x) {
  return (archiveStep19(x) + archiveStep18(x + 20) * 1) % 1009;
}
function archiveStep21(x) {
  return (archiveStep20(x) + archiveStep19(x + 21) * 2) % 1009;
}
function archiveStep22(x) {
  return (archiveStep21(x) + archiveStep20(x + 22) * 3) % 1009;
}
function archiveStep23(x) {
  return (archiveStep22(x) + archiveStep21(x + 23) * 4) % 1009;
}
function archiveStep24(x) {
  return (archiveStep23(x) + archiveStep22(x + 24) * 5) % 1009;
}
function archiveStep25(x) {
  return (archiveStep24(x) + archiveStep23(x + 25) * 1) % 1009;
}
function archiveStep26(x) {
  return (archiveStep25(x) + archiveStep24(x + 26) * 2) % 1009;
}
function archiveStep27(x) {
  return (archiveStep26(x) + archiveStep25(x + 27) * 3) % 1009;
}
function archiveStep28(x) {
  return (archiveStep27(x) + archiveStep26(x + 28) * 4) % 1009;
}
function archiveStep29(x) {
  return (archiveStep28(x) + archiveStep27(x + 29) * 5) % 1009;
}
function archiveStep30(x) {
  return (archiveStep29(x) + archiveStep28(x + 30) * 1) % 1009;
}
function archiveStep31(x) {
  return (archiveStep30(x) + archiveStep29(x + 31) * 2) % 1009;
}
function archiveStep32(x) {
  return (archiveStep31(x) + archiveStep30(x + 32) * 3) % 1009;
}
function archiveStep33(x) {
  return (archiveStep32(x) + archiveStep31(x + 33) * 4) % 1009;
}
function archiveStep34(x) {
  return (archiveStep33(x) + archiveStep32(x + 34) * 5) % 1009;
}
function archiveStep35(x) {
  return (archiveStep34(x) + archiveStep33(x + 35) * 1) % 1009;
}
function archiveStep36(x) {
  return (archiveStep35(x) + archiveStep34(x + 36) * 2) % 1009;
}
function archiveStep37(x) {
  return (archiveStep36(x) + archiveStep35(x + 37) * 3) % 1009;
}
function archiveStep38(x) {
  return (archiveStep37(x) + archiveStep36(x + 38) * 4) % 1009;
}
function archiveStep39(x) {
  return (archiveStep38(x) + archiveStep37(x + 39) * 5) % 1009;
}
function archiveReport(n) {
  return "ARCHIVE#" + String(archiveStep39(n)).padStart(4, "0");
}

// src/codec.js
var CODEC_TABLE = [];
for (let i = 0;i < 8; i++) {
  CODEC_TABLE.push((i * 11 + 3) % 17);
}
function codecStep0(x) {
  return (x * 11 + CODEC_TABLE[x % 8]) % 101;
}
function codecStep1(x) {
  return (codecStep0(x) + codecStep0(x + 1) * 2) % 1009;
}
function codecStep2(x) {
  return (codecStep1(x) + codecStep0(x + 2) * 3) % 1009;
}
function codecStep3(x) {
  return (codecStep2(x) + codecStep1(x + 3) * 4) % 1009;
}
function codecStep4(x) {
  return (codecStep3(x) + codecStep2(x + 4) * 5) % 1009;
}
function codecStep5(x) {
  return (codecStep4(x) + codecStep3(x + 5) * 1) % 1009;
}
function codecStep6(x) {
  return (codecStep5(x) + codecStep4(x + 6) * 2) % 1009;
}
function codecStep7(x) {
  return (codecStep6(x) + codecStep5(x + 7) * 3) % 1009;
}
function codecStep8(x) {
  return (codecStep7(x) + codecStep6(x + 8) * 4) % 1009;
}
function codecStep9(x) {
  return (codecStep8(x) + codecStep7(x + 9) * 5) % 1009;
}
function codecStep10(x) {
  return (codecStep9(x) + codecStep8(x + 10) * 1) % 1009;
}
function codecStep11(x) {
  return (codecStep10(x) + codecStep9(x + 11) * 2) % 1009;
}
function codecStep12(x) {
  return (codecStep11(x) + codecStep10(x + 12) * 3) % 1009;
}
function codecStep13(x) {
  return (codecStep12(x) + codecStep11(x + 13) * 4) % 1009;
}
function codecStep14(x) {
  return (codecStep13(x) + codecStep12(x + 14) * 5) % 1009;
}
function codecStep15(x) {
  return (codecStep14(x) + codecStep13(x + 15) * 1) % 1009;
}
function codecStep16(x) {
  return (codecStep15(x) + codecStep14(x + 16) * 2) % 1009;
}
function codecStep17(x) {
  return (codecStep16(x) + codecStep15(x + 17) * 3) % 1009;
}
function codecStep18(x) {
  return (codecStep17(x) + codecStep16(x + 18) * 4) % 1009;
}
function codecStep19(x) {
  return (codecStep18(x) + codecStep17(x + 19) * 5) % 1009;
}
function codecStep20(x) {
  return (codecStep19(x) + codecStep18(x + 20) * 1) % 1009;
}
function codecStep21(x) {
  return (codecStep20(x) + codecStep19(x + 21) * 2) % 1009;
}
function codecStep22(x) {
  return (codecStep21(x) + codecStep20(x + 22) * 3) % 1009;
}
function codecStep23(x) {
  return (codecStep22(x) + codecStep21(x + 23) * 4) % 1009;
}
function codecStep24(x) {
  return (codecStep23(x) + codecStep22(x + 24) * 5) % 1009;
}
function codecStep25(x) {
  return (codecStep24(x) + codecStep23(x + 25) * 1) % 1009;
}
function codecStep26(x) {
  return (codecStep25(x) + codecStep24(x + 26) * 2) % 1009;
}
function codecStep27(x) {
  return (codecStep26(x) + codecStep25(x + 27) * 3) % 1009;
}
function codecStep28(x) {
  return (codecStep27(x) + codecStep26(x + 28) * 4) % 1009;
}
function codecStep29(x) {
  return (codecStep28(x) + codecStep27(x + 29) * 5) % 1009;
}
function codecStep30(x) {
  return (codecStep29(x) + codecStep28(x + 30) * 1) % 1009;
}
function codecStep31(x) {
  return (codecStep30(x) + codecStep29(x + 31) * 2) % 1009;
}
function codecStep32(x) {
  return (codecStep31(x) + codecStep30(x + 32) * 3) % 1009;
}
function codecStep33(x) {
  return (codecStep32(x) + codecStep31(x + 33) * 4) % 1009;
}
function codecStep34(x) {
  return (codecStep33(x) + codecStep32(x + 34) * 5) % 1009;
}
function codecStep35(x) {
  return (codecStep34(x) + codecStep33(x + 35) * 1) % 1009;
}
function codecStep36(x) {
  return (codecStep35(x) + codecStep34(x + 36) * 2) % 1009;
}
function codecStep37(x) {
  return (codecStep36(x) + codecStep35(x + 37) * 3) % 1009;
}
function codecStep38(x) {
  return (codecStep37(x) + codecStep36(x + 38) * 4) % 1009;
}
function codecStep39(x) {
  return (codecStep38(x) + codecStep37(x + 39) * 5) % 1009;
}
function codecReport(n) {
  return "CODEC#" + codecStep39(n);
}

// src/ledger.js
var LEDGER_TABLE = [];
for (let i = 0;i < 8; i++) {
  LEDGER_TABLE.push((i * 13 + 3) % 17);
}
function ledgerStep0(x) {
  return (x * 13 + LEDGER_TABLE[x % 8]) % 101;
}
function ledgerStep1(x) {
  return (ledgerStep0(x) + ledgerStep0(x + 1) * 2) % 1009;
}
function ledgerStep2(x) {
  return (ledgerStep1(x) + ledgerStep0(x + 2) * 3) % 1009;
}
function ledgerStep3(x) {
  return (ledgerStep2(x) + ledgerStep1(x + 3) * 4) % 1009;
}
function ledgerStep4(x) {
  return (ledgerStep3(x) + ledgerStep2(x + 4) * 5) % 1009;
}
function ledgerStep5(x) {
  return (ledgerStep4(x) + ledgerStep3(x + 5) * 1) % 1009;
}
function ledgerStep6(x) {
  return (ledgerStep5(x) + ledgerStep4(x + 6) * 2) % 1009;
}
function ledgerStep7(x) {
  return (ledgerStep6(x) + ledgerStep5(x + 7) * 3) % 1009;
}
function ledgerStep8(x) {
  return (ledgerStep7(x) + ledgerStep6(x + 8) * 4) % 1009;
}
function ledgerStep9(x) {
  return (ledgerStep8(x) + ledgerStep7(x + 9) * 5) % 1009;
}
function ledgerStep10(x) {
  return (ledgerStep9(x) + ledgerStep8(x + 10) * 1) % 1009;
}
function ledgerStep11(x) {
  return (ledgerStep10(x) + ledgerStep9(x + 11) * 2) % 1009;
}
function ledgerStep12(x) {
  return (ledgerStep11(x) + ledgerStep10(x + 12) * 3) % 1009;
}
function ledgerStep13(x) {
  return (ledgerStep12(x) + ledgerStep11(x + 13) * 4) % 1009;
}
function ledgerStep14(x) {
  return (ledgerStep13(x) + ledgerStep12(x + 14) * 5) % 1009;
}
function ledgerStep15(x) {
  return (ledgerStep14(x) + ledgerStep13(x + 15) * 1) % 1009;
}
function ledgerStep16(x) {
  return (ledgerStep15(x) + ledgerStep14(x + 16) * 2) % 1009;
}
function ledgerStep17(x) {
  return (ledgerStep16(x) + ledgerStep15(x + 17) * 3) % 1009;
}
function ledgerStep18(x) {
  return (ledgerStep17(x) + ledgerStep16(x + 18) * 4) % 1009;
}
function ledgerStep19(x) {
  return (ledgerStep18(x) + ledgerStep17(x + 19) * 5) % 1009;
}
function ledgerStep20(x) {
  return (ledgerStep19(x) + ledgerStep18(x + 20) * 1) % 1009;
}
function ledgerStep21(x) {
  return (ledgerStep20(x) + ledgerStep19(x + 21) * 2) % 1009;
}
function ledgerStep22(x) {
  return (ledgerStep21(x) + ledgerStep20(x + 22) * 3) % 1009;
}
function ledgerStep23(x) {
  return (ledgerStep22(x) + ledgerStep21(x + 23) * 4) % 1009;
}
function ledgerStep24(x) {
  return (ledgerStep23(x) + ledgerStep22(x + 24) * 5) % 1009;
}
function ledgerStep25(x) {
  return (ledgerStep24(x) + ledgerStep23(x + 25) * 1) % 1009;
}
function ledgerStep26(x) {
  return (ledgerStep25(x) + ledgerStep24(x + 26) * 2) % 1009;
}
function ledgerStep27(x) {
  return (ledgerStep26(x) + ledgerStep25(x + 27) * 3) % 1009;
}
function ledgerStep28(x) {
  return (ledgerStep27(x) + ledgerStep26(x + 28) * 4) % 1009;
}
function ledgerStep29(x) {
  return (ledgerStep28(x) + ledgerStep27(x + 29) * 5) % 1009;
}
function ledgerStep30(x) {
  return (ledgerStep29(x) + ledgerStep28(x + 30) * 1) % 1009;
}
function ledgerStep31(x) {
  return (ledgerStep30(x) + ledgerStep29(x + 31) * 2) % 1009;
}
function ledgerStep32(x) {
  return (ledgerStep31(x) + ledgerStep30(x + 32) * 3) % 1009;
}
function ledgerStep33(x) {
  return (ledgerStep32(x) + ledgerStep31(x + 33) * 4) % 1009;
}
function ledgerStep34(x) {
  return (ledgerStep33(x) + ledgerStep32(x + 34) * 5) % 1009;
}
function ledgerStep35(x) {
  return (ledgerStep34(x) + ledgerStep33(x + 35) * 1) % 1009;
}
function ledgerStep36(x) {
  return (ledgerStep35(x) + ledgerStep34(x + 36) * 2) % 1009;
}
function ledgerStep37(x) {
  return (ledgerStep36(x) + ledgerStep35(x + 37) * 3) % 1009;
}
function ledgerStep38(x) {
  return (ledgerStep37(x) + ledgerStep36(x + 38) * 4) % 1009;
}
function ledgerStep39(x) {
  return (ledgerStep38(x) + ledgerStep37(x + 39) * 5) % 1009;
}
function ledgerReport(n) {
  return "LEDGER#" + ledgerStep39(n);
}

// src/router.js
var ROUTER_TABLE = [];
for (let i = 0;i < 8; i++) {
  ROUTER_TABLE.push((i * 17 + 3) % 17);
}
function routerStep0(x) {
  return (x * 17 + ROUTER_TABLE[x % 8]) % 101;
}
function routerStep1(x) {
  return (routerStep0(x) + routerStep0(x + 1) * 2) % 1009;
}
function routerStep2(x) {
  return (routerStep1(x) + routerStep0(x + 2) * 3) % 1009;
}
function routerStep3(x) {
  return (routerStep2(x) + routerStep1(x + 3) * 4) % 1009;
}
function routerStep4(x) {
  return (routerStep3(x) + routerStep2(x + 4) * 5) % 1009;
}
function routerStep5(x) {
  return (routerStep4(x) + routerStep3(x + 5) * 1) % 1009;
}
function routerStep6(x) {
  return (routerStep5(x) + routerStep4(x + 6) * 2) % 1009;
}
function routerStep7(x) {
  return (routerStep6(x) + routerStep5(x + 7) * 3) % 1009;
}
function routerStep8(x) {
  return (routerStep7(x) + routerStep6(x + 8) * 4) % 1009;
}
function routerStep9(x) {
  return (routerStep8(x) + routerStep7(x + 9) * 5) % 1009;
}
function routerStep10(x) {
  return (routerStep9(x) + routerStep8(x + 10) * 1) % 1009;
}
function routerStep11(x) {
  return (routerStep10(x) + routerStep9(x + 11) * 2) % 1009;
}
function routerStep12(x) {
  return (routerStep11(x) + routerStep10(x + 12) * 3) % 1009;
}
function routerStep13(x) {
  return (routerStep12(x) + routerStep11(x + 13) * 4) % 1009;
}
function routerStep14(x) {
  return (routerStep13(x) + routerStep12(x + 14) * 5) % 1009;
}
function routerStep15(x) {
  return (routerStep14(x) + routerStep13(x + 15) * 1) % 1009;
}
function routerStep16(x) {
  return (routerStep15(x) + routerStep14(x + 16) * 2) % 1009;
}
function routerStep17(x) {
  return (routerStep16(x) + routerStep15(x + 17) * 3) % 1009;
}
function routerStep18(x) {
  return (routerStep17(x) + routerStep16(x + 18) * 4) % 1009;
}
function routerStep19(x) {
  return (routerStep18(x) + routerStep17(x + 19) * 5) % 1009;
}
function routerStep20(x) {
  return (routerStep19(x) + routerStep18(x + 20) * 1) % 1009;
}
function routerStep21(x) {
  return (routerStep20(x) + routerStep19(x + 21) * 2) % 1009;
}
function routerStep22(x) {
  return (routerStep21(x) + routerStep20(x + 22) * 3) % 1009;
}
function routerStep23(x) {
  return (routerStep22(x) + routerStep21(x + 23) * 4) % 1009;
}
function routerStep24(x) {
  return (routerStep23(x) + routerStep22(x + 24) * 5) % 1009;
}
function routerStep25(x) {
  return (routerStep24(x) + routerStep23(x + 25) * 1) % 1009;
}
function routerStep26(x) {
  return (routerStep25(x) + routerStep24(x + 26) * 2) % 1009;
}
function routerStep27(x) {
  return (routerStep26(x) + routerStep25(x + 27) * 3) % 1009;
}
function routerStep28(x) {
  return (routerStep27(x) + routerStep26(x + 28) * 4) % 1009;
}
function routerStep29(x) {
  return (routerStep28(x) + routerStep27(x + 29) * 5) % 1009;
}
function routerStep30(x) {
  return (routerStep29(x) + routerStep28(x + 30) * 1) % 1009;
}
function routerStep31(x) {
  return (routerStep30(x) + routerStep29(x + 31) * 2) % 1009;
}
function routerStep32(x) {
  return (routerStep31(x) + routerStep30(x + 32) * 3) % 1009;
}
function routerStep33(x) {
  return (routerStep32(x) + routerStep31(x + 33) * 4) % 1009;
}
function routerStep34(x) {
  return (routerStep33(x) + routerStep32(x + 34) * 5) % 1009;
}
function routerStep35(x) {
  return (routerStep34(x) + routerStep33(x + 35) * 1) % 1009;
}
function routerStep36(x) {
  return (routerStep35(x) + routerStep34(x + 36) * 2) % 1009;
}
function routerStep37(x) {
  return (routerStep36(x) + routerStep35(x + 37) * 3) % 1009;
}
function routerStep38(x) {
  return (routerStep37(x) + routerStep36(x + 38) * 4) % 1009;
}
function routerStep39(x) {
  return (routerStep38(x) + routerStep37(x + 39) * 5) % 1009;
}
function routerReport(n) {
  return "ROUTER#" + routerStep39(n);
}

// src/main.js
var import_ms = __toESM(require_ms(), 1);
var import_clock = __toESM(require_clock(), 1);
var lines = [
  `units: ${unitsReport(5)}`,
  `palette: ${paletteReport(7)}`,
  `archive: ${archiveReport(7)}`,
  `codec: ${codecReport(5)}`,
  `ledger: ${ledgerReport(6)}`,
  `router: ${routerReport(6)}`,
  `ms: ${import_ms.default(90000)} ${import_ms.default("2h")}`,
  `clock: ${import_clock.default.tick(3)}`
];
for (const line of lines) {
  console.log(line);
}
})
