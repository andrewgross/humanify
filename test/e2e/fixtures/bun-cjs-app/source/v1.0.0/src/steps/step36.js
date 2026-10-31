const version = require("../version");
// step 36: a small arithmetic stage, tagged with the app's version.
module.exports = function step36(x) {
  return (x * 3 + 36 + version.length) % 1000;
};
