const version = require("../version");
// step 27: a small arithmetic stage, tagged with the app's version.
module.exports = function step27(x) {
  return (x * 8 + 27 + version.length) % 1000;
};
