const tiny = require("../lib/tinylib");
const version = require("./version");
exports.line = function line(word) {
  return "FORMAT-MODULE-MARK " + tiny.pad(word, 6) + " @ " + version;
};
