// WPB.4 probe hook: registers wpb4-export-loader.mjs (see there).
//   npx tsx --import ./test/parity/wpb4-export-hook.mjs test/parity/wpb4-probe.ts
import { register } from "node:module";

register(new URL("./wpb4-export-loader.mjs", import.meta.url));
