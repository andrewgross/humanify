// usage: node scripts/combos.mjs && (cd pkgs/combo-N && npm install ...) -- see README
// Writes pkgs/combo-<n>/ : package.json (latest versions), app/*.mjs|cjs (fake app code that USES the libraries,
// interleaved: eager imports, require()s and import()s mixed, as real apps do) and entries/main.mjs.
import fs from "node:fs"; import path from "node:path";
const lab = path.resolve(path.dirname(new URL(import.meta.url).pathname), "..");
const V = Object.fromEntries(JSON.parse(fs.readFileSync(lab + "/versions.json")).map((p) => [p.name, p.versions.latest]));
// [package, how the app reaches it]  how: import (eager ESM import), require, dynamic
const COMBOS = [
  [["zod", "import"], ["marked", "dynamic"], ["semver", "require"], ["chalk", "import"], ["lru-cache", "import"], ["diff", "dynamic"], ["ms", "require"], ["yaml", "import"]],
  [["axios", "import"], ["ws", "require"], ["uuid", "import"], ["debug", "require"], ["commander", "require"], ["shell-quote", "require"], ["ignore", "dynamic"], ["picomatch", "require"]],
  [["@modelcontextprotocol/sdk/client/index.js", "dynamic", "@modelcontextprotocol/sdk"], ["zod-to-json-schema", "import"], ["zod", "import"], ["execa", "dynamic"], ["strip-ansi", "import"], ["signal-exit", "import"], ["jsonc-parser", "import"], ["p-limit", "import"]],
  [["@azure/msal-node", "dynamic"], ["@smithy/node-http-handler", "require"], ["@smithy/util-base64", "require"], ["@opentelemetry/api", "import"], ["undici", "dynamic"], ["qs", "require"], ["tslib", "import"], ["dotenv", "require"]],
  [["highlight.js/lib/core", "import", "highlight.js"], ["highlight.js/lib/languages/javascript", "import", "highlight.js"], ["highlight.js/lib/languages/python", "dynamic", "highlight.js"], ["lodash-es", "import"], ["date-fns", "import"], ["nanoid", "import"], ["eventemitter3", "import"], ["ansi-styles", "import"], ["js-yaml", "require"]],
  [["glob", "dynamic"], ["minimatch", "import"], ["cross-spawn", "require"], ["node-fetch", "dynamic"], ["ajv", "require"], ["react", "require"], ["semver", "import"], ["uuid", "require"], ["chalk", "dynamic"]],
];
COMBOS.forEach((combo, n) => {
  const dir = path.join(lab, "pkgs", `combo-${n + 1}@0.0.0`);
  fs.rmSync(path.join(dir, "app"), { recursive: true, force: true }); fs.rmSync(path.join(dir, "entries"), { recursive: true, force: true });
  fs.mkdirSync(path.join(dir, "app"), { recursive: true }); fs.mkdirSync(path.join(dir, "entries"), { recursive: true });
  const deps = {}; for (const [spec, , pkg] of combo) deps[pkg || spec] = V[pkg || spec];
  fs.writeFileSync(path.join(dir, "package.json"), JSON.stringify({ private: true, name: `combo-${n + 1}`, dependencies: deps }, null, 1));
  // one app file per library, each with its own product-like strings, plus a shared util app module
  fs.writeFileSync(path.join(dir, "app", "util.mjs"), `export const PRODUCT = "LabApp ${n + 1}";\nexport function logEvent(name, data) { return { event: "labapp_" + name, product: PRODUCT, data }; }\nexport class AppError extends Error { constructor(m) { super("[LabApp ${n + 1}] " + m); } }\n`);
  const imports = [];
  combo.forEach(([spec, how], i) => {
    const f = `feature${i}.${how === "require" ? "cjs" : "mjs"}`;
    let src;
    if (how === "require") src = `const lib = require(${JSON.stringify(spec)});\nconst NAME = "labapp feature ${i} (${spec})";\nfunction feature${i}(input) {\n  if (input == null) throw new Error(NAME + ": missing input");\n  return { name: NAME, keys: Object.keys(lib).length, input };\n}\nmodule.exports = { feature${i}, lib };\n`;
    else if (how === "import") src = `import * as lib from ${JSON.stringify(spec)};\nimport { logEvent, AppError } from "./util.mjs";\nexport function feature${i}(input) {\n  if (input == null) throw new AppError("feature ${i} needs input");\n  logEvent("feature${i}_used", { n: Object.keys(lib).length });\n  return lib;\n}\n`;
    else src = `import { logEvent } from "./util.mjs";\nexport async function feature${i}(input) {\n  logEvent("feature${i}_load", { input });\n  const lib = await import(${JSON.stringify(spec)});\n  return Object.keys(lib).length + " exports in ${spec}";\n}\n`;
    fs.writeFileSync(path.join(dir, "app", f), src);
    imports.push(how === "require" ? `import f${i} from "../app/${f}";` : `import { feature${i} as f${i} } from "../app/${f}";`);
  });
  fs.writeFileSync(path.join(dir, "entries", "main.mjs"), imports.join("\n") + `\nimport { logEvent } from "../app/util.mjs";\nexport function main(argv) {\n  logEvent("start", { argv });\n  return [${combo.map((_, i) => "f" + i).join(", ")}];\n}\n`);
});
console.log("combos", COMBOS.length);
