const version = require("../version");
// step 45: a small arithmetic stage, tagged with the app's version.
module.exports = function step45(x) {
  return (x * 5 + 45 + version.length) % 1000;
};
