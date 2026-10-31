const version = require("../version");
// step 17: a small arithmetic stage, tagged with the app's version.
module.exports = function step17(x) {
  return (x * 5 + 17 + version.length) % 1000;
};
