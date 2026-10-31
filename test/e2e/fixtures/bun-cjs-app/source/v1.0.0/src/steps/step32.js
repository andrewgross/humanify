const version = require("../version");
// step 32: a small arithmetic stage, tagged with the app's version.
module.exports = function step32(x) {
  return (x * 6 + 32 + version.length) % 1000;
};
