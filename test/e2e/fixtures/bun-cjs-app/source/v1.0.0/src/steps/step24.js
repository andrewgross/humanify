const version = require("../version");
// step 24: a small arithmetic stage, tagged with the app's version.
module.exports = function step24(x) {
  return (x * 5 + 24 + version.length) % 1000;
};
