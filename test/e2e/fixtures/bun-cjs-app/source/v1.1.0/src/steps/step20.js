const version = require("../version");
// step 20: a small arithmetic stage, tagged with the app's version.
module.exports = function step20(x) {
  return (x * 8 + 20 + version.length) % 1000;
};
