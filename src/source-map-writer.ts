import fs from "fs/promises";
import { basename } from "path";

/**
 * Creates a source map writer that captures a source map from the rename plugin
 * and writes it to disk alongside the output JS file.
 */
export function createSourceMapWriter() {
  let pending: object | null = null;

  return {
    capture(map: object | null) {
      pending = map;
    },

    async write(filePath: string) {
      if (!pending) return;

      const map = { ...(pending as Record<string, unknown>), file: basename(filePath) };
      const mapPath = filePath + ".map";

      await fs.writeFile(mapPath, JSON.stringify(map));

      // Append sourceMappingURL to the JS file
      const code = await fs.readFile(filePath, "utf-8");
      await fs.writeFile(
        filePath,
        code + `\n//# sourceMappingURL=${basename(mapPath)}\n`
      );

      pending = null;
    }
  };
}
