// WP4.2 capture: the module-load half. Registered by wp42-capture-hook.mjs.
// For three TS modules it rewrites the (tsx-transformed) source so each
// named export below is wrapped: the original body runs unchanged, then the
// wrapper hands (args, result) to `globalThis.__wp42rec` synchronously — the
// hook records what it needs and returns. Pure observation: the result is
// returned untouched and no argument is mutated.
const WRAP = {
  "/src/rename/code-window.ts": ["selectFunctionCode", "capContextCode"],
  "/src/rename/context-builder.ts": ["buildContext"],
  "/src/llm/prompts.ts": [
    "buildModuleLevelRenamePrompt",
    "buildModuleLevelRetryPrefix",
    "buildModuleLevelRenameBody"
  ]
};

export async function load(url, context, nextLoad) {
  const result = await nextLoad(url, context);
  const suffix = Object.keys(WRAP).find((s) => url.endsWith(s));
  if (!suffix || result.source == null) return result;
  let source =
    typeof result.source === "string"
      ? result.source
      : Buffer.from(result.source).toString("utf8");
  for (const name of WRAP[suffix]) {
    // tsx emits `function NAME(...)` declarations and ONE trailing
    // `export{...}` list, so renaming the declaration and re-declaring NAME
    // as the wrapper routes the export AND every in-module call through it.
    const decl = `function ${name}(`;
    if (source.split(decl).length !== 2) {
      throw new Error(`wp42 loader: expected one "${decl}" in ${url}`);
    }
    source = source.replace(decl, `function __wp42_orig_${name}(`);
    source += `\nfunction ${name}(...args) {
  const out = __wp42_orig_${name}(...args);
  globalThis.__wp42rec?.(${JSON.stringify(name)}, args, out);
  return out;
}\n`;
  }
  return { ...result, source };
}
