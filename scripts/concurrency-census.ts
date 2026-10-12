/**
 * Concurrency census. `npm run census:concurrency` — a GATED check (stage
 * `census:concurrency` in scripts/check.ts), and the pre-commit hook on
 * staged `.rs` files (`--files`, via lint-staged).
 *
 * The pipeline's concurrency approach is written down in docs/concurrency.md:
 * a FIXED set of patterns, each with its rules. This census finds every
 * concurrency construct in the pipeline's non-test Rust (`crates/**\/*.rs`)
 * and requires each one to be on scripts/concurrency-allowlist.json with the
 * pattern it follows and a reason. An unlisted site FAILS — that is the
 * "did you really mean this?" flag for whoever (usually an agent) added it:
 * the message names the construct, the doc section with its rules, and the
 * exact entry to add once the site is confirmed. A stale entry (its site is
 * gone) also fails, so the allowlist stays an inventory, not a graveyard.
 *
 * ## What counts as a site
 *
 * A site is (file, scope, construct). `scope` is the enclosing function as
 * `Type::fn` (nested functions joined by `::`), or the enclosing item outside
 * a function (`struct X`, `static X`, `type X`, `impl X`), or `<module>`.
 * Several uses of one construct in one scope are ONE site, and line numbers
 * are not part of the key: ordinary edits do not churn the allowlist, while
 * moving a construct to another function does — that is a new site to judge.
 *
 * Constructs are matched by regex over the source with comments and string
 * literals blanked out (so a doc comment that mentions `block_on` is not a
 * site). `use` declarations are not sites. The construct list is CONSTRUCTS
 * below; each names the doc section whose rules apply.
 *
 * ## What is not scanned
 *
 * Test code: `*_test.rs`, anything under a `tests/` directory, a module file
 * declared `#[cfg(test)] mod x;`, and an inline `#[cfg(test)]` item. Tests
 * build their own runtimes and sleep on purpose; the rules are for the code
 * that produces the output. (The clippy lints in Cargo.toml DO cover tests,
 * and the allow-comment check below scans test files too.)
 *
 * ## The allow-comment check
 *
 * clippy's `disallowed_methods` bans the concurrency methods listed in
 * clippy.toml. Every `#[allow(clippy::disallowed_methods)]` /
 * `disallowed_types` in `crates/` (tests included) must carry a
 * `// concurrency: <reason>` comment on its line or in the three lines
 * above — or `// env: <reason>` for the env-reader ban that shares the lint.
 */
import * as fs from "node:fs";
import * as path from "node:path";

const REPO = path.resolve(import.meta.dirname, "..");
const DOC = "docs/concurrency.md";
const ALLOWLIST_REL = "scripts/concurrency-allowlist.json";

/** The documented patterns (docs/concurrency.md, "The patterns"). An entry
 * must name one of these; the value is its doc section. */
const PATTERNS: Readonly<Record<string, string>> = {
  "par-seam": "cpu-parallelism",
  "llm-runtime": "async-io",
  "rate-limiter": "async-io",
  "single-flight": "shared-state",
  "retry-envelope": "failures-and-retries",
  deadline: "deadlines",
  subprocess: "subprocesses",
  "short-lock": "shared-state",
  "stats-counter": "shared-state",
  observer: "observers"
};

interface Construct {
  id: string;
  re: RegExp;
  /** docs/concurrency.md anchor whose rules apply. */
  section: string;
  /** One line: the rule an author must have checked. */
  rule: string;
}

