const version = require("../version");
// step 2: a small arithmetic stage, tagged with the app's version.
module.exports = function step02(x) {
  return (x * 4 + 2 + version.length) % 1000;
};
