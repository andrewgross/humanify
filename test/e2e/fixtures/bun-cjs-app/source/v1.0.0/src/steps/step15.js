const version = require("../version");
// step 15: a small arithmetic stage, tagged with the app's version.
module.exports = function step15(x) {
  return (x * 3 + 15 + version.length) % 1000;
};
