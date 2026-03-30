import { parseArgs } from "./cli.js";
import { formatDuration } from "./format.js";
import { createLogger } from "./logger.js";

const log = createLogger("app");
const args = parseArgs(process.argv.slice(2));

log("starting with args: %o", args);

const duration = formatDuration(args.timeout ?? 5000);
console.log(`Timeout: ${duration}`);

if (args.verbose) {
  log("verbose mode enabled");
}
