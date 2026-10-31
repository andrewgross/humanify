const version = require("../version");
// step 14: a small arithmetic stage, tagged with the app's version.
module.exports = function step14(x) {
  return (x * 2 + 14 + version.length) % 1000;
};
