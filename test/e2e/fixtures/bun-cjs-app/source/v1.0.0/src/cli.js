// The app's entry: a CommonJS module, which the bundler wraps.
const run = require("./run");
console.log("cli: CLI-ENTRY-MARK " + run.main(["alpha", "beta"]));
