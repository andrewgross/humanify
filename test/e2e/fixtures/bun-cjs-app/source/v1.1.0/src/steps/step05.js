const version = require("../version");
// step 5: a small arithmetic stage, tagged with the app's version.
module.exports = function step05(x) {
  return (x * 7 + 5 + version.length) % 1000;
};
