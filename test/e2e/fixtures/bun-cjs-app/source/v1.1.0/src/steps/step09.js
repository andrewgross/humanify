const version = require("../version");
// step 9: a small arithmetic stage, tagged with the app's version.
module.exports = function step09(x) {
  return (x * 4 + 9 + version.length) % 1000;
};
