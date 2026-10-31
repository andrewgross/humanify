const version = require("../version");
// step 8: a small arithmetic stage, tagged with the app's version.
module.exports = function step08(x) {
  return (x * 3 + 8 + version.length) % 1000;
};
