import { webcrack as wc } from "webcrack";
import fs from "fs/promises";
import path from "path";

type File = {
  path: string;
};

export async function webcrack(
  code: string,
  outputDir: string
): Promise<File[]> {
  // Clear output directory to avoid processing stale files from previous runs
  await clearDirectory(outputDir);

  const cracked = await wc(code);
  await cracked.save(outputDir);

  const output = await fs.readdir(outputDir);
  return output
    .filter((file) => file.endsWith(".js"))
    .map((file) => ({ path: path.join(outputDir, file) }));
}

/**
 * Removes all files and subdirectories in a directory, creating it if needed.
 */
async function clearDirectory(dir: string): Promise<void> {
  try {
    const entries = await fs.readdir(dir, { withFileTypes: true });
    await Promise.all(
      entries.map(async (entry) => {
        const fullPath = path.join(dir, entry.name);
        if (entry.isDirectory()) {
          await fs.rm(fullPath, { recursive: true });
        } else {
          await fs.unlink(fullPath);
        }
      })
    );
  } catch (err: any) {
    if (err.code === "ENOENT") {
      // Directory doesn't exist, create it
      await fs.mkdir(dir, { recursive: true });
    } else {
      throw err;
    }
  }
}
