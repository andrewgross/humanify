// Unpack-only probe: does this input trigger Bun factory extraction?
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { BunUnpackAdapter } from "../../src/unpack/adapters/bun.js";

const input = process.argv[2];
const code = fs.readFileSync(input, "utf8");
const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "probe-unpack-"));
try {
  const res = await new BunUnpackAdapter().unpack(code, tmp);
  const mp = path.join(tmp, "vendor", "_bun-modules.json");
  if (fs.existsSync(mp)) {
    const m = JSON.parse(fs.readFileSync(mp, "utf8"));
    const bySrc: Record<string, number> = {};
    for (const f of m.factories)
      bySrc[f.nameSource] = (bySrc[f.nameSource] ?? 0) + 1;
    console.log(
      `EXTRACTION FIRED: ${m.factories.length} factories, ${res.files.length} files, nameSource=${JSON.stringify(bySrc)}`
    );
    console.log(
      `  sample: ${m.factories
        .slice(0, 4)
        .map((f: { fileName: string }) => f.fileName)
        .join(", ")}`
    );
  } else {
    console.log(
      `PASSTHROUGH (no extraction): ${res.files.length} file(s): ${res.files.map((f) => path.basename(f.path)).join(", ")}`
    );
  }
} catch (e) {
  console.log(
    `unpack THREW: ${(e as Error).name}: ${(e as Error).message.slice(0, 160)}`
  );
} finally {
  fs.rmSync(tmp, { recursive: true, force: true });
}
