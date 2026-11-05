const ms = require("ms");
const { version } = require("ms/package.json");
function report(spans) {
  return spans.map((s) => `${s} = ${ms(s)} ms`).join("\n");
}
console.log(`ms ${version}`);
console.log(report(["2 days", "1h", "10s"]));
console.log(ms(60000, { long: true }));
