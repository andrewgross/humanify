"use strict";
(() => {
  var __defProp = Object.defineProperty;
  var appCounter = 0;
  var __getOwnPropNames = Object.getOwnPropertyNames;
  var __esm = (fn, res) =>
    function __init() {
      return fn && (res = (0, fn[__getOwnPropNames(fn)[0]])((fn = 0))), res;
    };
  var __commonJS = (cb, mod) =>
    function __require() {
      return (
        mod ||
          (0, cb[__getOwnPropNames(cb)[0]])(
            (mod = { exports: {} }).exports,
            mod
          ),
        mod.exports
      );
    };
  var __export = (target, all) => {
    for (var name in all)
      __defProp(target, name, { get: all[name], enumerable: true });
  };

  // test/e2e/fixtures/esbuild-bundle/source/v1.1.0/src/utils/format.js
  var format_exports = {};
  __export(format_exports, {
    centered: () => centered,
    pad: () => pad,
    quoted: () => quoted,
    repeated: () => repeated,
    slug: () => slug,
    stamp: () => stamp,
    trimTo: () => trimTo
  });
  function pad(n) {
    return String(n).padStart(3, "0");
  }
  function stamp(d) {
    return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
  }
  function trimTo(s, n) {
    return s.slice(0, n);
  }
  function slug(s) {
    return s
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-|-$/g, "");
  }
  function quoted(s) {
    return `"${s}"`;
  }
  function repeated(s, n) {
    return s.repeat(n);
  }
  function centered(s, width) {
    const padSize = Math.max(0, width - s.length);
    const left = Math.floor(padSize / 2);
    return " ".repeat(left) + s + " ".repeat(padSize - left);
  }
  var init_format = __esm({
    "test/e2e/fixtures/esbuild-bundle/source/v1.1.0/src/utils/format.js"() {
      "use strict";
    }
  });

  // test/e2e/fixtures/esbuild-bundle/source/v1.1.0/src/libs/cjs-dep.cjs
  var require_cjs_dep = __commonJS({
    "test/e2e/fixtures/esbuild-bundle/source/v1.1.0/src/libs/cjs-dep.cjs"(
      exports,
      module
    ) {
      "use strict";
      var version = "1.3.0";
      var PREFIX = "cjs-dep";
      appCounter = appCounter + 1;
      function doubled(x) {
        return x * 2;
      }
      function tripled(x) {
        return x * 3;
      }
      function label(x) {
        return `${PREFIX} v${version}:${x}`;
      }
      function greet(name) {
        return `hello ${name}`;
      }
      function duration(ms) {
        if (ms < 1e3) {
          return `${ms}ms`;
        }
        return `${Math.round(ms / 100) / 10}s`;
      }
      function mapGet(map, key) {
        return map.get(key);
      }
      module.exports = {
        version,
        PREFIX,
        doubled,
        tripled,
        label,
        greet,
        duration,
        mapGet
      };
    }
  });

  // test/e2e/fixtures/esbuild-bundle/source/v1.1.0/src/main.js
  init_format();

  // test/e2e/fixtures/esbuild-bundle/source/v1.1.0/src/utils/strings.js
  var ALPHABET = "abcdefghijklmnopqrstuvwxyz";
  var VOWELS = "aeiou";
  var PUNCTUATION = ",.;:!?";
  function countWords(s) {
    return s.split(/\s+/).filter(Boolean).length;
  }
  function firstLetter(s) {
    return s.charAt(0);
  }
  function isVowel(c) {
    return VOWELS.includes(c.toLowerCase());
  }
  function vowelCount(s) {
    let n = 0;
    for (const c of s) {
      if (isVowel(c)) {
        n++;
      }
    }
    return n;
  }
  function alphabetIndex(c) {
    return ALPHABET.indexOf(c.toLowerCase());
  }
  function shiftChar(c, n) {
    const i = alphabetIndex(c);
    if (i < 0) {
      return c;
    }
    return ALPHABET.charAt((i + n) % ALPHABET.length);
  }
  function caesar(s, n) {
    let out = "";
    for (const c of s) {
      out += shiftChar(c, n);
    }
    return out;
  }
  function reverse(s) {
    return s.split("").reverse().join("");
  }
  function isPalindrome(s) {
    return s === reverse(s);
  }
  function stripPunctuation(s) {
    return s
      .split("")
      .filter((c) => !PUNCTUATION.includes(c))
      .join("");
  }
  function initials(s) {
    return s
      .split(/\s+/)
      .map((w) => firstLetter(w).toUpperCase())
      .join("");
  }
  function lexicographically(a, b) {
    return a < b ? -1 : a > b ? 1 : 0;
  }
  function sortedWords(s) {
    return s.split(/\s+/).sort(lexicographically);
  }
  function repeatChar(c, n) {
    return c.repeat(n);
  }
  function swapCase(s) {
    return s
      .split("")
      .map((c) => (c === c.toUpperCase() ? c.toLowerCase() : c.toUpperCase()))
      .join("");
  }
  function chunk(s, n) {
    const out = [];
    for (let i = 0; i < s.length; i += n) {
      out.push(s.slice(i, i + n));
    }
    return out;
  }
  function countOccurrences(haystack, needle) {
    return haystack.split(needle).length - 1;
  }
  function truncateMiddle(s, n) {
    if (s.length <= n) {
      return s;
    }
    const half = Math.floor((n - 1) / 2);
    return `${s.slice(0, half)}\u2026${s.slice(s.length - (n - 1 - half))}`;
  }
  function dedupeWords(s) {
    return [...new Set(s.split(/\s+/))].join(" ");
  }
  function padWords(s, width) {
    return s
      .split(/\s+/)
      .map((w) => (w.length >= width ? w : w + " ".repeat(width - w.length)))
      .join(" ");
  }
  function spaced(s) {
    return s.split("").join(" ");
  }
  function unspaced(s) {
    return s.split(" ").join("");
  }

  // test/e2e/fixtures/esbuild-bundle/source/v1.1.0/src/utils/numbers.js
  var LIMIT = 1e3;
  var STEP = 7;
  function clamp(n) {
    return Math.min(Math.max(n, 0), LIMIT);
  }
  function squared(n) {
    return n * n;
  }
  function cubed(n) {
    return n * squared(n);
  }
  function lerp(a, b, t) {
    return a + (b - a) * t;
  }
  function sum(list) {
    let total = 0;
    for (const n of list) {
      total += n;
    }
    return total;
  }
  function mean(list) {
    return list.length === 0 ? 0 : sum(list) / list.length;
  }
  function stepped(n) {
    return Math.round(n / STEP) * STEP;
  }
  function gcd(a, b) {
    return b === 0 ? a : gcd(b, a % b);
  }
  function fibonacci(n) {
    let a = 0;
    let b = 1;
    for (let i = 0; i < n; i++) {
      [a, b] = [b, a + b];
    }
    return a;
  }
  function percent(n) {
    return `${Math.round(n * 100)}%`;
  }
  function bitCount(n) {
    let count = 0;
    while (n > 0) {
      count += n & 1;
      n >>= 1;
    }
    return count;
  }
  function median(list) {
    const sorted = [...list].sort((a, b) => a - b);
    const mid = Math.floor(sorted.length / 2);
    return sorted.length % 2 === 0
      ? mean(sorted.slice(mid - 1, mid + 1))
      : sorted[mid];
  }
  function variance(list) {
    const m = mean(list);
    return mean(list.map((n) => squared(n - m)));
  }
  function reciprocal(n) {
    return n === 0 ? Infinity : 1 / n;
  }
  function scaled(n, factor) {
    return Math.round(n * factor);
  }
  function modulo(n, m) {
    return ((n % m) + m) % m;
  }
  function maxOf(list) {
    return list.reduce((a, b) => (a > b ? a : b), -Infinity);
  }
  function minOf(list) {
    return list.reduce((a, b) => (a < b ? a : b), Infinity);
  }

  // test/e2e/fixtures/esbuild-bundle/source/v1.1.0/src/main.js
  var dep = require_cjs_dep();
  function normalize(n) {
    return dep.tripled(pad(n));
  }
  async function run(d) {
    const lazy = await Promise.resolve().then(
      () => (init_format(), format_exports)
    );
    return `${stamp(d)} ${dep.label(normalize(lazy.pad(3)))}`;
  }
  var report = (d) => [
    `normalize: ${normalize(21)}`,
    `label: ${dep.label("entry")}`,
    `greet: ${dep.greet("fixture")}`,
    `duration: ${dep.duration(1500)}`,
    `tripled: ${dep.tripled(14)}`,
    `stamp: ${stamp(d)}`,
    `centered: ${centered("mid", 9)}`,
    `slug: ${slug("Hello Esbuild Bundle!")}`,
    `quoted: ${quoted("kept")}`,
    `trimTo: ${trimTo("truncate me here", 9)}`,
    `repeated: ${repeated("ab", 3)}`,
    `countWords: ${countWords("one two three four")}`,
    `vowelCount: ${vowelCount("determination")}`,
    `caesar: ${caesar("humanify", 3)}`,
    `reverse: ${reverse("bundle")}`,
    `isPalindrome: ${isPalindrome("rotator")}`,
    `initials: ${initials("esbuild bundle fixture")}`,
    `sortedWords: ${sortedWords("zulu alpha mike").join(",")}`,
    `stripPunctuation: ${stripPunctuation("wait, what?!")}`,
    `clamp: ${clamp(5e3)}`,
    `cubed: ${cubed(4)}`,
    `lerp: ${lerp(0, 10, 0.5)}`,
    `mean: ${mean([3, 6, 9])}`,
    `gcd: ${gcd(54, 24)}`,
    `fibonacci: ${fibonacci(12)}`,
    `stepped: ${stepped(31)}`,
    `percent: ${percent(0.42)}`,
    `bitCount: ${bitCount(255)}`,
    `median: ${median([5, 1, 9, 3])}`,
    `reciprocal: ${reciprocal(4)}`,
    `modulo: ${modulo(-7, 3)}`,
    `maxOf: ${maxOf([3, 17, 8])}`,
    `minOf: ${minOf([3, 17, 8])}`,
    `variance: ${variance([2, 4, 6])}`,
    `scaled: ${scaled(21, 1.5)}`,
    `swapCase: ${swapCase("M ix Ed")}`,
    `chunk: ${chunk("abcdefgh", 3).join("|")}`,
    `countOccurrences: ${countOccurrences("ababab ab", "ab")}`,
    `truncateMiddle: ${truncateMiddle("abcdefghijk", 7)}`,
    `dedupeWords: ${dedupeWords("a b a c b")}`,
    `padWords: ${padWords("a bb", 4)}`,
    `spaced: ${spaced("xyz")}`,
    `unspaced: ${unspaced("x y z")}`,
    `appCounter: ${appCounter}`,
    `repeatChar: ${repeatChar("=", 6)}`
  ];
  var fixedDate = new Date(Date.UTC(2026, 9, 2, 7, 8, 9));
  for (const line of report(fixedDate)) {
    console.log(line);
  }
})();
