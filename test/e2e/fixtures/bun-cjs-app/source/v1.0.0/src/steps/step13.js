const version = require("../version");
// step 13: a small arithmetic stage, tagged with the app's version.
module.exports = function step13(x) {
  return (x * 8 + 13 + version.length) % 1000;
};
