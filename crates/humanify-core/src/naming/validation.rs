//! Name legality tables and the conflict ladder — src/llm/validation.ts,
//! the DECORATION_WORDS owner.
//!
//! The three tables are pinned to the TS's in its order
//! (validation_test::tables_equal_the_ts_tables, from
//! test/parity/wp42-vectors.json). GLOBAL_BUILTINS is DERIVED in the TS —
//! `Object.keys` of the `globals` package's `builtin`, `nodeBuiltin` and
//! `shared-node-browser` sets (globals@17.4.0, the version node_modules
//! resolves) plus the curated host list — so a `globals` bump changes the
//! TS table and the pin test goes red until this list is regenerated from
//! the probe.

/// JavaScript reserved words that cannot be used as identifiers
/// (keywords, strict-mode reserved words incl. `arguments`, literals, and
/// the three global values).
pub const RESERVED_WORDS: &[&str] = &[
    "break",
    "case",
    "catch",
    "continue",
    "debugger",
    "default",
    "delete",
    "do",
    "else",
    "finally",
    "for",
    "function",
    "if",
    "in",
    "instanceof",
    "new",
    "return",
    "switch",
    "this",
    "throw",
    "try",
    "typeof",
    "var",
    "void",
    "while",
    "with",
    "class",
    "const",
    "enum",
    "export",
    "extends",
    "import",
    "super",
    "arguments",
    "implements",
    "interface",
    "let",
    "package",
    "private",
    "protected",
    "public",
    "static",
    "yield",
    "await",
    "null",
    "true",
    "false",
    "undefined",
    "NaN",
    "Infinity",
];

/// Runtime globals a rename must never shadow, in the TS Set's order: ES
/// builtins, Node builtins, shared node/browser globals (all from
/// `globals`), then the curated HIGH_RISK_HOST_GLOBALS (review C1 — a
/// deliberately short list, not all of globals.browser, which would forbid
/// ~1,125 desirable names such as `event` and `status`).
pub const GLOBAL_BUILTINS: &[&str] = &[
    "AggregateError",
    "Array",
    "ArrayBuffer",
    "Atomics",
    "BigInt",
    "BigInt64Array",
    "BigUint64Array",
    "Boolean",
    "DataView",
    "Date",
    "decodeURI",
    "decodeURIComponent",
    "encodeURI",
    "encodeURIComponent",
    "Error",
    "escape",
    "eval",
    "EvalError",
    "FinalizationRegistry",
    "Float16Array",
    "Float32Array",
    "Float64Array",
    "Function",
    "globalThis",
    "Infinity",
    "Int16Array",
    "Int32Array",
    "Int8Array",
    "Intl",
    "isFinite",
    "isNaN",
    "Iterator",
    "JSON",
    "Map",
    "Math",
    "NaN",
    "Number",
    "Object",
    "parseFloat",
    "parseInt",
    "Promise",
    "Proxy",
    "RangeError",
    "ReferenceError",
    "Reflect",
    "RegExp",
    "Set",
    "SharedArrayBuffer",
    "String",
    "Symbol",
    "SyntaxError",
    "TypeError",
    "Uint16Array",
    "Uint32Array",
    "Uint8Array",
    "Uint8ClampedArray",
    "undefined",
    "unescape",
    "URIError",
    "WeakMap",
    "WeakRef",
    "WeakSet",
    "AbortController",
    "AbortSignal",
    "AsyncDisposableStack",
    "atob",
    "Blob",
    "BroadcastChannel",
    "btoa",
    "Buffer",
    "ByteLengthQueuingStrategy",
    "clearImmediate",
    "clearInterval",
    "clearTimeout",
    "CloseEvent",
    "CompressionStream",
    "console",
    "CountQueuingStrategy",
    "crypto",
    "Crypto",
    "CryptoKey",
    "CustomEvent",
    "DecompressionStream",
    "DisposableStack",
    "DOMException",
    "ErrorEvent",
    "Event",
    "EventTarget",
    "fetch",
    "File",
    "FormData",
    "global",
    "Headers",
    "localStorage",
    "MessageChannel",
    "MessageEvent",
    "MessagePort",
    "navigator",
    "Navigator",
    "performance",
    "Performance",
    "PerformanceEntry",
    "PerformanceMark",
    "PerformanceMeasure",
    "PerformanceObserver",
    "PerformanceObserverEntryList",
    "PerformanceResourceTiming",
    "process",
    "queueMicrotask",
    "ReadableByteStreamController",
    "ReadableStream",
    "ReadableStreamBYOBReader",
    "ReadableStreamBYOBRequest",
    "ReadableStreamDefaultController",
    "ReadableStreamDefaultReader",
    "Request",
    "Response",
    "sessionStorage",
    "setImmediate",
    "setInterval",
    "setTimeout",
    "Storage",
    "structuredClone",
    "SubtleCrypto",
    "SuppressedError",
    "TextDecoder",
    "TextDecoderStream",
    "TextEncoder",
    "TextEncoderStream",
    "TransformStream",
    "TransformStreamDefaultController",
    "URL",
    "URLPattern",
    "URLSearchParams",
    "WebAssembly",
    "WebSocket",
    "WritableStream",
    "WritableStreamDefaultController",
    "WritableStreamDefaultWriter",
    // HIGH_RISK_HOST_GLOBALS
    "window",
    "document",
    "self",
    "location",
    "$",
    "jQuery",
    "define",
    "Bun",
    "importScripts",
    "postMessage",
];

