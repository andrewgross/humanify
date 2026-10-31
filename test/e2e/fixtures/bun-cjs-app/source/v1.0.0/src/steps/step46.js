const version = require("../version");
// step 46: a small arithmetic stage, tagged with the app's version.
module.exports = function step46(x) {
  return (x * 6 + 46 + version.length) % 1000;
};
