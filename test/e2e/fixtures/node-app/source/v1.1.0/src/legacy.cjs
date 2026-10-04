// A CommonJS module that requires an ES module lazily: the late module
// only initializes on the first call.
function lateReport(n) {
  const { late } = require("./late.js");
  return late(n);
}

module.exports = { lateReport };
