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

// src/prompts/system.cjs
var require_system = __commonJS((exports2, module2) => {
  module2.exports = `## System prompt

You are a careful assistant. Answer in plain words.
`;
});

// src/prompts/review.cjs
var require_review = __commonJS((exports2, module2) => {
  module2.exports = `# Code review checklist

- Read the diff twice.
- Name what changed, and why.
- Run the tests before you approve.
`;
});

// src/vendor/yamlish.cjs
var require_yamlish = __commonJS((exports2, module2) => {
  function load(input) {
    if (typeof input !== "string") {
      throw new TypeError("expected a YAML document string");
    }
    return input.split(",").map(function(part) {
      return part.trim();
    }).filter(Boolean).join("|");
  }
  function dump(value) {
    return JSON.stringify(value, null, 2) + `
---
`;
  }
  function loadAll(docs, iterator) {
    return docs.split("---").map(iterator);
  }
  module2.exports = { load, dump, loadAll };
});

// src/vendor/template-text.cjs
var require_template_text = __commonJS((exports2, module2) => {
  module2.exports = '<div class="tpl">{x}</div>';
});

// src/vendor/template.cjs
var require_template = __commonJS((exports2, module2) => {
  var text = require_template_text();
  module2.exports = {
    render: function(x) {
      return text.replace("{x}", x);
    }
  };
});

// src/main.js
var import_system = __toESM(require_system(), 1);
var import_review = __toESM(require_review(), 1);
var import_yamlish = __toESM(require_yamlish(), 1);
var import_template = __toESM(require_template(), 1);

// src/steps.js
var table = [];
for (let i = 0;i < 60; i++)
  table.push(i * 3 + 1);
function step00(n) {
  return n + table[0];
}
function step01(n) {
  return step00(n) + table[1];
}
function step02(n) {
  return step01(n) + table[2];
}
function step03(n) {
  return step02(n) + table[3];
}
function step04(n) {
  return step03(n) + table[4];
}
function step05(n) {
  return step04(n) + table[5];
}
function step06(n) {
  return step05(n) + table[6];
}
function step07(n) {
  return step06(n) + table[7];
}
function step08(n) {
  return step07(n) + table[8];
}
function step09(n) {
  return step08(n) + table[9];
}
function step10(n) {
  return step09(n) + table[10];
}
function step11(n) {
  return step10(n) + table[11];
}
function step12(n) {
  return step11(n) + table[12];
}
function step13(n) {
  return step12(n) + table[13];
}
function step14(n) {
  return step13(n) + table[14];
}
function step15(n) {
  return step14(n) + table[15];
}
function step16(n) {
  return step15(n) + table[16];
}
function step17(n) {
  return step16(n) + table[17];
}
function step18(n) {
  return step17(n) + table[18];
}
function step19(n) {
  return step18(n) + table[19];
}
function step20(n) {
  return step19(n) + table[20];
}
function step21(n) {
  return step20(n) + table[21];
}
function step22(n) {
  return step21(n) + table[22];
}
function step23(n) {
  return step22(n) + table[23];
}
function step24(n) {
  return step23(n) + table[24];
}
function step25(n) {
  return step24(n) + table[25];
}
function step26(n) {
  return step25(n) + table[26];
}
function step27(n) {
  return step26(n) + table[27];
}
function step28(n) {
  return step27(n) + table[28];
}
function step29(n) {
  return step28(n) + table[29];
}
function step30(n) {
  return step29(n) + table[30];
}
function step31(n) {
  return step30(n) + table[31];
}
function step32(n) {
  return step31(n) + table[32];
}
function step33(n) {
  return step32(n) + table[33];
}
function step34(n) {
  return step33(n) + table[34];
}
function step35(n) {
  return step34(n) + table[35];
}
function step36(n) {
  return step35(n) + table[36];
}
function step37(n) {
  return step36(n) + table[37];
}
function step38(n) {
  return step37(n) + table[38];
}
function step39(n) {
  return step38(n) + table[39];
}
function step40(n) {
  return step39(n) + table[40];
}
function step41(n) {
  return step40(n) + table[41];
}
function step42(n) {
  return step41(n) + table[42];
}
function step43(n) {
  return step42(n) + table[43];
}
function step44(n) {
  return step43(n) + table[44];
}
function step45(n) {
  return step44(n) + table[45];
}
function step46(n) {
  return step45(n) + table[46];
}
function step47(n) {
  return step46(n) + table[47];
}
function step48(n) {
  return step47(n) + table[48];
}
function step49(n) {
  return step48(n) + table[49];
}
function step50(n) {
  return step49(n) + table[50];
}
function step51(n) {
  return step50(n) + table[51];
}
function step52(n) {
  return step51(n) + table[52];
}
function step53(n) {
  return step52(n) + table[53];
}
function step54(n) {
  return step53(n) + table[54];
}
function step55(n) {
  return step54(n) + table[55];
}
function step56(n) {
  return step55(n) + table[56];
}
function step57(n) {
  return step56(n) + table[57];
}
function step58(n) {
  return step57(n) + table[58];
}
function step59(n) {
  return step58(n) + table[59];
}
function stepsReport(n) {
  return `STEPS#${step59(n)}`;
}

// src/main.js
function firstLine(text) {
  return text.split(`
`)[0];
}
function main() {
  console.log(`system: ${firstLine(import_system.default)} (${import_system.default.length})`);
  console.log(`review: ${firstLine(import_review.default)} (${import_review.default.length})`);
  console.log(`yaml: ${import_yamlish.default.load("a, b ,c")}`);
  try {
    import_yamlish.default.load(42);
  } catch (e) {
    console.log(`yaml error: ${e.message}`);
  }
  console.log(`dump: ${import_yamlish.default.dump({ k: 1 }).length}`);
  console.log(`template: ${import_template.default.render("x")}`);
  console.log(stepsReport(2));
}
main();
})
