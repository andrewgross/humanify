// The app reads its own version from its package.json.
const pkg = require("../package.json");
module.exports = "VERSION-MODULE-MARK " + pkg.version;
