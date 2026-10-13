//! Globals a bundled file may legitimately reference (WP1.5) — TS original:
//! `src/analysis/known-globals.ts`.
//!
//! Used to tell a real runtime name apart from a leftover the renamer never
//! reached. Structured as a BASE plus per-environment sets, because the
//! right answer depends on what is being deobfuscated ("maybe in the future
//! we need to populate it as a base + plugin system" — the plug point is
//! `known_globals(...envs)`; today every caller asks for all of them).
//!
//! ## This list can be wrong, and that bounds what may depend on it
//!
//! A first version omitted `Bun`, `btoa`, `crypto` and `Blob`, and so
//! reported 16 unreachable free references where there were 2. That is fine
//! for a report and disqualifying for a rewrite: renaming a free reference
//! changes a global lookup, and `typeof Bun !== "undefined"` is a feature
//! check whose answer flips if the name is rewritten. So this drives
//! REPORTING only. If something ever wants to rename on the strength of it,
//! that caller needs a different and much stronger justification than "not
//! in our list".
//!
//! The module also owns the function fingerprint's frozen globals
//! vocabulary ([`is_fingerprint_global`], at the end) — a different
//! question with a declared, tested relation to these sets.

/// ECMAScript itself — true in every host.
const ECMASCRIPT: &[&str] = &[
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
    "AsyncGeneratorFunction",
];

/// Web platform APIs, present in browsers and in most modern server runtimes.
const WEB: &[&str] = &[
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
    "Audio",
];

/// Browser DOM — only meaningful when the bundle targets a page.
const BROWSER: &[&str] = &[
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
    "AbortPaymentEvent",
];

/// Node.js host globals. The CommonJS module plumbing (`require`,
/// `module`, `exports`, `__filename`, `__dirname`) is its own environment,
/// read from the toolchain's one list (`toolchain::COMMONJS_CONTEXT`,
/// review R21).
const NODE: &[&str] = &[
    "process",
    "Buffer",
    "global",
    "setImmediate",
    "clearImmediate",
    "gc",
    "AsyncLocalStorage",
];

/// Bun, Deno and edge/worker runtimes.
const ALT_RUNTIME: &[&str] = &[
    "Bun",
    "Deno",
    "EdgeRuntime",
    "WebSocketPair",
    "Cloudflare",
    "caches",
    "HTMLRewriter",
    "ExecutionContext",
    "ScheduledEvent",
    "FetchEvent",
];

/// Environments this module knows about. Add a set, add a key
/// (GLOBAL_ENVIRONMENTS — key order matters to `known_globals` with no
/// arguments: the TS Object.keys order is declaration order).
pub const GLOBAL_ENVIRONMENTS: [(&str, &[&str]); 6] = [
    ("ecmascript", ECMASCRIPT),
    ("web", WEB),
    ("browser", BROWSER),
    ("commonjs", &crate::toolchain::COMMONJS_CONTEXT_NAMES),
    ("node", NODE),
    ("altRuntime", ALT_RUNTIME),
];

/// The globals for the given environments; all of them when none is named.
///
/// All-of-them is the honest default for a bundle: the Claude Code binary
/// references Bun, Node and web APIs in one tree, and narrowing by guesswork
/// would turn a real global into a reported leftover.
pub fn known_globals(envs: &[&str]) -> Vec<&'static str> {
    let keys: Vec<&str> = if envs.is_empty() {
        GLOBAL_ENVIRONMENTS.iter().map(|(k, _)| *k).collect()
    } else {
        envs.to_vec()
    };
    let mut out: Vec<&'static str> = Vec::new();
    for key in keys {
        let Some((_, names)) = GLOBAL_ENVIRONMENTS.iter().find(|(k, _)| *k == key) else {
            continue;
        };
        for name in *names {
            if !out.contains(name) {
                out.push(name);
            }
        }
    }
    out
}

/// Every known global — the common case (isKnownGlobal).
pub fn is_known_global(name: &str) -> bool {
    GLOBAL_ENVIRONMENTS
        .iter()
        .any(|(_, names)| names.contains(&name))
}

