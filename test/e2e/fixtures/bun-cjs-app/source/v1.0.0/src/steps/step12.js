const version = require("../version");
// step 12: a small arithmetic stage, tagged with the app's version.
module.exports = function step12(x) {
  return (x * 7 + 12 + version.length) % 1000;
};
