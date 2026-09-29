/**
 * 072 — corpus generator: a project whose source we KNOW.
 *
 *   npx tsx experiments/072-identity-validation/generate.ts <outDir> [files]
 *
 * The point of this corpus is to make the bundler emit the SAME SHAPE the
 * real target has: one lazy `__esm` initializer per source file. Two
 * conditions produce that, both verified against bun 1.3.14 (2026-08-15):
 *
 *   1. every module owns TOP-LEVEL COMPUTED state (`const x = f()`), not
 *      just function declarations — a module of pure function decls is
 *      hoisted and gets an EMPTY initializer or none at all;
 *   2. the module graph hangs off a DYNAMIC import, so evaluation must be
 *      deferred — a plain static ESM import tree is inlined flat.
 *
 * Miss either and the corpus validates a shape production never meets.
 *
 * Deterministic: same seed ⇒ byte-identical corpus, so a mutation's
 * effect is the only variable between two builds.
 */
import * as fs from "node:fs";
import * as path from "node:path";

/** Mulberry32 — small deterministic PRNG; no dependency, reproducible. */
function rng(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

const FOLDERS = [
  "core",
  "utils",
  "features/auth",
  "features/report",
  "features/sync",
  "services",
  "adapters",
  "shared/text",
  "shared/math"
];

export interface CorpusFile {
  /** repo-relative path, e.g. `src/utils/mod-12.js` */
  file: string;
  /** module indexes this file imports (corpus-local) */
  imports: number[];
  /** true when this file is a verbatim duplicate of another (twin class) */
  duplicateOf?: string;
}

export interface CorpusManifest {
  seed: number;
  files: CorpusFile[];
  entry: string;
  /** npm packages copied into node_modules */
  deps: string[];
}

const DEPS = ["ms", "semver", "debug"];

function moduleSource(
  idx: number,
  imports: number[],
  files: string[],
  depImport: string | null,
  rand: () => number
): string {
  const lines: string[] = [];
  for (const i of imports) {
    const rel = path
      .relative(path.dirname(files[idx]), files[i])
      .replace(/\\/g, "/");
    lines.push(
      `import { value${i}, compute${i} } from "${rel.startsWith(".") ? rel : `./${rel}`}";`
    );
  }
  if (depImport) lines.push(`import dep${idx} from "${depImport}";`);
  // TOP-LEVEL COMPUTED STATE — this is what forces a real initializer.
  const base = Math.floor(rand() * 900) + 100;
  lines.push(`export const value${idx} = Math.round(${base} / 7) + ${idx};`);
  lines.push(
    `export const table${idx} = [${base}, ${base + 1}, ${base + 2}].map((n) => n * value${idx});`
  );
  lines.push(`const label${idx} = "module-${idx}-tag";`);
  const sum = imports.length
    ? imports.map((i) => `value${i}`).join(" + ")
    : "0";
  lines.push(`export const total${idx} = value${idx} + ${sum};`);
  lines.push(`export function compute${idx}(input) {`);
  lines.push(`  const scaled = input * value${idx} + table${idx}.length;`);
  if (imports.length > 0) {
    lines.push(`  return compute${imports[0]}(scaled) + total${idx};`);
  } else {
    lines.push(`  return scaled + label${idx}.length;`);
  }
  lines.push(`}`);
  if (depImport) {
    lines.push(
      `export function describe${idx}() { return String(dep${idx}) + label${idx}; }`
    );
  }
  return `${lines.join("\n")}\n`;
}

export function generate(
  outDir: string,
  count = 150,
  seed = 42
): CorpusManifest {
  const rand = rng(seed);
  fs.rmSync(outDir, { recursive: true, force: true });
  fs.mkdirSync(path.join(outDir, "src"), { recursive: true });

  const files: string[] = [];
  for (let i = 0; i < count; i++) {
    const folder = FOLDERS[Math.floor(rand() * FOLDERS.length)];
    files.push(`src/${folder}/mod-${i}.js`);
  }

  const manifest: CorpusManifest = {
    seed,
    files: [],
    entry: "src/index.js",
    deps: DEPS
  };

  // Imports point STRICTLY backwards (i -> j<i): a DAG, no cycles, so the
  // bundler's laziness decision is driven by the dynamic entry alone.
  for (let i = 0; i < count; i++) {
    const imports: number[] = [];
    const want = i === 0 ? 0 : Math.min(i, Math.floor(rand() * 3));
    const seen = new Set<number>();
    for (let k = 0; k < want; k++) {
      const j = Math.floor(rand() * i);
      if (!seen.has(j)) {
        seen.add(j);
        imports.push(j);
      }
    }
    const depImport =
      rand() < 0.15 ? DEPS[Math.floor(rand() * DEPS.length)] : null;
    const src = moduleSource(i, imports, files, depImport, rand);
    const dest = path.join(outDir, files[i]);
    fs.mkdirSync(path.dirname(dest), { recursive: true });
    fs.writeFileSync(dest, src);
    manifest.files.push({ file: files[i], imports });
  }

  // Verbatim duplicates — the twin class, with truth attached.
  const dupCount = Math.max(2, Math.floor(count * 0.03));
  for (let d = 0; d < dupCount; d++) {
    const srcIdx = Math.floor(rand() * count);
    const from = files[srcIdx];
    const to = `src/shared/dup/copy-${d}.js`;
    const body = fs.readFileSync(path.join(outDir, from), "utf8");
    // Rewrite relative imports for the new location so it still builds.
    const rewritten = body.replace(/from "(\.[^"]+)"/g, (_m, rel) => {
      const abs = path.resolve(path.dirname(path.join(outDir, from)), rel);
      const nrel = path
        .relative(path.dirname(path.join(outDir, to)), abs)
        .replace(/\\/g, "/");
      return `from "${nrel.startsWith(".") ? nrel : `./${nrel}`}"`;
    });
    const dest = path.join(outDir, to);
    fs.mkdirSync(path.dirname(dest), { recursive: true });
    fs.writeFileSync(dest, rewritten);
    manifest.files.push({
      file: to,
      imports: manifest.files[srcIdx].imports,
      duplicateOf: from
    });
  }

  // Entry: DYNAMIC imports force the whole graph lazy. EVERY module is
  // imported here — a module reachable only statically is inlined, and a
  // module reachable not at all is tree-shaken out of existence (measured:
  // importing every 7th file yielded 47 modules from 154 source files).
  const roots = manifest.files.map((f, i) => ({ f, i }));
  const entryLines = [
    "const mods = await Promise.all([",
    ...roots.map(
      ({ f }) =>
        `  import("./${path.relative("src", f.file).replace(/\\/g, "/")}"),`
    ),
    "]);",
    "let acc = 0;",
    "for (const m of mods) acc += Object.keys(m).length;",
    "console.log(acc);"
  ];
  fs.writeFileSync(
    path.join(outDir, "src/index.js"),
    `${entryLines.join("\n")}\n`
  );
  fs.writeFileSync(
    path.join(outDir, "package.json"),
    `${JSON.stringify({ name: "corpus", version: "1.0.0", type: "module" }, null, 2)}\n`
  );

  // Real dependencies, copied from this repo's node_modules.
  const nm = path.join(outDir, "node_modules");
  fs.mkdirSync(nm, { recursive: true });
  for (const d of DEPS) {
    const from = path.resolve(process.cwd(), "node_modules", d);
    if (fs.existsSync(from))
      fs.cpSync(from, path.join(nm, d), { recursive: true });
  }
  fs.writeFileSync(
    path.join(outDir, "manifest.json"),
    JSON.stringify(manifest, null, 2)
  );
  return manifest;
}

if (process.argv[1]?.endsWith("generate.ts")) {
  const [out, n] = process.argv.slice(2);
  if (!out) {
    console.error("usage: generate.ts <outDir> [files]");
    process.exit(1);
  }
  const m = generate(out, n ? Number(n) : 150);
  console.log(`generated ${m.files.length} files in ${out}`);
}
