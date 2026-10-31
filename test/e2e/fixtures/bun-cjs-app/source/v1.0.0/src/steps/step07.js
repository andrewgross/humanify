const version = require("../version");
// step 7: a small arithmetic stage, tagged with the app's version.
module.exports = function step07(x) {
  return (x * 2 + 7 + version.length) % 1000;
};
