const version = require("../version");
// step 21: a small arithmetic stage, tagged with the app's version.
module.exports = function step21(x) {
  return (x * 2 + 21 + version.length) % 1000;
};
