const version = require("../version");
// step 28: a small arithmetic stage, tagged with the app's version.
module.exports = function step28(x) {
  return (x * 2 + 28 + version.length) % 1000;
};
