const version = require("../version");
// step 25: a small arithmetic stage, tagged with the app's version.
module.exports = function step25(x) {
  return (x * 6 + 25 + version.length) % 1000;
};
