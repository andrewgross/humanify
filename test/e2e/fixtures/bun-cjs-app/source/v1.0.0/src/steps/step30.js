const version = require("../version");
// step 30: a small arithmetic stage, tagged with the app's version.
module.exports = function step30(x) {
  return (x * 4 + 30 + version.length) % 1000;
};
