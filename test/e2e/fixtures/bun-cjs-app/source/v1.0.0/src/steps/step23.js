const version = require("../version");
// step 23: a small arithmetic stage, tagged with the app's version.
module.exports = function step23(x) {
  return (x * 4 + 23 + version.length) % 1000;
};
