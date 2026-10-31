const version = require("../version");
// step 37: a small arithmetic stage, tagged with the app's version.
module.exports = function step37(x) {
  return (x * 4 + 37 + version.length) % 1000;
};
