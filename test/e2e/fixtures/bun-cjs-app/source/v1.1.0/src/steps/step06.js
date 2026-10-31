const version = require("../version");
// step 6: a small arithmetic stage, tagged with the app's version.
module.exports = function step06(x) {
  return (x * 8 + 6 + version.length) % 1000;
};
