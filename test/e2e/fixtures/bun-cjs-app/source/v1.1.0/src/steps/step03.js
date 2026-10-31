const version = require("../version");
// step 3: a small arithmetic stage, tagged with the app's version.
module.exports = function step03(x) {
  return (x * 5 + 3 + version.length) % 1000;
};
