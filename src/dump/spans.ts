/**
 * UTF-16 → UTF-8 byte offset conversion for the artifact dump (07 §1).
 *
 * The TS pipeline's native spans (Babel `node.start/end`) are UTF-16 code-unit
 * indices into a JS string; the dumps persist UTF-8 BYTE offsets because oxc
 * spans are byte offsets natively and the dumps outlive the port. The
 * conversion must mirror Node's own UTF-8 encoding exactly — a byte offset is
 * only meaningful against the file `fs.writeFileSync(text, "utf8")` writes,
 * in which an unpaired surrogate becomes U+FFFD (3 bytes).
 *
 * Two guards are mandatory (07 §1):
 *  (a) when `Buffer.byteLength(text) === text.length` the mapping is the
 *      identity, asserted as an explicit fast path so ASCII files cost
 *      nothing (and can hold no surrogate pairs, so no guard needed);
 *  (b) an endpoint that lands INSIDE a paired surrogate is a dumper bug —
 *      fail loud, never round.
 *
 * The table is built in ONE pass over the text (never per-span slicing) and
 * memoized; `convertSpanEndpoints` answers a whole batch from it.
 */

const INTERIOR = 0xffffffff; // sentinel: inside a paired surrogate

export class ByteOffsetTable {
  private constructor(
    private readonly text: string,
    private readonly identity: boolean,
    private table?: Uint32Array
  ) {}

  static for(text: string): ByteOffsetTable {
    const identity = Buffer.byteLength(text, "utf8") === text.length;
    return new ByteOffsetTable(text, identity);
  }

  /** True when every code unit is one UTF-8 byte — offsets pass through. */
  get isIdentity(): boolean {
    return this.identity;
  }

  /** Convert one UTF-16 code-unit index to a UTF-8 byte offset. */
  toByte(utf16Index: number): number {
    if (utf16Index < 0 || utf16Index > this.text.length) {
      throw new Error(
        `span endpoint ${utf16Index} outside the anchored text (length ${this.text.length})`
      );
    }
    if (this.identity) return utf16Index;
    const table = this.ensureTable();
    const entry = table[utf16Index];
    if (entry === INTERIOR) {
      throw new Error(
        `span endpoint ${utf16Index} lands inside a surrogate pair — a dumper bug (07 §1 guard b); refusing to round`
      );
    }
    return entry;
  }

  private ensureTable(): Uint32Array {
    if (this.table) return this.table;
    const text = this.text;
    const n = text.length;
    const table = new Uint32Array(n + 1);
    let byte = 0;
    let i = 0;
    while (i < n) {
      table[i] = byte;
      const units = utf8UnitCount(text, i);
      if (units.interior) {
        // Paired surrogate: the interior code unit is marked so a span
        // endpoint there fails loud instead of rounding.
        table[i + 1] = INTERIOR;
      }
      byte += units.bytes;
      i += units.codeUnits;
    }
    table[n] = byte;
    this.table = table;
    return table;
  }
}

/**
 * How many UTF-8 bytes the code units starting at `i` encode, and how many
 * UTF-16 code units they span. Paired surrogates are one 4-byte character;
 * an unpaired surrogate encodes as U+FFFD (3 bytes) — Node's own utf8
 * encoding, which is what a written file's byte offsets are.
 */
function utf8UnitCount(
  text: string,
  i: number
): { bytes: number; codeUnits: number; interior: boolean } {
  const code = text.charCodeAt(i);
  const next = i + 1 < text.length ? text.charCodeAt(i + 1) : 0;
  if (code >= 0xd800 && code <= 0xdbff && next >= 0xdc00 && next <= 0xdfff) {
    return { bytes: 4, codeUnits: 2, interior: true };
  }
  if (
    (code >= 0xd800 && code <= 0xdfff) ||
    code === 0xfffe ||
    code === 0xffff
  ) {
    return { bytes: 3, codeUnits: 1, interior: false };
  }
  if (code < 0x80) return { bytes: 1, codeUnits: 1, interior: false };
  if (code < 0x800) return { bytes: 2, codeUnits: 1, interior: false };
  return { bytes: 3, codeUnits: 1, interior: false };
}
