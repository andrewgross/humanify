/**
 * The webcrack subprocess shim (WPB.2): the Rust unpack stage's webcrack
 * adapter runs webpack/browserify bundles through THIS script rather than
 * porting webcrack (docs/rust-port/10-work-breakdown.md, WPB.2).
 *
 *   <code on stdin> | npx tsx scripts/webcrack-shim.ts <output-dir>
 *
 * Unpacks with `webcrack` and prints one JSON line:
 * `{"files":[{"path","metadata"?}],"bundleType"?}`, which
 * `humanify_core::unpack::webcrack` parses. Self-contained since the cutover
 * absorbed the TS pipeline's src/plugins/webcrack.ts into it
 * (docs/rust-port/19-cutover.md): this is the binary's one Node dependency
 * at run time, and only for webpack/browserify inputs.
 */
import fs from "node:fs/promises";
import path from "node:path";
import { webcrack as wc } from "webcrack";

type ModuleMetadata = {
  /** Module ID from the bundler (e.g., "0", "1", "abc123") */
  id: string;
  /** Module path as resolved by webcrack (e.g., "./node_modules/react/index.js") */
  modulePath: string;
  /** Whether this module is the bundle entry point */
  isEntry: boolean;
};

type WebcrackFile = {
  path: string;
  metadata?: ModuleMetadata;
};

type WebcrackOutput = {
  files: WebcrackFile[];
  bundleType?: "webpack" | "browserify";
};

async function webcrack(
  code: string,
  outputDir: string
): Promise<WebcrackOutput> {
  // Clear output directory to avoid processing stale files from previous runs
  await clearDirectory(outputDir);

  const cracked = await wc(code);
  await cracked.save(outputDir);

  const bundle = cracked.bundle;

  // Build a map from output filename to module metadata
  const moduleMetadataMap = new Map<string, ModuleMetadata>();
  if (bundle) {
    for (const [id, mod] of bundle.modules) {
      // webcrack saves modules using their path (or id if no path)
      // Normalize to match the output filename
      const filename = normalizeModulePath(mod.path || id);
      moduleMetadataMap.set(filename, {
        id,
        modulePath: mod.path,
        isEntry: mod.isEntry
      });
    }
  }

  const output = await fs.readdir(outputDir);
  const files: WebcrackFile[] = output
    .filter((file) => file.endsWith(".js"))
    .map((file) => {
      const filePath = path.join(outputDir, file);
      // Try to find metadata for this file
      const metadata = findMetadataForFile(file, moduleMetadataMap);
      return { path: filePath, metadata };
    });

  return {
    files,
    bundleType: bundle?.type
  };
}

/**
 * Normalize a module path to match webcrack's output filename convention.
 * webcrack saves modules as their path with slashes replaced.
 */
function normalizeModulePath(modulePath: string): string {
  // Remove leading ./
  let normalized = modulePath.replace(/^\.\//, "");
  // Ensure .js extension
  if (!normalized.endsWith(".js")) {
    normalized += ".js";
  }
  return normalized;
}

/**
 * Find metadata for an output file by matching against module paths.
 */
function findMetadataForFile(
  filename: string,
  metadataMap: Map<string, ModuleMetadata>
): ModuleMetadata | undefined {
  // Direct match
  if (metadataMap.has(filename)) {
    return metadataMap.get(filename);
  }

  // Try matching just the basename
  for (const [key, metadata] of metadataMap) {
    if (path.basename(key) === filename) {
      return metadata;
    }
  }

  return undefined;
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
  } catch (err: unknown) {
    if ((err as NodeJS.ErrnoException).code === "ENOENT") {
      // Directory doesn't exist, create it
      await fs.mkdir(dir, { recursive: true });
    } else {
      throw err;
    }
  }
}

async function readStdin(): Promise<string> {
  const chunks: Buffer[] = [];
  for await (const chunk of process.stdin) chunks.push(chunk as Buffer);
  return Buffer.concat(chunks).toString("utf-8");
}

async function main(): Promise<void> {
  const outputDir = process.argv[2];
  if (!outputDir) {
    process.stderr.write("usage: webcrack-shim.ts <output-dir> < bundle.js\n");
    process.exit(2);
  }
  const output = await webcrack(await readStdin(), outputDir);
  process.stdout.write(`${JSON.stringify(output)}\n`);
}

void main();
