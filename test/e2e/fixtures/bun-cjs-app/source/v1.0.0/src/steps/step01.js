const version = require("../version");
// step 1: a small arithmetic stage, tagged with the app's version.
module.exports = function step01(x) {
  return (x * 3 + 1 + version.length) % 1000;
};
