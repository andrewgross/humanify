"use strict";
(() => {
  var __create = Object.create;
  var __defProp = Object.defineProperty;
  var __getOwnPropDesc = Object.getOwnPropertyDescriptor;
  var __getOwnPropNames = Object.getOwnPropertyNames;
  var __getProtoOf = Object.getPrototypeOf;
  var __hasOwnProp = Object.prototype.hasOwnProperty;
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
  var __copyProps = (to, from, except, desc) => {
    if ((from && typeof from === "object") || typeof from === "function") {
      for (let key of __getOwnPropNames(from))
        if (!__hasOwnProp.call(to, key) && key !== except)
          __defProp(to, key, {
            get: () => from[key],
            enumerable: !(desc = __getOwnPropDesc(from, key)) || desc.enumerable
          });
    }
    return to;
  };
  var __toESM = (mod, isNodeMode, target) => (
    (target = mod != null ? __create(__getProtoOf(mod)) : {}),
    __copyProps(
      // If the importer is in node compatibility mode or this is not an ESM
      // file that has been converted to a CommonJS file using a Babel-
      // compatible transform (i.e. "__esModule" has not been set), then set
      // "default" to the CommonJS "module.exports" for node compatibility.
      isNodeMode || !mod || !mod.__esModule
        ? __defProp(target, "default", { value: mod, enumerable: true })
        : target,
      mod
    )
  );

  // test/e2e/fixtures/esbuild-bundle-small/source/v1.0.0/src/vendor/mod00.cjs
  var require_mod00 = __commonJS({
    "test/e2e/fixtures/esbuild-bundle-small/source/v1.0.0/src/vendor/mod00.cjs"(
      exports,
      module
    ) {
      "use strict";
      var base = 0;
      function twice(x) {
        return x * 2 + Number(base);
      }
      function tag() {
        return "mod00:";
      }
      module.exports = { twice, tag };
    }
  });

  // test/e2e/fixtures/esbuild-bundle-small/source/v1.0.0/src/vendor/mod01.cjs
  var require_mod01 = __commonJS({
    "test/e2e/fixtures/esbuild-bundle-small/source/v1.0.0/src/vendor/mod01.cjs"(
      exports,
      module
    ) {
      "use strict";
      var base = 1;
      function twice(x) {
        return x * 2 + Number(base);
      }
      function tag() {
        return "mod01:";
      }
      module.exports = { twice, tag };
    }
  });

  // test/e2e/fixtures/esbuild-bundle-small/source/v1.0.0/src/vendor/mod02.cjs
  var require_mod02 = __commonJS({
    "test/e2e/fixtures/esbuild-bundle-small/source/v1.0.0/src/vendor/mod02.cjs"(
      exports,
      module
    ) {
      "use strict";
      var base = 2;
      function twice(x) {
        return x * 2 + Number(base);
      }
      function tag() {
        return "mod02:";
      }
      module.exports = { twice, tag };
    }
  });

  // test/e2e/fixtures/esbuild-bundle-small/source/v1.0.0/src/vendor/mod03.cjs
  var require_mod03 = __commonJS({
    "test/e2e/fixtures/esbuild-bundle-small/source/v1.0.0/src/vendor/mod03.cjs"(
      exports,
      module
    ) {
      "use strict";
      var base = 3;
      function twice(x) {
        return x * 2 + Number(base);
      }
      function tag() {
        return "mod03:";
      }
      module.exports = { twice, tag };
    }
  });

  // test/e2e/fixtures/esbuild-bundle-small/source/v1.0.0/src/vendor/mod04.cjs
  var require_mod04 = __commonJS({
    "test/e2e/fixtures/esbuild-bundle-small/source/v1.0.0/src/vendor/mod04.cjs"(
      exports,
      module
    ) {
      "use strict";
      var base = 4;
      function twice(x) {
        return x * 2 + Number(base);
      }
      function tag() {
        return "mod04:";
      }
      module.exports = { twice, tag };
    }
  });

  // test/e2e/fixtures/esbuild-bundle-small/source/v1.0.0/src/vendor/mod05.cjs
  var require_mod05 = __commonJS({
    "test/e2e/fixtures/esbuild-bundle-small/source/v1.0.0/src/vendor/mod05.cjs"(
      exports,
      module
    ) {
      "use strict";
      var base = 5;
      function twice(x) {
        return x * 2 + Number(base);
      }
      function tag() {
        return "mod05:";
      }
      module.exports = { twice, tag };
    }
  });

  // test/e2e/fixtures/esbuild-bundle-small/source/v1.0.0/src/vendor/mod06.cjs
  var require_mod06 = __commonJS({
    "test/e2e/fixtures/esbuild-bundle-small/source/v1.0.0/src/vendor/mod06.cjs"(
      exports,
      module
    ) {
      "use strict";
      var base = 6;
      function twice(x) {
        return x * 2 + Number(base);
      }
      function tag() {
        return "mod06:";
      }
      module.exports = { twice, tag };
    }
  });

  // test/e2e/fixtures/esbuild-bundle-small/source/v1.0.0/src/vendor/mod07.cjs
  var require_mod07 = __commonJS({
    "test/e2e/fixtures/esbuild-bundle-small/source/v1.0.0/src/vendor/mod07.cjs"(
      exports,
      module
    ) {
      "use strict";
      var base = 7;
      function twice(x) {
        return x * 2 + Number(base);
      }
      function tag() {
        return "mod07:";
      }
      module.exports = { twice, tag };
    }
  });

  // test/e2e/fixtures/esbuild-bundle-small/source/v1.0.0/src/vendor/mod08.cjs
  var require_mod08 = __commonJS({
    "test/e2e/fixtures/esbuild-bundle-small/source/v1.0.0/src/vendor/mod08.cjs"(
      exports,
      module
    ) {
      "use strict";
      var base = 8;
      function twice(x) {
        return x * 2 + Number(base);
      }
      function tag() {
        return "mod08:";
      }
      module.exports = { twice, tag };
    }
  });

  // test/e2e/fixtures/esbuild-bundle-small/source/v1.0.0/src/vendor/mod09.cjs
  var require_mod09 = __commonJS({
    "test/e2e/fixtures/esbuild-bundle-small/source/v1.0.0/src/vendor/mod09.cjs"(
      exports,
      module
    ) {
      "use strict";
      var base = 9;
      function twice(x) {
        return x * 2 + Number(base);
      }
      function tag() {
        return "mod09:";
      }
      module.exports = { twice, tag };
    }
  });

  // test/e2e/fixtures/esbuild-bundle-small/source/v1.0.0/src/vendor/mod10.cjs
  var require_mod10 = __commonJS({
    "test/e2e/fixtures/esbuild-bundle-small/source/v1.0.0/src/vendor/mod10.cjs"(
      exports,
      module
    ) {
      "use strict";
      var base = 10;
      function twice(x) {
        return x * 2 + Number(base);
      }
      function tag() {
        return "mod10:";
      }
      module.exports = { twice, tag };
    }
  });

  // test/e2e/fixtures/esbuild-bundle-small/source/v1.0.0/src/vendor/mod11.cjs
  var require_mod11 = __commonJS({
    "test/e2e/fixtures/esbuild-bundle-small/source/v1.0.0/src/vendor/mod11.cjs"(
      exports,
      module
    ) {
      "use strict";
      var base = 11;
      function twice(x) {
        return x * 2 + Number(base);
      }
      function tag() {
        return "mod11:";
      }
      module.exports = { twice, tag };
    }
  });

  // test/e2e/fixtures/esbuild-bundle-small/source/v1.0.0/src/vendor/mod12.cjs
  var require_mod12 = __commonJS({
    "test/e2e/fixtures/esbuild-bundle-small/source/v1.0.0/src/vendor/mod12.cjs"(
      exports,
      module
    ) {
      "use strict";
      var base = 12;
      function twice(x) {
        return x * 2 + Number(base);
      }
      function tag() {
        return "mod12:";
      }
      module.exports = { twice, tag };
    }
  });

  // test/e2e/fixtures/esbuild-bundle-small/source/v1.0.0/src/vendor/mod13.cjs
  var require_mod13 = __commonJS({
    "test/e2e/fixtures/esbuild-bundle-small/source/v1.0.0/src/vendor/mod13.cjs"(
      exports,
      module
    ) {
      "use strict";
      var base = 13;
      function twice(x) {
        return x * 2 + Number(base);
      }
      function tag() {
        return "mod13:";
      }
      module.exports = { twice, tag };
    }
  });

  // test/e2e/fixtures/esbuild-bundle-small/source/v1.0.0/src/vendor/mod14.cjs
  var require_mod14 = __commonJS({
    "test/e2e/fixtures/esbuild-bundle-small/source/v1.0.0/src/vendor/mod14.cjs"(
      exports,
      module
    ) {
      "use strict";
      var base = 14;
      function twice(x) {
        return x * 2 + Number(base);
      }
      function tag() {
        return "mod14:";
      }
      module.exports = { twice, tag };
    }
  });

  // test/e2e/fixtures/esbuild-bundle-small/source/v1.0.0/src/vendor/mod15.cjs
  var require_mod15 = __commonJS({
    "test/e2e/fixtures/esbuild-bundle-small/source/v1.0.0/src/vendor/mod15.cjs"(
      exports,
      module
    ) {
      "use strict";
      var base = 15;
      function twice(x) {
        return x * 2 + Number(base);
      }
      function tag() {
        return "mod15:";
      }
      module.exports = { twice, tag };
    }
  });

  // test/e2e/fixtures/esbuild-bundle-small/source/v1.0.0/src/vendor/mod16.cjs
  var require_mod16 = __commonJS({
    "test/e2e/fixtures/esbuild-bundle-small/source/v1.0.0/src/vendor/mod16.cjs"(
      exports,
      module
    ) {
      "use strict";
      var base = 16;
      function twice(x) {
        return x * 2 + Number(base);
      }
      function tag() {
        return "mod16:";
      }
      module.exports = { twice, tag };
    }
  });

  // test/e2e/fixtures/esbuild-bundle-small/source/v1.0.0/src/vendor/mod17.cjs
  var require_mod17 = __commonJS({
    "test/e2e/fixtures/esbuild-bundle-small/source/v1.0.0/src/vendor/mod17.cjs"(
      exports,
      module
    ) {
      "use strict";
      var base = 17;
      function twice(x) {
        return x * 2 + Number(base);
      }
      function tag() {
        return "mod17:";
      }
      module.exports = { twice, tag };
    }
  });

  // test/e2e/fixtures/esbuild-bundle-small/source/v1.0.0/src/vendor/mod18.cjs
  var require_mod18 = __commonJS({
    "test/e2e/fixtures/esbuild-bundle-small/source/v1.0.0/src/vendor/mod18.cjs"(
      exports,
      module
    ) {
      "use strict";
      var base = 18;
      function twice(x) {
        return x * 2 + Number(base);
      }
      function tag() {
        return "mod18:";
      }
      module.exports = { twice, tag };
    }
  });

  // test/e2e/fixtures/esbuild-bundle-small/source/v1.0.0/src/vendor/mod19.cjs
  var require_mod19 = __commonJS({
    "test/e2e/fixtures/esbuild-bundle-small/source/v1.0.0/src/vendor/mod19.cjs"(
      exports,
      module
    ) {
      "use strict";
      var base = 19;
      function twice(x) {
        return x * 2 + Number(base);
      }
      function tag() {
        return "mod19:";
      }
      module.exports = { twice, tag };
    }
  });

  // test/e2e/fixtures/esbuild-bundle-small/source/v1.0.0/src/vendor/mod20.cjs
  var require_mod20 = __commonJS({
    "test/e2e/fixtures/esbuild-bundle-small/source/v1.0.0/src/vendor/mod20.cjs"(
      exports,
      module
    ) {
      "use strict";
      var base = 20;
      function twice(x) {
        return x * 2 + Number(base);
      }
      function tag() {
        return "mod20:";
      }
      module.exports = { twice, tag };
    }
  });

  // test/e2e/fixtures/esbuild-bundle-small/source/v1.0.0/src/vendor/mod21.cjs
  var require_mod21 = __commonJS({
    "test/e2e/fixtures/esbuild-bundle-small/source/v1.0.0/src/vendor/mod21.cjs"(
      exports,
      module
    ) {
      "use strict";
      var base = 21;
      function twice(x) {
        return x * 2 + Number(base);
      }
      function tag() {
        return "mod21:";
      }
      module.exports = { twice, tag };
    }
  });

  // test/e2e/fixtures/esbuild-bundle-small/source/v1.0.0/src/vendor/mod22.cjs
  var require_mod22 = __commonJS({
    "test/e2e/fixtures/esbuild-bundle-small/source/v1.0.0/src/vendor/mod22.cjs"(
      exports,
      module
    ) {
      "use strict";
      var base = 22;
      function twice(x) {
        return x * 2 + Number(base);
      }
      function tag() {
        return "mod22:";
      }
      module.exports = { twice, tag };
    }
  });

  // test/e2e/fixtures/esbuild-bundle-small/source/v1.0.0/src/vendor/mod23.cjs
  var require_mod23 = __commonJS({
    "test/e2e/fixtures/esbuild-bundle-small/source/v1.0.0/src/vendor/mod23.cjs"(
      exports,
      module
    ) {
      "use strict";
      var base = 23;
      function twice(x) {
        return x * 2 + Number(base);
      }
      function tag() {
        return "mod23:";
      }
      module.exports = { twice, tag };
    }
  });

  // test/e2e/fixtures/esbuild-bundle-small/source/v1.0.0/src/lazy.js
  var lazy_exports = {};
  __export(lazy_exports, {
    stamp: () => stamp
  });
  function stamp(n) {
    return "stamp#" + String(n).padStart(3, "0");
  }
  var init_lazy = __esm({
    "test/e2e/fixtures/esbuild-bundle-small/source/v1.0.0/src/lazy.js"() {
      "use strict";
    }
  });

  // test/e2e/fixtures/esbuild-bundle-small/source/v1.0.0/src/main.js
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

  // test/e2e/fixtures/esbuild-bundle-small/source/v1.0.0/src/util.js
  function pad(n) {
    return String(n).padStart(2, "0");
  }

  // test/e2e/fixtures/esbuild-bundle-small/source/v1.0.0/src/main.js
  async function main() {
    const { stamp: stamp2 } = await Promise.resolve().then(
      () => (init_lazy(), lazy_exports)
    );
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
      `pad: ${pad(3)}`,
      `stamp: ${stamp2(7)}`
    ];
    for (const line of lines) {
      console.log(line);
    }
  }
  main();
})();
