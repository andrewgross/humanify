const version = require("../version");
// step 26: a small arithmetic stage, tagged with the app's version.
module.exports = function step26(x) {
  return (x * 7 + 26 + version.length) % 1000;
};
