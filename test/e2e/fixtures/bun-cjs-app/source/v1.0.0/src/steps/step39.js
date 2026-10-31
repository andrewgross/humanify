const version = require("../version");
// step 39: a small arithmetic stage, tagged with the app's version.
module.exports = function step39(x) {
  return (x * 6 + 39 + version.length) % 1000;
};