/// The decoration words the conflict ladder appends, in ladder order.
/// Single source: the prior-name snap's stem stripper (WP4.5) derives from
/// this list, so every producible decoration is also strippable.
pub const DECORATION_WORDS: &[&str] = &["Val", "Var", "Ref", "Item", "Data", "Result", "Value"];

pub fn is_reserved_word(name: &str) -> bool {
    RESERVED_WORDS.contains(&name)
}

pub fn is_global_builtin(name: &str) -> bool {
    GLOBAL_BUILTINS.contains(&name)
}

fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_' || b == b'$'
}

fn is_ident_part(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '$'
}

/// `/^[a-zA-Z_$][a-zA-Z0-9_$]*$/` — ASCII only, by design: a non-ASCII
/// letter is valid JS but not a valid rename target here.
pub fn is_valid_identifier(name: &str) -> bool {
    let bytes = name.as_bytes();
    !bytes.is_empty() && is_ident_start(bytes[0]) && name.chars().all(is_ident_part)
}

/// The fallback when validation fails: strip every character outside
/// `[a-zA-Z0-9_$]` (per UTF-16 unit in the TS — an astral character's two
/// surrogates both go, same as removing the char), `_`-prefix a leading
/// digit, `_unnamed` for nothing left, `_`-suffix a reserved word or
/// global builtin.
pub fn sanitize_identifier(name: &str) -> String {
    let mut s: String = name.chars().filter(|c| is_ident_part(*c)).collect();
    if s.as_bytes().first().is_some_and(u8::is_ascii_digit) {
        s.insert(0, '_');
    }
    if s.is_empty() {
        s = "_unnamed".to_string();
    }
    if is_reserved_word(&s) || is_global_builtin(&s) {
        s.push('_');
    }
    s
}

/// Resolves a naming conflict by DECORATING, never by inventing: the
/// decoration words, then `name2..name999`, then `nameVal2, nameVal3, …`
/// (terminates because the used set is finite). Every rung is reducible
/// back to the input by the prior-name snap's stem stripper.
pub fn resolve_conflict(name: &str, is_used: impl Fn(&str) -> bool) -> String {
    for suffix in DECORATION_WORDS {
        let candidate = format!("{name}{suffix}");
        if !is_used(&candidate) {
            return candidate;
        }
    }
    for i in 2..=999 {
        let candidate = format!("{name}{i}");
        if !is_used(&candidate) {
            return candidate;
        }
    }
    (2u64..)
        .map(|i| format!("{name}Val{i}"))
        .find(|c| !is_used(c))
        .expect("the used set is finite")
}

#[cfg(test)]
mod validation_test;
