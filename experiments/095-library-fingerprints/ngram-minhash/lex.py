"""Token streams for n-gram fingerprints.

The lexer is a rule-for-rule port of humanify's
crates/humanify-core/src/modules/vendor_pairing.rs (lex_with), so the `H`
stream reproduces humanify's content-pairing features exactly. Other streams
are this experiment's own normalisations:

  H   humanify: identifiers of <=4 chars (or lib_<hex8>) masked to I, long
      strings (>40 chars incl. quotes) split into words between <str> </str>.
  B   bindings masked: every identifier is I unless it is a property name
      (after . or ?.), an object key (prev { or , and next :), a known global,
      or a keyword. Strings normalised to their value (quote style dropped,
      no-substitution templates = strings), numbers normalised by value.
  BX  B with literal VALUES dropped: strings -> S, numbers -> N, regex -> R.
  L   survivors only: the property names / keys / globals of B plus string,
      number and regex values. No keywords, no punctuation, no bindings.
  LX  L without literal values: property names / keys / globals only.

Every token is hashed with Python's hash() under PYTHONHASHSEED=0 (checked).
"""
import os
import re

assert os.environ.get("PYTHONHASHSEED") == "0", "run with PYTHONHASHSEED=0"

KEYWORDS = set(
    "var let const function return if else for while do switch case break continue new this typeof "
    "instanceof in of void delete throw try catch finally class extends super null true false undefined "
    "async await yield get set static default".split()
)
REGEX_AFTER_WORDS = set("return typeof case do else in of new delete void throw instanceof yield await".split())
REGEX_AFTER_CHARS = set("(,=:[!&|?{};+-*%<>~^")
REGEX_AFTER_OPS = {"=>", "&&", "||", "??", "==", "===", "!=", "!==", "+=", "-="}
PUNCTUATORS = [">>>=", "...", "===", "!==", "**=", "<<=", ">>=", ">>>", "=>", "==", "!=", "<=", ">=", "&&",
               "||", "??", "?.", "++", "--", "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", "<<", ">>"]
PUNCT_CHARS = set("{}()[];,<>+-*/%&|^!~?:=.@#")
# humanify known_globals.rs plus the CommonJS names
GLOBALS = set("""undefined Object Array String Number Boolean Symbol BigInt Math JSON Date RegExp Error TypeError
RangeError SyntaxError ReferenceError EvalError URIError AggregateError SuppressedError Promise Proxy Reflect Map Set
WeakMap WeakSet WeakRef FinalizationRegistry ArrayBuffer SharedArrayBuffer DataView Int8Array Uint8Array
Uint8ClampedArray Int16Array Uint16Array Int32Array Uint32Array Float16Array Float32Array Float64Array BigInt64Array
BigUint64Array Atomics Intl WebAssembly globalThis Function eval arguments NaN Infinity parseInt parseFloat isNaN
isFinite escape unescape encodeURI decodeURI encodeURIComponent decodeURIComponent Iterator console fetch Headers
Request Response FormData Blob File FileReader URL URLSearchParams TextEncoder TextDecoder ReadableStream
WritableStream TransformStream AbortController AbortSignal Event EventTarget CustomEvent MessageChannel MessagePort
Worker WebSocket XMLHttpRequest DOMException performance crypto structuredClone queueMicrotask setTimeout
clearTimeout setInterval clearInterval btoa atob navigator location window document self process Buffer global
setImmediate clearImmediate Bun Deno require module exports __dirname __filename define""".split())

_ID_START = re.compile(r"[A-Za-z_$\u0080-￿]")
_ID = re.compile(r"[A-Za-z_$\u0080-￿][\w$\u0080-￿]*")
_WS = re.compile(r"[ \t\r\n]+")
_NUM = re.compile(
    r"0[xXbBoO][0-9a-fA-F_]+n?|[0-9][0-9_]*(?:\.[0-9]*)?(?:[eE][+-]?[0-9]+)?n?|\.[0-9]+(?:[eE][+-]?[0-9]+)?n?"
)


def _is_minified(t):
    if t.startswith("lib_") and len(t) >= 12 and re.fullmatch(r"[0-9a-f]{8}(_[0-9]+)?", t[4:]):
        return True
    c = t[0]
    return (c.isascii() and c.isalpha() or c in "_$") and len(t) <= 4


def _string_end(s, i, n):
    q = s[i]
    j = i + 1
    while j < n and s[j] != q:
        j += 2 if s[j] == "\\" else 1
    return j + 1


def _template_end(s, i, n):
    j = i + 1
    depth = 0
    while j < n:
        c = s[j]
        if c == "\\":
            j += 2
            continue
        if depth == 0 and c == "`":
            break
        if c == "$" and j + 1 < n and s[j + 1] == "{":
            depth += 1
            j += 2
            continue
        if depth > 0 and c == "}":
            depth -= 1
        j += 1
    return j + 1


def _regex_end(s, i, n):
    j = i + 1
    cls = False
    while j < n and s[j] != "\n":
        c = s[j]
        if c == "\\":
            j += 2
            continue
        if c == "[":
            cls = True
        elif c == "]":
            cls = False
        elif c == "/" and not cls:
            break
        j += 1
    j += 1
    while j < n and s[j].isalpha():
        j += 1
    return j


def _regex_may_follow(prev):
    if prev is None:
        return True
    return prev in REGEX_AFTER_WORDS or (len(prev) == 1 and prev in REGEX_AFTER_CHARS) or prev in REGEX_AFTER_OPS