// ---------------------------------------------------------------------------
// The FINGERPRINT vocabulary — a frozen, named subset of this module
// ---------------------------------------------------------------------------
//
// The function fingerprint's `externalCalls` feature (`matching::features`)
// keeps a callee's name only when it is one of THESE globals: TS
// `KNOWN_GLOBALS` (structural-hash.ts :11-102), verbatim. It is a FROZEN
// vocabulary, not an answer to "what may a bundle reference" (the
// environment sets above): changing it changes every function's match key
// (toolchain review R21, declared). It lived as a local list inside
// `matching::features` until 2026-10-13; it is owned here so the two
// questions sit side by side with their relation declared and tested
// (`modules_test::the_fingerprint_vocabulary_is_the_frozen_structural_hash_list`):
//
// - [`FINGERPRINT_HOST_GLOBALS`] — every one is ALSO a reporting global;
// - [`FINGERPRINT_EXTRA_GLOBALS`] — counted by the fingerprint, unknown to
//   the reporting sets.
//
// The difference is deliberate in both directions. The vocabulary is
// SMALLER than the reporting sets (no `Bun`, `crypto`, `globalThis`,
// `exports`, `module`, … — `Bun.write('b')` fingerprints as `*.write`), and
// `$` is absent (the first name of minifier alphabets — as jQuery it would
// make features rename-variant). Adding a reporting global to the
// vocabulary, or an extra to the reporting sets, is a BEHAVIOUR change
// (match keys; the vendor content key and the census read the reporting
// sets), never a tidy-up.

/// The fingerprint's globals that are also reporting globals (see the
/// block comment above), in the TS source's order.
pub const FINGERPRINT_HOST_GLOBALS: [&str; 79] = [
    "fetch",
    "setTimeout",
    "setInterval",
    "clearTimeout",
    "clearInterval",
    "requestAnimationFrame",
    "cancelAnimationFrame",
    "alert",
    "confirm",
    "prompt",
    "console",
    "document",
    "window",
    "navigator",
    "location",
    "history",
    "localStorage",
    "sessionStorage",
    "indexedDB",
    "XMLHttpRequest",
    "WebSocket",
    "Worker",
    "Blob",
    "File",
    "FileReader",
    "URL",
    "URLSearchParams",
    "FormData",
    "Headers",
    "Request",
    "Response",
    "AbortController",
    "CustomEvent",
    "MutationObserver",
    "IntersectionObserver",
    "ResizeObserver",
    "require",
    "process",
    "Buffer",
    "global",
    "__dirname",
    "__filename",
    "Array",
    "Object",
    "String",
    "Number",
    "Boolean",
    "Symbol",
    "BigInt",
    "Map",
    "Set",
    "WeakMap",
    "WeakSet",
    "Promise",
    "Proxy",
    "Reflect",
    "JSON",
    "Math",
    "Date",
    "RegExp",
    "Error",
    "TypeError",
    "RangeError",
    "SyntaxError",
    "ReferenceError",
    "URIError",
    "EvalError",
    "Function",
    "eval",
    "parseInt",
    "parseFloat",
    "isNaN",
    "isFinite",
    "encodeURI",
    "decodeURI",
    "encodeURIComponent",
    "decodeURIComponent",
    "atob",
    "btoa",
];

/// The fingerprint's globals the reporting sets do not hold:
/// `PerformanceObserver` (a real web global the reporting `WEB` set lacks —
/// adding it there would move vendor content keys) and the TS list's
/// "common library patterns" (`jQuery`, `React`, `Vue`, `angular`), which
/// are library bindings, not host globals.
pub const FINGERPRINT_EXTRA_GLOBALS: [&str; 5] =
    ["PerformanceObserver", "jQuery", "React", "Vue", "angular"];

/// Whether a free callee name is fingerprint vocabulary (TS
/// `KNOWN_GLOBALS.has`).
pub fn is_fingerprint_global(name: &str) -> bool {
    FINGERPRINT_HOST_GLOBALS.contains(&name) || FINGERPRINT_EXTRA_GLOBALS.contains(&name)
}
