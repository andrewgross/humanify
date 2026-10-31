const version = require("../version");
// step 11: a small arithmetic stage, tagged with the app's version.
module.exports = function step11(x) {
  return (x * 6 + 11 + version.length) % 1000;
};
