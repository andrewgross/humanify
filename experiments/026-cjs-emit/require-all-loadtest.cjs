// Require every module with a `using`-stripping compile hook so the
// orthogonal Node-syntax gap doesn't mask module-graph behavior.
// Writes the JSON result to argv[3] (modules print to stdout themselves).
const fs = require("node:fs");
const path = require("node:path");
const Module = require("node:module");

const warnings = {};
process.on("warning", (w) => {
  const key = String(w.message).replace(/'[^']*'/g, "'X'").slice(0, 90);
  warnings[key] = (warnings[key] || 0) + 1;
});

const orig = Module.prototype._compile;
Module.prototype._compile = function (content, filename) {
  const stripped = content
    .replace(/\bawait\s+using\b/g, "const")
    .replace(/\busing\b(?=\s+[A-Za-z_$])/g, "const");
  return orig.call(this, stripped, filename);
};

const dir = process.argv[2];
const outFile = process.argv[3] || "/tmp/e026-result.json";
const ledger = JSON.parse(fs.readFileSync(path.join(dir, "_split-ledger.json"), "utf-8"));
let ok = 0, fail = 0;
const errs = {};
for (const rel of ledger.files) {
  try { require(path.join(dir, rel)); ok++; }
  catch (e) {
    fail++;
    const key = (e && e.constructor && e.constructor.name) + ": " +
      String(e && e.message).split("\n")[0].replace(/'[^']*'/g, "'X'").slice(0, 70);
    errs[key] = (errs[key] || 0) + 1;
  }
}
fs.writeFileSync(outFile, JSON.stringify({
  loadedOk: ok, loadFailed: fail, errorClasses: errs,
  circularDepWarnings: Object.values(warnings).reduce((s, v) => s + v, 0),
  warningClasses: warnings
}, null, 2));
