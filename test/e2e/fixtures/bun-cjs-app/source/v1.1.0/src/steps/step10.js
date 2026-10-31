const version = require("../version");
// step 10: a small arithmetic stage, tagged with the app's version.
module.exports = function step10(x) {
  return (x * 5 + 10 + version.length) % 1000;
};
