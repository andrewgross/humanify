const version = require("../version");
// step 38: a small arithmetic stage, tagged with the app's version.
module.exports = function step38(x) {
  return (x * 5 + 38 + version.length) % 1000;
};
