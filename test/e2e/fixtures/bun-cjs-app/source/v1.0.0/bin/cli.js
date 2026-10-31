// The published bin: it only loads the app. (No shebang: Bun would keep it
// above its `// @bun @bun-cjs` line, which is how its loader is recognised.)
require("../src/cli");
