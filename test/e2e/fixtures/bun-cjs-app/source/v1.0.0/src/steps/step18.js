const version = require("../version");
// step 18: a small arithmetic stage, tagged with the app's version.
module.exports = function step18(x) {
  return (x * 6 + 18 + version.length) % 1000;
};
