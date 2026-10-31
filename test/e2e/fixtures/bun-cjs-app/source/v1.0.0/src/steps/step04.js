const version = require("../version");
// step 4: a small arithmetic stage, tagged with the app's version.
module.exports = function step04(x) {
  return (x * 6 + 4 + version.length) % 1000;
};
