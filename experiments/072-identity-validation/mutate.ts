/**
 * 072 — mutator: produce version N+1 from a corpus, recording TRUTH.
 *
 *   npx tsx experiments/072-identity-validation/mutate.ts <v1Dir> <v2Dir>
 *
 * Each mutation targets a disjoint set of files so every file carries
 * exactly one truth label. The labels are what the validator scores
 * against — they are the reason this experiment can say "wrong" at all,
 * which no measurement on the real target can.
 *
 * `renamed` and `moved` are truth-UNCHANGED (behaviour identical, only
 * names or the path differ) and MUST read identical: masking identifier
 * names is the whole premise of the fingerprint. `reordered` is called
 * out separately because a module signature is a SORTED set of statement
 * hashes, so it is predicted to read identical — benign for carrying
 * names (same statements, same names) but a real limit on the claim
 * "identical means byte-identical source".
 */
import * as fs from "node:fs";
import * as path from "node:path";

export type Truth =
  | "unchanged"
  | "importer-repointed"
  | "renamed"
  | "moved"
  | "reordered"
  | "literal"
  | "added-statement"
  | "removed-statement"
  | "added-file"
  | "removed-file"
  | "dep-changed";

export interface MutationLog {
  /** v2 path -> truth label */
  labels: Record<string, Truth>;
  /** v2 path -> v1 path, when the file moved */
  movedFrom: Record<string, string>;
  counts: Record<string, number>;
}

function walkFiles(dir: string, base = dir, out: string[] = []): string[] {
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    if (e.name === "node_modules") continue;
    const p = path.join(dir, e.name);
    if (e.isDirectory()) walkFiles(p, base, out);
    else if (e.name.endsWith(".js")) out.push(path.relative(base, p));
  }
  return out;
}

