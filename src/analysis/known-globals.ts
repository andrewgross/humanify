/**
 * Globals a bundled file may legitimately reference.
 *
 * Used to tell a real runtime name apart from a leftover the renamer never
 * reached. Structured as a BASE plus per-environment sets, because the right
 * answer depends on what is being deobfuscated — Andrew, 2026-08-19: "maybe in
 * the future we need to populate it as a base + plugin system based on what we
 * are unminifying." The plug point is `knownGlobals(...envs)`; today every
 * caller asks for all of them, which is correct for a bundle that may target
 * several runtimes at once.
 *
 * ## This list can be wrong, and that bounds what may depend on it
 *
 * A first version omitted `Bun`, `btoa`, `crypto` and `Blob`, and so reported
 * 16 unreachable free references where there were 2. That is fine for a report
 * and disqualifying for a rewrite: renaming a free reference changes a global
 * lookup, and `typeof Bun !== "undefined"` is a feature check whose answer
 * flips if the name is rewritten. So this drives REPORTING only. If something
 * ever wants to rename on the strength of it, that caller needs a different
 * and much stronger justification than "not in our list".
 */

/** ECMAScript itself — true in every host. */
const ECMASCRIPT = [
  "Object",
  "Array",
  "String",
  "Number",
  "Boolean",
  "Symbol",
  "BigInt",
  "Math",
  "JSON",
  "Date",
  "RegExp",
  "Error",
  "TypeError",
  "RangeError",
  "SyntaxError",
  "ReferenceError",
  "EvalError",
  "URIError",
  "AggregateError",
  "SuppressedError",
  "Promise",
  "Proxy",
  "Reflect",
  "Map",
  "Set",
  "WeakMap",
  "WeakSet",
  "WeakRef",
  "FinalizationRegistry",
  "ArrayBuffer",
  "SharedArrayBuffer",
  "DataView",
  "Int8Array",
  "Uint8Array",
  "Uint8ClampedArray",
  "Int16Array",
  "Uint16Array",
  "Int32Array",
  "Uint32Array",
  "Float16Array",
  "Float32Array",
  "Float64Array",
  "BigInt64Array",
  "BigUint64Array",
  "Atomics",
  "Intl",
  "WebAssembly",
  "globalThis",
  "Function",
  "eval",
  "arguments",
  "undefined",
  "NaN",
  "Infinity",
  "parseInt",
  "parseFloat",
  "isNaN",
  "isFinite",
  "escape",
  "unescape",
  "encodeURI",
  "decodeURI",
  "encodeURIComponent",
  "decodeURIComponent",
  "Iterator",
  "AsyncFunction",
  "GeneratorFunction",
  "AsyncGeneratorFunction"
];

/** Web platform APIs, present in browsers and in most modern server runtimes. */
const WEB = [
  "console",
  "fetch",
  "Headers",
  "Request",
  "Response",
  "FormData",
  "Blob",
  "File",
  "FileReader",
  "URL",
  "URLSearchParams",
  "TextEncoder",
  "TextDecoder",
  "TextEncoderStream",
  "TextDecoderStream",
  "ReadableStream",
  "WritableStream",
  "TransformStream",
  "ByteLengthQueuingStrategy",
  "CountQueuingStrategy",
  "AbortController",
  "AbortSignal",
  "Event",
  "EventTarget",
  "CustomEvent",
  "MessageChannel",
  "MessagePort",
  "BroadcastChannel",
  "Worker",
  "WebSocket",
  "XMLHttpRequest",
  "DOMException",
  "performance",
  "crypto",
  "SubtleCrypto",
  "structuredClone",
  "queueMicrotask",
  "setTimeout",
  "clearTimeout",
  "setInterval",
  "clearInterval",
  "requestAnimationFrame",
  "cancelAnimationFrame",
  "requestIdleCallback",
  "cancelIdleCallback",
  "btoa",
  "atob",
  "navigator",
  "location",
  "history",
  "screen",
  "caches",
  "indexedDB",
  "localStorage",
  "sessionStorage",
  "reportError",
  "CompressionStream",
  "DecompressionStream",
  "EventSource",
  "Notification",
  "Image",
  "Audio"
];

/** Browser DOM — only meaningful when the bundle targets a page. */
const BROWSER = [
  "window",
  "document",
  "self",
  "parent",
  "top",
  "frames",
  "opener",
  "getComputedStyle",
  "matchMedia",
  "scrollTo",
  "scrollBy",
  "alert",
  "confirm",
  "prompt",
  "open",
  "close",
  "print",
  "focus",
  "blur",
  "postMessage",
  "Node",
  "Element",
  "HTMLElement",
  "Document",
  "DocumentFragment",
  "ShadowRoot",
  "MutationObserver",
  "IntersectionObserver",
  "ResizeObserver",
  "customElements",
  "CSS",
  "FontFace",
  "Range",
  "Selection",
  "DOMParser",
  "XMLSerializer",
  "XPathResult",
  "NodeFilter",
  "AbortPaymentEvent"
];

/** Node.js and CommonJS module plumbing. */
const NODE = [
  "require",
  "module",
  "exports",
  "process",
  "Buffer",
  "__dirname",
  "__filename",
  "global",
  "setImmediate",
  "clearImmediate",
  "gc",
  "AsyncLocalStorage"
];

/** Bun, Deno and edge/worker runtimes. */
const ALT_RUNTIME = [
  "Bun",
  "Deno",
  "EdgeRuntime",
  "WebSocketPair",
  "Cloudflare",
  "caches",
  "HTMLRewriter",
  "ExecutionContext",
  "ScheduledEvent",
  "FetchEvent"
];

/** Environments this module knows about. Add a set, add a key. */
export const GLOBAL_ENVIRONMENTS = {
  ecmascript: ECMASCRIPT,
  web: WEB,
  browser: BROWSER,
  node: NODE,
  altRuntime: ALT_RUNTIME
} as const;

export type GlobalEnvironment = keyof typeof GLOBAL_ENVIRONMENTS;

/**
 * The globals for the given environments; all of them when none is named.
 *
 * All-of-them is the honest default for a bundle: the Claude Code binary
 * references Bun, Node and web APIs in one tree, and narrowing by guesswork
 * would turn a real global into a reported leftover.
 */
export function knownGlobals(...envs: GlobalEnvironment[]): Set<string> {
  const keys =
    envs.length > 0
      ? envs
      : (Object.keys(GLOBAL_ENVIRONMENTS) as GlobalEnvironment[]);
  const out = new Set<string>();
  for (const key of keys) {
    for (const name of GLOBAL_ENVIRONMENTS[key]) out.add(name);
  }
  return out;
}

/** Every known global, memoized — the common case. */
let allCache: Set<string> | undefined;
export function isKnownGlobal(name: string): boolean {
  allCache ??= knownGlobals();
  return allCache.has(name);
}
