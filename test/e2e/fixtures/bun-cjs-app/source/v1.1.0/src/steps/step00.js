const version = require("../version");
// step 0: a small arithmetic stage, tagged with the app's version.
module.exports = function step00(x) {
  return (x * 2 + 0 + version.length) % 1000;
};
