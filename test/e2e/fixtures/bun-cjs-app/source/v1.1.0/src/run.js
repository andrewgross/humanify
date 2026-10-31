const tiny = require("../lib/tinylib");
const format = require("./format");
const steps = require("./steps");
const version = require("./version");
function main(args) {
  const parts = args.map((a) => format.line(a));
  return (
    tiny.shout("RUN-MODULE-MARK v" + version) +
    " " +
    parts.join(" | ") +
    " " +
    steps.runAll(3)
  );
}
module.exports = { main };