def lex_raw(text):
    """[(kind, token, char_pos)] with kind in id/str/tpl/re/num/p. Comments and whitespace dropped."""
    s = text
    n = len(s)
    out = []
    prev = None
    i = 0
    while i < n:
        c = s[i]
        if c in " \t\r\n":
            m = _WS.match(s, i)
            i = m.end()
            continue
        if c == "/" and i + 1 < n and s[i + 1] == "/":
            j = s.find("\n", i)
            i = n if j < 0 else j
            continue
        if c == "/" and i + 1 < n and s[i + 1] == "*":
            j = s.find("*/", i + 2)
            i = n if j < 0 else j + 2
            continue
        kind = None
        if c == '"' or c == "'":
            end, kind = _string_end(s, i, n), "str"
        elif c == "`":
            end, kind = _template_end(s, i, n), "tpl"
        elif c == "/" and _regex_may_follow(prev):
            end, kind = _regex_end(s, i, n), "re"
        if kind:
            tok = s[i:end]
            out.append((kind, tok, i))
            prev = tok
            i = end
            continue
        if _ID_START.match(c):
            m = _ID.match(s, i)
            tok = m.group()
            out.append(("id", tok, i))
            prev = tok
            i = m.end()
            continue
        end = None
        if c.isdigit() or c == ".":
            m = _NUM.match(s, i)
            if m and m.end() > i:
                end = m.end()
                kind = "num"
        if end is None:
            for p in PUNCTUATORS:
                if s.startswith(p, i):
                    end = i + len(p)
                    break
            else:
                end = i + 1
            kind = "p"
        tok = s[i:end]
        out.append((kind, tok, i))
        prev = tok
        i = end
    return out


def _words(text, out):
    run = []
    for c in text:
        if c.isalnum() or c == "_":
            run.append(c)
            continue
        if run:
            out.append("".join(run))
            run = []
        if not c.isspace():
            out.append(c)
    if run:
        out.append("".join(run))


def _tpl_static(t):
    """A template literal with every ${...} substitution emptied (its minified identifiers are not stable)."""
    out = []
    j, n, depth = 0, len(t), 0
    while j < n:
        c = t[j]
        if depth == 0:
            if c == "\\":
                out.append(t[j:j + 2])
                j += 2
                continue
            if c == "$" and j + 1 < n and t[j + 1] == "{":
                out.append("${}")
                depth = 1
                j += 2
                continue
            out.append(c)
        else:
            if c == "{":
                depth += 1
            elif c == "}":
                depth -= 1
        j += 1
    return "".join(out)


def _num_value(t):
    try:
        u = t.replace("_", "")
        if u.endswith("n"):
            return "N:" + str(int(u[:-1], 0))
        lo = u.lower()
        if lo.startswith(("0x", "0b", "0o")):
            return "N:" + repr(float(int(u, 0)))
        return "N:" + repr(float(u))
    except ValueError:
        return "N:" + t


def streams(text, positions=False):
    """dict name -> list[str] of the five streams, plus 'lits' (humanify literal set).
    positions=True: also dict name -> list[int] of each token's char offset (H's split words share one)."""
    toks = lex_raw(text)
    if positions:
        out, lits = _streams(toks)
        pos = {}
        for name in STREAMS:
            pos[name] = [p for p, _ in out[name]]
            out[name] = [t for _, t in out[name]]
        return out, lits, pos
    out, lits = _streams(toks)
    return {k: [t for _, t in v] for k, v in out.items()}, lits


class _Rec(list):
    """A list that records (pos, token) pairs; .pos is set by the caller before appending."""
    pos = 0

    def append(self, t):
        super().append((self.pos, t))


def _streams(toks):
    H, B, BX, L, LX = _Rec(), _Rec(), _Rec(), _Rec(), _Rec()
    lits = set()
    m = len(toks)
    for k in range(m):
        kind, t, p = toks[k]
        H.pos = B.pos = BX.pos = L.pos = LX.pos = p
        # humanify H stream and literal set
        if kind in ("str", "tpl", "re"):
            if kind != "re" and t[:1] in "\"'`" and len(t) >= 5:
                lits.add(t[1:-1])
            if t[:1] in "\"'`" and len(t) > 40:
                H.append("<str>")
                ws = []
                _words(t[1:-1], ws)
                for w in ws:
                    H.append(w)
                H.append("</str>")
            else:
                H.append(t)
        elif kind == "id":
            if len(t) >= 5:
                lits.add("#" + t)
            H.append("I" if (t not in KEYWORDS and _is_minified(t)) else t)
        else:
            H.append(t)
        # B / L streams
        if kind == "id":
            prev = toks[k - 1][1] if k else None
            nxt = toks[k + 1][1] if k + 1 < m else None
            if t in KEYWORDS:
                B.append(t)
                BX.append(t)
                continue
            keep = prev in (".", "?.") or (nxt == ":" and prev in ("{", ",")) or t in GLOBALS
            if keep:
                B.append(t)
                BX.append(t)
                L.append(t)
                LX.append(t)
            else:
                B.append("I")
                BX.append("I")
        elif kind == "str" or (kind == "tpl" and "${" not in t):
            v = "S:" + t[1:-1]
            B.append(v)
            BX.append("S")
            L.append(v)
        elif kind == "tpl":
            v = "T:" + _tpl_static(t)
            B.append(v)
            BX.append("T")
            L.append(v)
        elif kind == "num":
            v = _num_value(t)
            B.append(v)
            BX.append("N")
            L.append(v)
        elif kind == "re":
            B.append("R:" + t)
            BX.append("R")
            L.append("R:" + t)
        else:
            B.append(t)
            BX.append(t)
    return {"H": H, "B": B, "BX": BX, "L": L, "LX": LX}, lits


STREAMS = ["H", "B", "BX", "L", "LX"]