const CONSTRUCTS: readonly Construct[] = [
  {
    id: "thread-spawn",
    re: /\bthread::(spawn|Builder)\b/g,
    section: "cpu-parallelism",
    rule: "a detached OS thread: only observers may run beside the pipeline, and they never touch output; CPU work goes through crate::par"
  },
  {
    id: "thread-scope",
    re: /\bthread::scope\b/g,
    section: "cpu-parallelism",
    rule: "a scoped thread: only inside crate::par (beside) or to drain a subprocess's pipes"
  },
  {
    id: "tokio-spawn",
    re: /\btokio::(task::)?spawn\b|\bspawn_local\b|\bJoinSet\b/g,
    section: "async-io",
    rule: "no detached tasks: every future is driven to completion by the provider's run_wave/run_pipelined"
  },
  {
    id: "spawn-blocking",
    re: /\b(spawn_blocking|block_in_place)\b/g,
    section: "async-io",
    rule: "no blocking pool: blocking or CPU work stays off the runtime thread entirely"
  },
  {
    id: "block-on",
    re: /\bblock_on\s*\(/g,
    section: "async-io",
    rule: "block_on only on the provider's one runtime, never nested, never from rayon"
  },
  {
    id: "runtime-builder",
    re: /\b(Runtime::new|new_current_thread|new_multi_thread)\b|#\[tokio::main\b/g,
    section: "async-io",
    rule: "ONE runtime per process, owned by the LLM provider; no second runtime"
  },
  {
    id: "rayon-par",
    re: /\b(par_iter|par_iter_mut|into_par_iter|par_bridge|par_chunks\w*|par_sort\w*|par_extend|par_drain|par_windows)\b/g,
    section: "cpu-parallelism",
    rule: "raw rayon only inside crate::par; callers use par::map_ordered / produce_then_map / join (results in input order)"
  },
  {
    id: "rayon-pool",
    re: /\b(ThreadPoolBuilder|ThreadPool|build_global|current_num_threads)\b/g,
    section: "cpu-parallelism",
    rule: "the rayon pool size is an open decision (audit R4); a pool is never built ad hoc"
  },
  {
    id: "rayon-join",
    re: /\brayon::(join|scope|spawn)\b/g,
    section: "cpu-parallelism",
    rule: "raw rayon only inside crate::par"
  },
  {
    id: "async-fanout",
    re: /\b(join_all|try_join_all|FuturesUnordered|FuturesOrdered)\b|\b(try_)?join!/g,
    section: "output-determinism",
    rule: "a fan-out is bounded by the prompt window, and completion order never reaches the tree or stats"
  },
  {
    id: "select",
    re: /\bselect!/g,
    section: "async-io",
    rule: "select! cancels the losing branch: every branch must be cancel-safe"
  },
  {
    id: "lock",
    re: /(?<!tokio::sync::)\b(Mutex|RwLock)\b/g,
    section: "shared-state",
    rule: "a std lock: short critical section, never held across .await, IO or a call that can take another lock"
  },
  {
    id: "async-lock",
    re: /\btokio::sync::(Mutex|RwLock)\b/g,
    section: "shared-state",
    rule: "an async lock: only where the guard must be held across .await (the single-flight gate)"
  },
  {
    id: "condvar",
    re: /\b(Condvar|Barrier)\b/g,
    section: "shared-state",
    rule: "a wait on another thread: needs a deadline or a proof it is always signalled"
  },
  {
    id: "semaphore",
    re: /\bSemaphore\b/g,
    section: "async-io",
    rule: "the rate limiter's concurrency cap; one cap, owned by rate.rs"
  },
  {
    id: "channel",
    re: /\b(mpsc|crossbeam\w*|oneshot|sync_channel)\b|\bbroadcast::|\bwatch::channel\b/g,
    section: "shared-state",
    rule: "a channel: bounded, and a full or closed channel is a decided failure, never a silent drop"
  },
  {
    id: "atomic",
    re: /\bAtomic(Bool|Usize|Isize|U8|U16|U32|U64|I8|I16|I32|I64|Ptr)\b/g,
    section: "shared-state",
    rule: "an atomic: a counter or flag only; nothing that decides output may race on it"
  },
  {
    id: "atomic-relaxed",
    // `Ordering::Relaxed`, or a bare `Relaxed` imported from it — not an
    // enum variant of the same name (`FastTier::Relaxed(..)`).
    re: /\bOrdering::Relaxed\b|(?<![:\w])Relaxed\b(?!\s*[({])/g,
    section: "shared-state",
    rule: "Ordering::Relaxed orders NOTHING: allowed only for stats counters and unique-name sequences (pattern stats-counter)"
  },
  {
    id: "subprocess",
    re: /\bCommand::new\b/g,
    section: "subprocesses",
    rule: "a subprocess: drain every pipe concurrently, a deadline on the whole wait, and a failure is loud"
  },
  {
    id: "piped-stdio",
    re: /\bStdio::piped\b/g,
    section: "subprocesses",
    rule: "a piped child: stdin written concurrently with draining stdout/stderr (audit W1)"
  },
  {
    id: "http-client",
    re: /\breqwest::(Client|ClientBuilder)\b|\bClient::(new|builder)\b/g,
    section: "deadlines",
    rule: "an HTTP client: every request runs under ONE deadline covering connect, send and the body (audit W2)"
  },
  {
    id: "sleep",
    re: /\bsleep(_until)?\s*\(/g,
    section: "async-io",
    rule: "a sleep: never std::thread::sleep on the runtime thread; in async only tokio::time::sleep, for backoff or a rate window"
  },
  {
    id: "timeout",
    re: /\b(timeout|timeout_at|recv_timeout)\s*\(/g,
    section: "deadlines",
    rule: "a deadline must cover the WHOLE operation it guards (audit W2: send AND body)"
  }
];

/** Constructs whose entries must use one specific pattern. */
const REQUIRED_PATTERN: Readonly<Record<string, string>> = {
  "atomic-relaxed": "stats-counter"
};

interface Entry {
  file: string;
  scope: string;
  construct: string;
  pattern: string;
  reason: string;
  /** A known deviation from the doc's rules, naming the open finding. */
  exception?: string;
}

interface Site {
  file: string;
  scope: string;
  construct: string;
  lines: number[];
}

// ─── source sanitising ────────────────────────────────────────────────────

/** Blank out a span (keep newlines so offsets and line numbers hold). */
function blank(chars: string[], from: number, to: number): void {
  for (let i = from; i < to; i++) if (chars[i] !== "\n") chars[i] = " ";
}

/** End index (exclusive) of a raw string starting at `i` (`r#*"`), or -1. */
function rawStringEnd(src: string, i: number): number {
  const m = /^b?r(#*)"/.exec(src.slice(i, i + 260));
  if (!m) return -1;
  const close = `"${m[1]}`;
  const end = src.indexOf(close, i + m[0].length);
  return end < 0 ? src.length : end + close.length;
}

/** End index (exclusive) of a quoted string starting at `i` (the `"`). */
function quotedEnd(src: string, i: number): number {
  let j = i + 1;
  while (j < src.length && src[j] !== '"') j += src[j] === "\\" ? 2 : 1;
  return j + 1;
}

/** End of a char literal at `i` (the `'`), or -1 when it is a lifetime. */
function charEnd(src: string, i: number): number {
  const m = /^'(\\(x[0-9a-fA-F]{2}|u\{[0-9a-fA-F]+\}|.)|[^\\'])'/u.exec(
    src.slice(i, i + 12)
  );
  return m ? i + m[0].length : -1;
}

/** End of a (nestable) block comment starting at `i`. */
function blockCommentEnd(src: string, i: number): number {
  let depth = 0;
  let j = i;
  while (j < src.length) {
    if (src.startsWith("/*", j)) {
      depth++;
      j += 2;
    } else if (src.startsWith("*/", j)) {
      depth--;
      j += 2;
      if (depth === 0) return j;
    } else j++;
  }
  return j;
}

/** Where the comment at `i` ends, or -1 when `i` starts none. */
function commentEnd(src: string, i: number): number {
  if (src[i] !== "/") return -1;
  if (src[i + 1] === "/") {
    const nl = src.indexOf("\n", i);
    return nl < 0 ? src.length : nl;
  }
  return src[i + 1] === "*" ? blockCommentEnd(src, i) : -1;
}

/** Where the string or char literal at `i` ends, or -1 when `i` starts
 * none (a lifetime is not a literal). */
function literalEnd(src: string, i: number): number {
  const c = src[i];
  const prevIsIdent = i > 0 && /\w/.test(src[i - 1]);
  if ((c === "r" || c === "b") && !prevIsIdent) {
    const raw = rawStringEnd(src, i);
    if (raw >= 0) return raw;
    if (c === "b" && src[i + 1] === '"') return quotedEnd(src, i + 1);
  }
  if (c === '"') return quotedEnd(src, i);
  return c === "'" ? charEnd(src, i) : -1;
}

/** Where the literal or comment at `i` ends, or -1 when `i` starts none. */
function skippableEnd(src: string, i: number): number {
  const comment = commentEnd(src, i);
  return comment >= 0 ? comment : literalEnd(src, i);
}

/** Source with comments and string/char literals blanked, and `use`
 * declarations blanked (an import is not a site). Non-ASCII code units
 * become `_`, so offsets and line numbers match the raw text. */
function sanitize(src: string): string {
  const ascii = src.replace(/[^\x00-\x7f]/g, "_");
  const out = ascii.split("");
  let i = 0;
  while (i < ascii.length) {
    const end = skippableEnd(ascii, i);
    if (end < 0) {
      i++;
      continue;
    }
    blank(out, i, end);
    i = end;
  }
  const text = out.join("");
  for (const m of text.matchAll(USE_DECL)) {
    const from = (m.index ?? 0) + m[1].length;
    blank(out, from, (m.index ?? 0) + m[0].length);
  }
  return out.join("");
}

const USE_DECL = /(^|[;{}]\s*)(?:pub(?:\([^)]*\))?\s+)?use\s[^;]*;/gm;

/** Index of the brace matching the `{` at `open` (sanitized text). */
function matchingBrace(text: string, open: number): number {
  let depth = 0;
  for (let i = open; i < text.length; i++) {
    if (text[i] === "{") depth++;
    else if (text[i] === "}" && --depth === 0) return i;
  }
  return text.length - 1;
}

/** Skip whitespace and `#[...]` attributes from `i`; return the item start. */
function skipAttributes(text: string, i: number): number {
  let j = i;
  for (;;) {
    while (j < text.length && /\s/.test(text[j])) j++;
    if (!text.startsWith("#[", j)) return j;
    let depth = 0;
    for (; j < text.length; j++) {
      if (text[j] === "[") depth++;
      else if (text[j] === "]" && --depth === 0) break;
    }
    j++;
  }
}

/** Blank every inline `#[cfg(test)]` item; return the out-of-line test
 * module names (`mod x;`) with any `#[path]` they carry. */
function stripCfgTest(
  text: string,
  raw: string
): {
  text: string;
  modules: Array<{ name: string; pathAttr?: string }>;
} {
  const out = text.split("");
  const modules: Array<{ name: string; pathAttr?: string }> = [];
  const re = /#\[cfg\(test\)\]/g;
  for (let m = re.exec(text); m; m = re.exec(text)) {
    const start = skipAttributes(text, m.index + m[0].length);
    const semi = text.indexOf(";", start);
    const brace = text.indexOf("{", start);
    if (semi >= 0 && (brace < 0 || semi < brace)) {
      const decl = /^(pub(\([^)]*\))?\s+)?mod\s+(\w+)\s*;/.exec(
        text.slice(start, semi + 1)
      );
      if (decl) {
        const attrs = raw.slice(m.index, start);
        const p = /#\[path\s*=\s*"([^"]+)"\]/.exec(attrs);
        modules.push({ name: decl[3], pathAttr: p?.[1] });
      }
      blank(out, m.index, semi + 1);
    } else if (brace >= 0) {
      blank(out, m.index, matchingBrace(text, brace) + 1);
    }
  }
  return { text: out.join(""), modules };
}

// ─── scopes ───────────────────────────────────────────────────────────────

interface Frame {
  kind: "fn" | "impl" | "item" | "block";
  name: string;
}

/** What a `{` opens, from the text since the previous `;`, `{` or `}`. */
function classifyHeader(header: string): Frame {
  const h = header.replace(/\s+/g, " ");
  const fn = /\bfn (\w+)/.exec(h);
  if (fn) return { kind: "fn", name: fn[1] };
  const impl = /^\s*(unsafe )?impl\b(?:\s*<[^{]*?>)?\s+(.*)$/.exec(h);
  if (impl) {
    const target = impl[2].includes(" for ")
      ? impl[2].split(" for ").pop()
      : impl[2];
    const name = /(\w+)\s*(<.*)?\s*(where\b.*)?$/.exec(
      (target ?? "").replace(/<[^<>]*(<[^<>]*>[^<>]*)*>/g, "").trim()
    );
    return { kind: "impl", name: name ? name[1] : "?" };
  }
  const item = /\b(struct|enum|trait|union|mod) (\w+)/.exec(h);
  if (item && !/[=(]/.test(h.slice(0, h.indexOf(item[0]))))
    return { kind: "item", name: `${item[1]} ${item[2]}` };
  return { kind: "block", name: "" };
}

/** The scope name of a construct, given the open frames and the text of
 * its statement so far. */
function scopeOf(stack: Frame[], statement: string): string {
  const fns = stack.filter((f) => f.kind === "fn").map((f) => f.name);
  // A construct in a signature belongs to the function it declares.
  const signature = /\bfn (\w+)/.exec(statement);
  if (signature) fns.push(signature[1]);
  if (fns.length > 0) {
    const firstFn = stack.findIndex((f) => f.kind === "fn");
    const owner = stack
      .slice(0, firstFn < 0 ? stack.length : firstFn)
      .reverse()
      .find((f) => f.kind === "impl" || f.kind === "item");
    return (owner ? `${owner.name}::` : "") + fns.join("::");
  }
  const top = [...stack].reverse().find((f) => f.kind !== "block");
  if (top) return top.kind === "impl" ? `impl ${top.name}` : top.name;
  const item = /\b(static|const|type|struct|enum|fn) (\w+)/.exec(statement);
  return item ? `${item[1]} ${item[2]}` : "<module>";
}

/** Every construct match in a file, in text order. */
function constructHits(text: string): Array<{ at: number; construct: string }> {
  const hits = CONSTRUCTS.flatMap((c) =>
    [...text.matchAll(c.re)].map((m) => ({ at: m.index ?? 0, construct: c.id }))
  );
  return hits.sort((a, b) => a.at - b.at);
}

/** Every construct occurrence in one sanitized file, keyed into sites. */
function sitesOf(file: string, text: string): Site[] {
  const hits = constructHits(text);
  const sites = new Map<string, Site>();
  const stack: Frame[] = [];
  let boundary = 0;
  let line = 1;
  let h = 0;
  for (let i = 0; i <= text.length && h < hits.length; i++) {
    for (; h < hits.length && hits[h].at === i; h++) {
      const scope = scopeOf(stack, text.slice(boundary, i));
      addSite(sites, file, scope, hits[h].construct, line);
    }
    const ch = text[i];
    if (ch === "\n") line++;
    else if (ch === "{") stack.push(classifyHeader(text.slice(boundary, i)));
    else if (ch === "}") stack.pop();
    if (ch === "{" || ch === "}" || ch === ";") boundary = i + 1;
  }
  return [...sites.values()];
}

function addSite(
  sites: Map<string, Site>,
  file: string,
  scope: string,
  construct: string,
  line: number
): void {
  const key = siteKey({ file, scope, construct });
  const site = sites.get(key) ?? { file, scope, construct, lines: [] };
  if (!site.lines.includes(line)) site.lines.push(line);
  sites.set(key, site);
}

function siteKey(s: {
  file: string;
  scope: string;
  construct: string;
}): string {
  return `${s.file}|${s.scope}|${s.construct}`;
}

// ─── files ────────────────────────────────────────────────────────────────

function walkRs(dir: string, out: string[]): void {
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) {
      if (e.name !== "target" && e.name !== "node_modules") walkRs(p, out);
    } else if (e.name.endsWith(".rs")) out.push(p);
  }
}

function isTestPath(rel: string): boolean {
  return rel.endsWith("_test.rs") || rel.split("/").includes("tests");
}

/** Candidate files for an out-of-line `mod name;` declared in `file`. */
function moduleFiles(
  file: string,
  mod: { name: string; pathAttr?: string }
): string[] {
  const dir = path.dirname(file);
  if (mod.pathAttr) return [path.join(dir, mod.pathAttr)];
  const stem = path.basename(file, ".rs");
  const own = ["lib", "main", "mod"].includes(stem)
    ? dir
    : path.join(dir, stem);
  return [
    path.join(own, `${mod.name}.rs`),
    path.join(own, mod.name, "mod.rs"),
    path.join(dir, `${mod.name}.rs`)
  ];
}

interface Scanned {
  rel: string;
  raw: string;
  /** Sanitized, cfg(test) items blanked. */
  text: string;
  testModules: string[];
}

function scanFile(root: string, abs: string): Scanned {
  const raw = fs.readFileSync(abs, "utf8");
  const stripped = stripCfgTest(sanitize(raw), raw);
  return {
    rel: path.relative(root, abs).split(path.sep).join("/"),
    raw,
    text: stripped.text,
    testModules: stripped.modules.flatMap((m) =>
      moduleFiles(abs, m).map((f) =>
        path.relative(root, f).split(path.sep).join("/")
      )
    )
  };
}

// ─── checks ───────────────────────────────────────────────────────────────

const ALLOW_RE = /#\[allow\([^\]]*clippy::disallowed_(methods|types)/;
const REASON_RE = /\/\/\s*(concurrency|env):\s*\S/;

/** `#[allow(clippy::disallowed_*)]` lines with no reason comment near. */
function unexplainedAllows(f: Scanned): string[] {
  const lines = f.raw.split("\n");
  const bad: string[] = [];
  lines.forEach((l, i) => {
    if (!ALLOW_RE.test(l)) return;
    const near = lines.slice(Math.max(0, i - 3), i + 1);
    if (!near.some((n) => REASON_RE.test(n))) {
      bad.push(
        `  ${f.rel}:${i + 1}  ${l.trim()}\n` +
          "    no `// concurrency: <reason>` comment on this line or the three above\n" +
          "    (`// env: <reason>` for the env-reader ban that shares the lint).\n" +
          `    Every allow of a banned concurrency method says why this site is the exception (${DOC}#what-clippy-enforces).`
      );
    }
  });
  return bad;
}

function entryProblems(e: Partial<Entry>, i: number): string[] {
  const where = `entry ${i} (${e.file ?? "?"} ${e.scope ?? "?"} ${e.construct ?? "?"})`;
  const problems: string[] = [];
  for (const k of [
    "file",
    "scope",
    "construct",
    "pattern",
    "reason"
  ] as const) {
    if (typeof e[k] !== "string" || (e[k] as string).trim() === "")
      problems.push(`  ${where}: missing "${k}"`);
  }
  if (e.pattern && !(e.pattern in PATTERNS))
    problems.push(
      `  ${where}: unknown pattern "${e.pattern}" — one of: ${Object.keys(PATTERNS).join(", ")} (${DOC}#the-patterns)`
    );
  if (e.construct && !CONSTRUCTS.some((c) => c.id === e.construct))
    problems.push(`  ${where}: unknown construct "${e.construct}"`);
  const required = e.construct ? REQUIRED_PATTERN[e.construct] : undefined;
  if (required && e.pattern !== required)
    problems.push(
      `  ${where}: ${e.construct} must use pattern "${required}" — ${CONSTRUCTS.find((c) => c.id === e.construct)?.rule}`
    );
  return problems;
}

function unlistedMessage(s: Site): string {
  const c = CONSTRUCTS.find((x) => x.id === s.construct);
  const entry = {
    file: s.file,
    scope: s.scope,
    construct: s.construct,
    pattern: `<one of: ${Object.keys(PATTERNS).join(", ")}>`,
    reason: "<why this site needs it, and how it meets the rule>"
  };
  return [
    `  ${s.file}:${s.lines.join(",")}  ${s.scope}  —  ${s.construct}`,
    `    Rule (${DOC}#${c?.section}): ${c?.rule}`,
    `    If this is intended, confirm it: add to ${ALLOWLIST_REL}:`,
    JSON.stringify(entry, null, 2)
      .split("\n")
      .map((l) => `      ${l}`)
      .join("\n"),
    `    If not, use the sanctioned primitive instead (${DOC}).`
  ].join("\n");
}

interface Options {
  root: string;
  allowlist: string;
  /** Repo-relative files to report on (pre-commit); null = everything. */
  files: Set<string> | null;
}

function parseArgs(argv: string[]): Options {
  const flag = (name: string): string | undefined => {
    const i = argv.indexOf(name);
    return i >= 0 ? argv[i + 1] : undefined;
  };
  const root = path.resolve(flag("--root") ?? REPO);
  const allowlist = path.resolve(
    flag("--allowlist") ?? path.join(REPO, ALLOWLIST_REL)
  );
  const values = new Set([flag("--root"), flag("--allowlist")]);
  const positional = argv.filter((a) => !a.startsWith("--") && !values.has(a));
  // Only `.rs` paths narrow the report (lint-staged passes staged files);
  // anything else — the allowlist itself staged — means a full run.
  const named = positional
    .filter((p) => p.endsWith(".rs"))
    .map((p) => path.relative(root, path.resolve(p)).split(path.sep).join("/"));
  const files = named.length > 0 ? new Set(named) : null;
  return { root, allowlist, files };
}

function loadEntries(file: string): Array<Partial<Entry>> {
  const parsed = JSON.parse(fs.readFileSync(file, "utf8")) as {
    entries?: Array<Partial<Entry>>;
  };
  return parsed.entries ?? [];
}

function main(): void {
  const started = Date.now();
  const opts = parseArgs(process.argv.slice(2));
  const all: string[] = [];
  const crates = path.join(opts.root, "crates");
  if (fs.existsSync(crates)) walkRs(crates, all);
  const scanned = all.map((abs) => scanFile(opts.root, abs));
  const testModules = new Set(scanned.flatMap((s) => s.testModules));
  const inScope = (rel: string): boolean => !opts.files || opts.files.has(rel);

  const allowProblems = scanned
    .filter((s) => inScope(s.rel))
    .flatMap(unexplainedAllows);
  const live = scanned.filter(
    (s) => !isTestPath(s.rel) && !testModules.has(s.rel)
  );
  const sites = live.flatMap((s) => sitesOf(s.rel, s.text));

  const entries = loadEntries(opts.allowlist);
  const entryErrors = entries.flatMap(entryProblems);
  const seen = new Set<string>();
  for (const e of entries) {
    const k = siteKey(e as Entry);
    if (seen.has(k)) entryErrors.push(`  duplicate entry: ${k}`);
    seen.add(k);
  }
  const siteKeys = new Set(sites.map(siteKey));
  const unlisted = sites.filter(
    (s) => inScope(s.file) && !seen.has(siteKey(s))
  );
  const stale = entries.filter(
    (e) => inScope(e.file ?? "") && !siteKeys.has(siteKey(e as Entry))
  );
  const exceptions = entries.filter((e) => e.exception);

  const report = (title: string, lines: string[]): void => {
    if (lines.length > 0) console.log(`\n${title}\n${lines.join("\n\n")}`);
  };
  report(
    `CONCURRENCY CENSUS: ${unlisted.length} unlisted concurrency site(s) — confirm each is intended (${DOC})`,
    unlisted.map(unlistedMessage)
  );
  report(
    `CONCURRENCY CENSUS: ${stale.length} stale allowlist entr${stale.length === 1 ? "y" : "ies"} — the site is gone (or moved to another function); delete or re-key it in ${ALLOWLIST_REL}`,
    stale.map((e) => `  ${e.file}  ${e.scope}  —  ${e.construct}`)
  );
  report(
    `CONCURRENCY CENSUS: ${entryErrors.length} malformed allowlist entr${entryErrors.length === 1 ? "y" : "ies"}`,
    entryErrors
  );
  report(
    `CONCURRENCY CENSUS: ${allowProblems.length} unexplained clippy allow(s)`,
    allowProblems
  );

  const failures =
    unlisted.length + stale.length + entryErrors.length + allowProblems.length;
  const scope = opts.files
    ? `${opts.files.size} named file(s)`
    : `${all.length} files`;
  const summary = `${sites.length} sites in ${live.length} non-test files, ${entries.length} allowlist entries (${exceptions.length} known exceptions), ${scope} checked, ${Date.now() - started} ms`;
  if (failures > 0) {
    console.log(`\nCONCURRENCY CENSUS RED — ${summary}`);
    process.exit(1);
  }
  console.log(`CONCURRENCY CENSUS GREEN — ${summary}`);
}

main();
