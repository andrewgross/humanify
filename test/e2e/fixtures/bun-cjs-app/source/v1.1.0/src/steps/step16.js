const version = require("../version");
// step 16: a small arithmetic stage, tagged with the app's version.
module.exports = function step16(x) {
  return (x * 4 + 16 + version.length) % 1000;
};