export function mutate(v1: string, v2: string): MutationLog {
  fs.rmSync(v2, { recursive: true, force: true });
  fs.cpSync(v1, v2, { recursive: true });

  const files = walkFiles(path.join(v2, "src"), v2)
    .filter((f) => !f.endsWith("index.js"))
    .sort();
  const log: MutationLog = { labels: {}, movedFrom: {}, counts: {} };
  for (const f of files) log.labels[f] = "unchanged";

  const read = (f: string) => fs.readFileSync(path.join(v2, f), "utf8");
  const write = (f: string, s: string) => fs.writeFileSync(path.join(v2, f), s);
  const bump = (k: Truth) => {
    log.counts[k] = (log.counts[k] ?? 0) + 1;
  };

  // Deterministic disjoint slices: every 11th file gets mutation k.
  const pick = (offset: number, stride = 11) =>
    files.filter((_f, i) => i % stride === offset);

  // 1. RENAME a local — truth: unchanged behaviour.
  for (const f of pick(0)) {
    const s = read(f);
    const m = /const (label\d+) = /.exec(s);
    if (!m) continue;
    write(f, s.split(m[1]).join(`renamedTag${m[1].slice(5)}`));
    log.labels[f] = "renamed";
    bump("renamed");
  }
  // 2. LITERAL change — truth: changed.
  for (const f of pick(1)) {
    const s = read(f);
    const m = /"module-(\d+)-tag"/.exec(s);
    if (!m) continue;
    write(f, s.replace(m[0], `"module-${m[1]}-CHANGED"`));
    log.labels[f] = "literal";
    bump("literal");
  }
  // 3. ADD a statement — truth: changed.
  for (const f of pick(2)) {
    const s = read(f);
    write(f, `${s}export const extraAdded = 17;\n`);
    log.labels[f] = "added-statement";
    bump("added-statement");
  }
  // 4. REMOVE a statement — truth: changed.
  for (const f of pick(3)) {
    const s = read(f);
    const lines = s.split("\n");
    const i = lines.findIndex((l) => l.startsWith("export const table"));
    if (i < 0) continue;
    // the table is referenced in compute(); drop the reference too.
    const kept = lines
      .filter((_l, k) => k !== i)
      .map((l) => l.replace(/ \+ table\d+\.length/, ""));
    write(f, kept.join("\n"));
    log.labels[f] = "removed-statement";
    bump("removed-statement");
  }
  // 5. REORDER two independent top-level statements — the suspected
  //    blind spot: signature is a SORTED set.
  for (const f of pick(4)) {
    const s = read(f);
    const lines = s.split("\n");
    const a = lines.findIndex((l) => l.startsWith("export const value"));
    const b = lines.findIndex((l) => l.startsWith("const label"));
    if (a < 0 || b < 0) continue;
    // `label` does not depend on `value`, so swapping is behaviour-safe.
    const copy = [...lines];
    [copy[a], copy[b]] = [copy[b], copy[a]];
    write(f, copy.join("\n"));
    log.labels[f] = "reordered";
    bump("reordered");
  }
  // 6. MOVE a file, content untouched — truth: unchanged.
  for (const f of pick(5)) {
    const dest = f.replace("src/", "src/relocated/");
    const body = read(f);
    const abs = path.join(v2, dest);
    fs.mkdirSync(path.dirname(abs), { recursive: true });
    // fix relative imports for the new depth
    const fixed = body.replace(/from "(\.[^"]+)"/g, (_m, rel) => {
      const target = path.resolve(path.dirname(path.join(v2, f)), rel);
      const nrel = path.relative(path.dirname(abs), target).replace(/\\/g, "/");
      return `from "${nrel.startsWith(".") ? nrel : `./${nrel}`}"`;
    });
    fs.writeFileSync(abs, fixed);
    fs.rmSync(path.join(v2, f));
    // Every importer of the moved file must be re-pointed, or the build
    // breaks. Re-pointing changes ONLY an import specifier string, which
    // is a real content change for the importer — recorded as such below.
    for (const other of walkFiles(path.join(v2, "src"), v2)) {
      if (other === dest) continue;
      const body2 = read(other);
      const rewritten = body2.replace(/from "(\.[^"]+)"/g, (mm, rel) => {
        const target = path.resolve(path.dirname(path.join(v2, other)), rel);
        // BOTH sides must be resolved: `path.join(v2, f)` is relative when
        // v2 is, and a relative string never equals a resolved one.
        if (target !== path.resolve(path.join(v2, f))) return mm;
        const nrel = path
          .relative(path.dirname(path.join(v2, other)), abs)
          .replace(/\\/g, "/");
        return `from "${nrel.startsWith(".") ? nrel : `./${nrel}`}"`;
      });
      if (rewritten !== body2) {
        write(other, rewritten);
        // Import specifiers vanish in bundling (they become init calls),
        // so the importer's EMITTED content is unchanged — but its source
        // did change. Label it so the score cannot silently credit us.
        if (log.labels[other] === "unchanged")
          log.labels[other] = "importer-repointed";
      }
    }
    delete log.labels[f];
    log.labels[dest] = "moved";
    log.movedFrom[dest] = f;
    bump("moved");
  }
  // 7. ADD a brand-new file — truth: added.
  for (let k = 0; k < 3; k++) {
    const dest = `src/core/brand-new-${k}.js`;
    fs.writeFileSync(
      path.join(v2, dest),
      `export const fresh${k} = Math.round(${k + 3} / 2);\nexport function useFresh${k}(x) { return x + fresh${k}; }\n`
    );
    log.labels[dest] = "added-file";
    bump("added-file");
  }
  // 8. DEPENDENCY change — a package's own source moves; app untouched.
  const msIndex = path.join(v2, "node_modules/ms/index.js");
  if (fs.existsSync(msIndex)) {
    const s = fs.readFileSync(msIndex, "utf8");
    fs.writeFileSync(
      msIndex,
      s.replace(/var s = 1000/, "var s = 1000 /* v2 */")
    );
    bump("dep-changed");
  }

  // The entry must import the moved/added files too, or they vanish.
  const entry = path.join(v2, "src/index.js");
  const all = walkFiles(path.join(v2, "src"), v2).filter(
    (f) => !f.endsWith("index.js")
  );
  const lines = [
    "const mods = await Promise.all([",
    ...all.map(
      (f) => `  import("./${path.relative("src", f).replace(/\\/g, "/")}"),`
    ),
    "]);",
    "let acc = 0;",
    "for (const m of mods) acc += Object.keys(m).length;",
    "console.log(acc);"
  ];
  fs.writeFileSync(entry, `${lines.join("\n")}\n`);
  fs.writeFileSync(
    path.join(v2, "mutations.json"),
    JSON.stringify(log, null, 2)
  );
  return log;
}

if (process.argv[1]?.endsWith("mutate.ts")) {
  const [a, b] = process.argv.slice(2);
  if (!a || !b) {
    console.error("usage: mutate.ts <v1Dir> <v2Dir>");
    process.exit(1);
  }
  const log = mutate(a, b);
  console.log("mutations:", JSON.stringify(log.counts));
}
