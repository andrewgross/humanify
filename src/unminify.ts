import fs from "node:fs/promises";
import type { MixedFileDetection } from "./library-detection/index.js";
import { selectLibraryDetector } from "./library-detection/index.js";
import type { FileContext, PipelineConfig } from "./pipeline/types.js";
import type { Profiler } from "./profiling/profiler.js";
import { NULL_PROFILER } from "./profiling/profiler.js";
import { selectUnpackAdapter } from "./unpack/index.js";
import { verbose } from "./verbose.js";

interface UnminifyOptions {
  afterFileWrite?: (filePath: string) => Promise<void>;
  skipLibraries?: boolean;
  /** Custom log output function (defaults to console.log) */
  log?: (message: string) => void;
  /** Profiler instance for performance instrumentation */
  profiler?: Profiler;
  /**
   * Called with the original source code of each file before plugins run.
   * Used by --split to capture pre-transform source for module detection.
   */
  onOriginalSource?: (filePath: string, code: string) => void;
  /**
   * When true, skip writing output files. Used by --split mode where the
   * caller handles output after post-processing the rename result.
   */
  skipFileWrite?: boolean;
}

async function unpackBundle(
  bundledCode: string,
  outputDir: string,
  config: PipelineConfig,
  profiler: Profiler
): Promise<Array<{ path: string }>> {
  const adapter = selectUnpackAdapter(config);
  const unpackSpan = profiler.startSpan("unpack", "pipeline");
  const { files } = await adapter.unpack(bundledCode, outputDir);
  unpackSpan.end({ fileCount: files.length, adapter: adapter.name });
  verbose.log(`Unpacked ${files.length} file(s) via ${adapter.name}`);
  return files;
}

async function filterLibraries(
  files: Array<{ path: string }>,
  config: PipelineConfig,
  profiler: Profiler,
  log: (msg: string) => void
): Promise<{
  filesToProcess: Array<{ path: string }>;
  mixedFiles: Map<string, MixedFileDetection>;
}> {
  const detector = selectLibraryDetector(config);
  const libSpan = profiler.startSpan("library-detection", "pipeline");
  const detection = await detector.detectLibraries(files);
  libSpan.end({
    libraryCount: detection.libraryFiles.size,
    mixedCount: detection.mixedFiles.size,
    detector: detector.name
  });
  const mixedFiles = detection.mixedFiles;

  if (detection.libraryFiles.size > 0) {
    const libCounts = new Map<string, number>();
    for (const det of detection.libraryFiles.values()) {
      const name = det.libraryName ?? "unknown";
      libCounts.set(name, (libCounts.get(name) ?? 0) + 1);
    }
    const libSummary = Array.from(libCounts.entries())
      .map(([name, count]) => `${name} (${count} file${count > 1 ? "s" : ""})`)
      .join(", ");
    log(
      `Skipping ${detection.libraryFiles.size} library file${detection.libraryFiles.size > 1 ? "s" : ""}: ${libSummary}`
    );
  }

  if (mixedFiles.size > 0) {
    for (const [path, mixed] of mixedFiles) {
      const libs = mixed.libraryNames.join(", ");
      log(`Mixed file ${path}: will skip library functions (${libs})`);
    }
  }

  const filesToProcess = files.filter(
    (f) => !detection.libraryFiles.has(f.path)
  );
  return { filesToProcess, mixedFiles };
}

async function processFile(
  file: { path: string },
  plugins: ((code: string, context: FileContext) => Promise<string>)[],
  options: UnminifyOptions,
  mixedFiles: Map<string, MixedFileDetection>,
  profiler: Profiler
): Promise<void> {
  const fileReadSpan = profiler.startSpan("file-io:read", "io");
  const code = await fs.readFile(file.path, "utf-8");
  fileReadSpan.end({ path: file.path, bytes: code.length });

  if (code.trim().length === 0) {
    verbose.log(`Skipping empty file ${file.path}`);
    return;
  }

  options.onOriginalSource?.(file.path, code);

  const mixed = mixedFiles.get(file.path);
  const context: FileContext = {
    commentRegions: mixed?.regions
  };

  const formattedCode = await plugins.reduce(
    (p, next) => p.then((c) => next(c, context)),
    Promise.resolve(code)
  );

  verbose.debug(
    "Input: ",
    code.slice(0, 2000) + (code.length > 2000 ? "\n... truncated" : "")
  );
  verbose.debug(
    "Output: ",
    formattedCode.slice(0, 2000) +
      (formattedCode.length > 2000 ? "\n... truncated" : "")
  );

  if (!options.skipFileWrite) {
    const fileWriteSpan = profiler.startSpan("file-io:write", "io");
    await fs.writeFile(file.path, formattedCode);
    fileWriteSpan.end({ path: file.path, bytes: formattedCode.length });
    await options.afterFileWrite?.(file.path);
  }
}

export async function unminify(
  bundledCode: string,
  outputDir: string,
  config: PipelineConfig,
  plugins: ((code: string, context: FileContext) => Promise<string>)[] = [],
  options?: UnminifyOptions
) {
  const log = options?.log ?? console.log;
  const profiler = options?.profiler ?? NULL_PROFILER;
  const opts: UnminifyOptions = options ?? {};

  const files = await unpackBundle(bundledCode, outputDir, config, profiler);

  let filesToProcess = files;
  let mixedFiles = new Map<string, MixedFileDetection>();
  const skipLibraries = options?.skipLibraries ?? true;

  if (skipLibraries) {
    ({ filesToProcess, mixedFiles } = await filterLibraries(
      files,
      config,
      profiler,
      log
    ));
  }

  for (let i = 0; i < filesToProcess.length; i++) {
    log(`Processing file ${i + 1}/${filesToProcess.length}`);
    await processFile(filesToProcess[i], plugins, opts, mixedFiles, profiler);
  }

  log(`Done! You can find your unminified code in ${outputDir}`);
}
