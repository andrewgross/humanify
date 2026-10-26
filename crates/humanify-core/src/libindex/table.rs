//! The index's LOOKUP tables (`humanify-library-index/3`,
//! docs/design/library-index.md §4): everything the lookup reads, in
//! sorted binary columns read on demand — never parsed into a map on load.
//!
//! A run opens a table by reading its header and its FENCE (every
//! [`BLOCK`]-th key) and nothing else. A lookup reads one block of keys
//! and, on a hit, the rows it points at, by positional reads of the file
//! (the OS page cache is the cache, shared by every run on the machine).
//! What a process holds is therefore ~1/[`BLOCK`] of the keys, not the
//! index: an index 50x larger costs a run 50x more fence, not 50x more
//! heap. (`humanify-library-index/2` parsed every JSONL row into hash maps
//! on load: 2.3 s and ~1.3 GB per run on the 577 MB step-4 index.)
//!
//! Five tables, one writer and one reader each, in this file:
//!
//! - [`PACKAGES_FILE`] — every owner package, sorted by name; an owner id
//!   is its position.
//! - `units.<slice>.bin` ([`UnitTable`]) — per distinct unit hash (16 hex
//!   digits read as a `u64`, [`unit_key`]) the rows holding it, in row
//!   order: (owner id, smallest module bytes). What the structural rule,
//!   specificity and containment read; a unit row's file, nodes, content
//!   and versions stay in `units.<slice>.jsonl` (provenance: the scorer's,
//!   never the run's).
//! - `edges.<slice>.bin` ([`EdgeTable`], format `/4`) — per unit row
//!   (its position in `units.<slice>.bin`, the key) the callees its
//!   statement references ([`super::format::EdgeRow`]): (owner id, print)
//!   pairs, each distinct callee list stored once. What a store's INSIDE /
//!   OUT-OF edge read answers; the row table `edges.<slice>.jsonl` is
//!   provenance.
//! - `callee-prints.<slice>.bin` ([`CalleePrintTable`], format `/4`,
//!   edges/2) — every DISTINCT callee print any edge row records, with the
//!   owners whose rows record it. Derived from the same rows at merge
//!   time, so a print needs no unit row to be queryable by key (a callee
//!   can exist in the edge data without owning a row); an INSIDE entry
//!   credits the row's owner, an OUT-OF entry the callee's own recorded
//!   owner.
//! - [`LITERALS_FILE`] ([`LiteralTable`]) — every distinct literal (keyed
//!   by [`literal_key`], confirmed against its stored string), with the
//!   number of packages holding it and the files holding it; per literal
//!   file its owner and sorted literal ids. A file's path and versions are
//!   in `literals.files.jsonl` (provenance), row for row.
//!
//! All integers little-endian. Ids are positions: a literal's is its place
//! in (key, string) order, a file's its row's.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::ops::Range;
use std::path::Path;

use super::format::{EdgeRow, LiteralRow, UnitRow};
use super::lookup::UnitHit;

/// Every owner package, sorted (a JSON array of names).
pub const PACKAGES_FILE: &str = "packages.json";
/// The literal lookup table.
pub const LITERALS_FILE: &str = "literals.bin";
/// The helper table (exp129, [`super::format::HelperRow`] lines); an index
/// whose manifest lists none has no helper prints.
pub const HELPERS_FILE: &str = "helpers.jsonl";

/// A slice's unit lookup table.
pub fn units_file(slice: &str) -> String {
    format!("units.{slice}.bin")
}

/// A slice's edge lookup table (format `/4`).
pub fn edges_file(slice: &str) -> String {
    format!("edges.{slice}.bin")
}

/// A slice's edge row table (provenance, format `/4`).
pub fn edge_rows_file(slice: &str) -> String {
    format!("edges.{slice}.jsonl")
}

/// A slice's callee-print table (format `/4`, edges/2): the flat index of
/// every callee print the slice's edge rows record ([`CalleePrintTable`]).
pub fn callee_prints_file(slice: &str) -> String {
    format!("callee-prints.{slice}.bin")
}

/// Keys per fence entry: what one lookup reads (512 bytes of keys), and
/// the share of a key column a process holds (1/64).
pub const BLOCK: usize = 64;

const UNITS_MAGIC: &[u8; 8] = b"HXUNIT03";
const LITERALS_MAGIC: &[u8; 8] = b"HXLITS03";
const EDGES_MAGIC: &[u8; 8] = b"HXEDGE04";
const CALLEE_PRINTS_MAGIC: &[u8; 8] = b"HXCLPR01";

/// A unit hash's key: its 16 lowercase hex digits as a number. Any other
/// spelling is no key (no indexed row can hold it: the encoder refuses
/// one).
pub fn unit_key(hash: &str) -> Option<u64> {
    (hash.len() == 16 && hash.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')))
        .then(|| u64::from_str_radix(hash, 16).ok())
        .flatten()
}

/// A literal's key: FNV-1a over its UTF-8 bytes. Not an identity — the
/// reader confirms every key hit against the stored string.
pub fn literal_key(literal: &str) -> u64 {
    literal.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// Every owner of `units` (any slice), of the OUT-OF callees of `edges`
/// and of `literals`, sorted, once.
pub fn package_table(
    units: &[&[UnitRow]],
    edges: &[&[EdgeRow]],
    literals: &[LiteralRow],
) -> Vec<String> {
    let owners: BTreeSet<&str> = units
        .iter()
        .flat_map(|rows| rows.iter().map(|r| r.owner.as_str()))
        .chain(
            edges
                .iter()
                .flat_map(|rows| rows.iter().flat_map(|r| r.out_of.iter()))
                .map(|c| c.owner.as_str()),
        )
        .chain(literals.iter().map(|r| r.owner.as_str()))
        .collect();
    owners.into_iter().map(str::to_string).collect()
}

/// The lookup files of an index (name → bytes): the package table, one
/// unit table per slice, the literal table, and with `edges` (format
/// `/4`) one edge table and one callee-print table per unit slice (a
/// slice `edges` lacks records no callee); None writes no edge table
/// (`/3`).
pub fn lookup_files(
    units: &BTreeMap<String, Vec<UnitRow>>,
    edges: Option<&BTreeMap<String, Vec<EdgeRow>>>,
    literals: &[LiteralRow],
) -> Result<BTreeMap<String, Vec<u8>>, String> {
    let slices: Vec<&[UnitRow]> = units.values().map(Vec::as_slice).collect();
    let edge_slices: Vec<&[EdgeRow]> = edges
        .into_iter()
        .flat_map(|e| e.values().map(Vec::as_slice))
        .collect();
    let packages = package_table(&slices, &edge_slices, literals);
    let mut files = BTreeMap::new();
    for (slice, rows) in units {
        files.insert(units_file(slice), UnitTable::encode(rows, &packages)?);
        if let Some(edges) = edges {
            let edge_rows = edges.get(slice).map_or(&[][..], Vec::as_slice);
            files.insert(
                edges_file(slice),
                EdgeTable::encode(rows, edge_rows, &packages)?,
            );
            files.insert(
                callee_prints_file(slice),
                CalleePrintTable::encode(edge_rows, &packages)?,
            );
        }
    }
    files.insert(
        LITERALS_FILE.to_string(),
        LiteralTable::encode(literals, &packages),
    );
    let mut names = serde_json::to_vec(&packages).map_err(|e| e.to_string())?;
    names.push(b'\n');
    files.insert(PACKAGES_FILE.to_string(), names);
    Ok(files)
}

fn owner_id(packages: &[String], owner: &str) -> u32 {
    packages
        .binary_search_by(|p| p.as_str().cmp(owner))
        .map(|i| i as u32)
        .unwrap_or_else(|_| panic!("owner {owner} is not in the package table"))
}

// ------------------------------------------------------------------ bytes

/// Where a table's bytes are: a file read by position through a page
/// cache of the pages this run touched (a run), or memory (an index built
/// from rows, the tests'). One reader for both.
pub enum Source {
    File(PagedFile),
    Memory(Vec<u8>),
}

/// The bytes a table read moves at once, and the unit the run's own cache
/// keeps: a lookup's reads cluster (a key block, its offsets, its rows), so
/// most of them are answered without a system call.
const PAGE: u64 = 4096;

/// A table file and the pages of it this run has read. The cache holds
/// only what a lookup touched — bounded by the file, in practice by the
/// bundle's own vocabulary — never the table. Single-threaded: the lookup
/// runs on one thread (a `RefCell`, not a lock).
pub struct PagedFile {
    file: std::fs::File,
    len: u64,
    pages: std::cell::RefCell<HashMap<u64, Box<[u8]>>>,
}

impl PagedFile {
    fn page(&self, p: u64, mut use_bytes: impl FnMut(&[u8])) {
        let mut pages = self.pages.borrow_mut();
        let bytes = pages.entry(p).or_insert_with(|| {
            let start = p * PAGE;
            let mut buf = vec![0; PAGE.min(self.len - start) as usize];
            read_at(&self.file, start, &mut buf)
                .unwrap_or_else(|e| panic!("library index table read at {start}: {e}"));
            buf.into_boxed_slice()
        });
        use_bytes(bytes);
    }
}

impl Source {
    pub fn open(path: &Path) -> Result<Source, String> {
        let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let len = file
            .metadata()
            .map_err(|e| format!("{}: {e}", path.display()))?
            .len();
        Ok(Source::File(PagedFile {
            file,
            len,
            pages: Default::default(),
        }))
    }

    fn len(&self) -> u64 {
        match self {
            Source::File(f) => f.len,
            Source::Memory(b) => b.len() as u64,
        }
    }

    /// `n` bytes at `at`. The table's size was checked against its header
    /// when it was opened, so a failed read is the file changing under a
    /// run: that run cannot answer, and stops.
    fn bytes(&self, at: u64, n: usize) -> Vec<u8> {
        match self {
            Source::Memory(b) => b[at as usize..at as usize + n].to_vec(),
            Source::File(f) => {
                let mut out = Vec::with_capacity(n);
                let end = at + n as u64;
                let mut pos = at;
                while pos < end {
                    let p = pos / PAGE;
                    let (from, to) = (
                        (pos - p * PAGE) as usize,
                        (end.min((p + 1) * PAGE) - p * PAGE) as usize,
                    );
                    f.page(p, |bytes| out.extend_from_slice(&bytes[from..to]));
                    pos = p * PAGE + to as u64;
                }
                out
            }
        }
    }

    fn u64s(&self, at: u64, n: usize) -> Vec<u64> {
        self.bytes(at, n * 8)
            .as_chunks::<8>()
            .0
            .iter()
            .map(|c| u64::from_le_bytes(*c))
            .collect()
    }

    fn u32s(&self, at: u64, n: usize) -> Vec<u32> {
        self.bytes(at, n * 4)
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| u32::from_le_bytes(*c))
            .collect()
    }
}
#[cfg(unix)]
fn read_at(file: &std::fs::File, at: u64, buf: &mut [u8]) -> std::io::Result<()> {
    std::os::unix::fs::FileExt::read_exact_at(file, buf, at)
}

#[cfg(windows)]
fn read_at(file: &std::fs::File, mut at: u64, mut buf: &mut [u8]) -> std::io::Result<()> {
    while !buf.is_empty() {
        let n = std::os::windows::fs::FileExt::seek_read(file, buf, at)?;
        if n == 0 {
            return Err(std::io::ErrorKind::UnexpectedEof.into());
        }
        at += n as u64;
        buf = &mut buf[n..];
    }
    Ok(())
}

/// A little-endian writer.
#[derive(Default)]
struct Out(Vec<u8>);

impl Out {
    fn u64(&mut self, v: u64) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn u64s(&mut self, vs: impl IntoIterator<Item = u64>) {
        vs.into_iter().for_each(|v| self.u64(v));
    }
    fn u32s(&mut self, vs: impl IntoIterator<Item = u32>) {
        vs.into_iter().for_each(|v| self.u32(v));
    }
}

/// Offsets (n + 1 of them) of `lens` laid end to end.
fn offsets(lens: impl IntoIterator<Item = usize>) -> Vec<u64> {
    let mut at = 0u64;
    let mut out = vec![0];
    for n in lens {
        at += n as u64;
        out.push(at);
    }
    out
}

// ------------------------------------------------------------ key columns

/// A sorted key column as written: its fence, then its keys.
pub struct KeyColumnBytes {
    pub bytes: Vec<u8>,
    n: u64,
}

impl KeyColumnBytes {
    /// `keys` must be sorted (duplicates allowed).
    pub fn encode(keys: &[u64]) -> KeyColumnBytes {
        let mut out = Out::default();
        out.u64s(keys.iter().step_by(BLOCK).copied());
        out.u64s(keys.iter().copied());
        KeyColumnBytes {
            bytes: out.0,
            n: keys.len() as u64,
        }
    }

    /// The column at byte `at` of `src` (the tests' entry).
    pub fn open(&self, src: &Source, at: u64) -> Result<KeyColumn, String> {
        KeyColumn::open(src, at, self.n)
    }
}

fn fence_len(n: u64) -> u64 {
    n.div_ceil(BLOCK as u64)
}

/// A sorted key column, by its fence: the only part a process holds.
pub struct KeyColumn {
    fence: Vec<u64>,
    keys_at: u64,
    n: u64,
}

impl KeyColumn {
    fn bytes(n: u64) -> u64 {
        (fence_len(n) + n) * 8
    }

    fn open(src: &Source, at: u64, n: u64) -> Result<KeyColumn, String> {
        let f = fence_len(n);
        if at + Self::bytes(n) > src.len() {
            return Err(format!(
                "a key column of {n} keys at byte {at} runs past the table's {} bytes",
                src.len()
            ));
        }
        Ok(KeyColumn {
            fence: src.u64s(at, f as usize),
            keys_at: at + f * 8,
            n,
        })
    }

    fn block(&self, src: &Source, b: usize) -> Vec<u64> {
        let start = (b * BLOCK) as u64;
        let len = (self.n - start).min(BLOCK as u64) as usize;
        src.u64s(self.keys_at + start * 8, len)
    }

    /// The positions holding `key` (empty: none).
    pub fn equal_range(&self, src: &Source, key: u64) -> Range<u64> {
        let blocks = self.fence.len();
        if blocks == 0 {
            return 0..0;
        }
        // the first key >= `key` is in the block before the first fence
        // >= key, or at that fence
        let mut b = self.fence.partition_point(|&f| f < key).saturating_sub(1);
        let mut keys = self.block(src, b);
        let mut i = keys.partition_point(|&k| k < key);
        if i == keys.len() {
            b += 1;
            if b == blocks {
                return self.n..self.n;
            }
            keys = self.block(src, b);
            i = 0;
        }
        let lo = (b * BLOCK + i) as u64;
        let mut hi = lo;
        loop {
            let run = keys[i..].iter().take_while(|&&k| k == key).count();
            hi += run as u64;
            if i + run < keys.len() || b + 1 == blocks {
                return lo..hi;
            }
            b += 1;
            keys = self.block(src, b);
            i = 0;
        }
    }
}

fn check_size(what: &str, src: &Source, want: u64) -> Result<(), String> {
    if src.len() == want {
        Ok(())
    } else {
        Err(format!(
            "{what} is {} bytes, its header says {want} bytes: a damaged or partly written table",
            src.len()
        ))
    }
}

fn header(src: &Source, magic: &[u8; 8], what: &str, n: usize) -> Result<Vec<u64>, String> {
    let need = 8 + 8 * n as u64;
    if src.len() < need || src.bytes(0, 8) != magic {
        return Err(format!(
            "{what}: not a {} table",
            String::from_utf8_lossy(magic)
        ));
    }
    Ok(src.u64s(8, n))
}

// ----------------------------------------------------------- unit table

/// `units.<slice>.bin`: header (magic, keys, rows), the key column, row
/// starts (keys + 1), rows (owner u32, module bytes u64).
pub struct UnitTable {
    src: Source,
    keys: KeyColumn,
    starts_at: u64,
    rows_at: u64,
}

const UNIT_ROW: u64 = 12;

/// Each row's position in the unit table's rows ([`UnitTable::encode`]'s
/// order: by key, a key's rows in their order in `rows`) — the edge
/// table's key.
pub fn unit_row_ids(rows: &[UnitRow]) -> Result<Vec<u32>, String> {
    let mut keyed: Vec<(u64, usize)> = Vec::with_capacity(rows.len());
    for (i, r) in rows.iter().enumerate() {
        let key = unit_key(&r.hash)
            .ok_or_else(|| format!("unit row hash {:?} is not 16 lowercase hex digits", r.hash))?;
        keyed.push((key, i));
    }
    keyed.sort_unstable();
    let mut ids = vec![0u32; rows.len()];
    for (pos, &(_, i)) in keyed.iter().enumerate() {
        ids[i] = pos as u32;
    }
    Ok(ids)
}

impl UnitTable {
    /// `rows` in any order; a key's rows keep their order in `rows`.
    pub fn encode(rows: &[UnitRow], packages: &[String]) -> Result<Vec<u8>, String> {
        let mut by_key: BTreeMap<u64, Vec<(u32, u64)>> = BTreeMap::new();
        for r in rows {
            let key = unit_key(&r.hash).ok_or_else(|| {
                format!("unit row hash {:?} is not 16 lowercase hex digits", r.hash)
            })?;
            by_key
                .entry(key)
                .or_default()
                .push((owner_id(packages, &r.owner), r.module_bytes));
        }
        let keys: Vec<u64> = by_key.keys().copied().collect();
        let mut out = Out::default();
        out.0.extend_from_slice(UNITS_MAGIC);
        out.u64s([keys.len() as u64, rows.len() as u64]);
        out.0.extend(KeyColumnBytes::encode(&keys).bytes);
        out.u64s(offsets(by_key.values().map(Vec::len)));
        for (owner, bytes) in by_key.into_values().flatten() {
            out.u32(owner);
            out.u64(bytes);
        }
        Ok(out.0)
    }

    pub fn open(src: Source) -> Result<UnitTable, String> {
        let h = header(&src, UNITS_MAGIC, "unit table", 2)?;
        let (n_keys, n_rows) = (h[0], h[1]);
        let keys_at = 24;
        let starts_at = keys_at + KeyColumn::bytes(n_keys);
        let rows_at = starts_at + (n_keys + 1) * 8;
        check_size("unit table", &src, rows_at + n_rows * UNIT_ROW)?;
        Ok(UnitTable {
            keys: KeyColumn::open(&src, keys_at, n_keys)?,
            src,
            starts_at,
            rows_at,
        })
    }

    /// Every indexed occurrence of `hash`, in row order.
    pub fn hits(&self, hash: &str) -> Vec<UnitHit> {
        let Some(key) = unit_key(hash) else {
            return Vec::new();
        };
        let r = self.keys.equal_range(&self.src, key);
        if r.is_empty() {
            return Vec::new();
        }
        let ends = self.src.u64s(self.starts_at + r.start * 8, 2);
        let n = (ends[1] - ends[0]) as usize;
        self.src
            .bytes(self.rows_at + ends[0] * UNIT_ROW, n * UNIT_ROW as usize)
            .as_chunks::<{ UNIT_ROW as usize }>()
            .0
            .iter()
            .enumerate()
            .map(|(i, c)| UnitHit {
                owner: u32::from_le_bytes(c[..4].try_into().expect("4 bytes")),
                module_bytes: u64::from_le_bytes(c[4..].try_into().expect("8 bytes")),
                row: (ends[0] + i as u64) as u32,
                table: 0,
            })
            .collect()
    }
}

// ----------------------------------------------------------- edge table

/// One recorded callee: its owner's id and its print's unit key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EdgeCallee {
    pub owner: u32,
    pub key: u64,
}

/// `edges.<slice>.bin` (format `/4`): header (magic, keyed rows, lists,
/// callees), the key column (unit row ids, [`unit_row_ids`], ascending),
/// per keyed row its list id (u32), list offsets (lists + 1), then the
/// callees (owner u32, key u64), sorted per list. A list is stored once
/// however many rows hold it (a statement's function rows share its list).
pub struct EdgeTable {
    src: Source,
    keys: KeyColumn,
    lists_at: u64,
    offsets_at: u64,
    callees_at: u64,
}

const EDGE_CALLEE: u64 = 12;

impl EdgeTable {
    /// `edges` keyed by (hash, owner, file) onto `units` (the slice's unit
    /// rows, any order); an edge row with no unit row is refused.
    pub fn encode(
        units: &[UnitRow],
        edges: &[EdgeRow],
        packages: &[String],
    ) -> Result<Vec<u8>, String> {
        let ids = unit_row_ids(units)?;
        let row_of: HashMap<(&str, &str, &str), u32> = units
            .iter()
            .zip(&ids)
            .map(|(r, &id)| ((r.hash.as_str(), r.owner.as_str(), r.file.as_str()), id))
            .collect();
        let mut keyed: Vec<(u32, Vec<EdgeCallee>)> = Vec::with_capacity(edges.len());
        for e in edges {
            let row = *row_of
                .get(&(e.hash.as_str(), e.owner.as_str(), e.file.as_str()))
                .ok_or_else(|| {
                    format!("edge row {} {} {} has no unit row", e.hash, e.owner, e.file)
                })?;
            let own = owner_id(packages, &e.owner);
            let key = |h: &str| {
                unit_key(h)
                    .ok_or_else(|| format!("callee hash {h:?} is not 16 lowercase hex digits"))
            };
            let mut callees: Vec<EdgeCallee> = Vec::new();
            for h in &e.inside {
                callees.push(EdgeCallee {
                    owner: own,
                    key: key(h)?,
                });
            }
            for c in &e.out_of {
                callees.push(EdgeCallee {
                    owner: owner_id(packages, &c.owner),
                    key: key(&c.hash)?,
                });
            }
            callees.sort_unstable();
            callees.dedup();
            if !callees.is_empty() {
                keyed.push((row, callees));
            }
        }
        keyed.sort_unstable_by_key(|(row, _)| *row);
        if keyed.windows(2).any(|w| w[0].0 == w[1].0) {
            return Err("two edge rows for one unit row".into());
        }
        let mut list_ids: HashMap<&[EdgeCallee], u32> = HashMap::new();
        let mut lists: Vec<&[EdgeCallee]> = Vec::new();
        let mut row_lists: Vec<u32> = Vec::with_capacity(keyed.len());
        for (_, callees) in &keyed {
            let id = *list_ids.entry(callees.as_slice()).or_insert_with(|| {
                lists.push(callees.as_slice());
                (lists.len() - 1) as u32
            });
            row_lists.push(id);
        }
        let n_callees: usize = lists.iter().map(|l| l.len()).sum();
        let mut out = Out::default();
        out.0.extend_from_slice(EDGES_MAGIC);
        out.u64s([keyed.len() as u64, lists.len() as u64, n_callees as u64]);
        let keys: Vec<u64> = keyed.iter().map(|(row, _)| u64::from(*row)).collect();
        out.0.extend(KeyColumnBytes::encode(&keys).bytes);
        out.u32s(row_lists);
        out.u64s(offsets(lists.iter().map(|l| l.len())));
        for c in lists.iter().flat_map(|l| l.iter()) {
            out.u32(c.owner);
            out.u64(c.key);
        }
        Ok(out.0)
    }

    pub fn open(src: Source) -> Result<EdgeTable, String> {
        let h = header(&src, EDGES_MAGIC, "edge table", 3)?;
        let (n_rows, n_lists, n_callees) = (h[0], h[1], h[2]);
        let keys_at = 8 + 3 * 8;
        let lists_at = keys_at + KeyColumn::bytes(n_rows);
        let offsets_at = lists_at + n_rows * 4;
        let callees_at = offsets_at + (n_lists + 1) * 8;
        check_size("edge table", &src, callees_at + n_callees * EDGE_CALLEE)?;
        Ok(EdgeTable {
            keys: KeyColumn::open(&src, keys_at, n_rows)?,
            src,
            lists_at,
            offsets_at,
            callees_at,
        })
    }

    /// Unit row `row`'s recorded callees, sorted by (owner, key); empty:
    /// none recorded (the index records every row's edges, so none).
    pub fn callees(&self, row: u32) -> Vec<EdgeCallee> {
        let r = self.keys.equal_range(&self.src, u64::from(row));
        if r.is_empty() {
            return Vec::new();
        }
        let list = u64::from(self.src.u32s(self.lists_at + r.start * 4, 1)[0]);
        let ends = self.src.u64s(self.offsets_at + list * 8, 2);
        let n = (ends[1] - ends[0]) as usize;
        self.src
            .bytes(
                self.callees_at + ends[0] * EDGE_CALLEE,
                n * EDGE_CALLEE as usize,
            )
            .as_chunks::<{ EDGE_CALLEE as usize }>()
            .0
            .iter()
            .map(|c| EdgeCallee {
                owner: u32::from_le_bytes(c[..4].try_into().expect("4 bytes")),
                key: u64::from_le_bytes(c[4..].try_into().expect("8 bytes")),
            })
            .collect()
    }
}

// ------------------------------------------------------- callee prints

/// `callee-prints.<slice>.bin` (format `/4`, edges/2): header (magic,
/// prints, recorders), the key column (every distinct callee print any edge
/// row records, ascending), per print its recorder offsets (prints + 1),
/// then the recorders (owner ids, ascending per print, once each). Derived
/// entirely from the slice's edge rows: a print the slice records as a
/// callee is queryable here EVEN WHEN no unit row holds it (under the
/// 20-node row floor, a stop-listed helper, or a row dropped for evidence).
pub struct CalleePrintTable {
    src: Source,
    keys: KeyColumn,
    offsets_at: u64,
    recorders_at: u64,
}

impl CalleePrintTable {
    /// `edges` in any order: every INSIDE print credited to its row's
    /// owner, every OUT-OF print to the owner the row records for it,
    /// sorted per print and once each.
    pub fn encode(edges: &[EdgeRow], packages: &[String]) -> Result<Vec<u8>, String> {
        let mut by_key: BTreeMap<u64, BTreeSet<u32>> = BTreeMap::new();
        for e in edges {
            let own = owner_id(packages, &e.owner);
            for h in &e.inside {
                let key = unit_key(h)
                    .ok_or_else(|| format!("callee hash {h:?} is not 16 lowercase hex digits"))?;
                by_key.entry(key).or_default().insert(own);
            }
            for c in &e.out_of {
                let key = unit_key(&c.hash).ok_or_else(|| {
                    format!("callee hash {:?} is not 16 lowercase hex digits", c.hash)
                })?;
                by_key
                    .entry(key)
                    .or_default()
                    .insert(owner_id(packages, &c.owner));
            }
        }
        let keys: Vec<u64> = by_key.keys().copied().collect();
        let owners: Vec<u32> = by_key.values().flatten().copied().collect();
        let mut out = Out::default();
        out.0.extend(CALLEE_PRINTS_MAGIC);
        out.u64s([keys.len() as u64, owners.len() as u64]);
        out.0.extend(KeyColumnBytes::encode(&keys).bytes);
        out.u64s(offsets(by_key.values().map(BTreeSet::len)));
        out.u32s(owners);
        Ok(out.0)
    }

    pub fn open(src: Source) -> Result<CalleePrintTable, String> {
        let h = header(&src, CALLEE_PRINTS_MAGIC, "callee-print table", 2)?;
        let (n_keys, n_owners) = (h[0], h[1]);
        let keys_at = 8 + 2 * 8;
        let offsets_at = keys_at + KeyColumn::bytes(n_keys);
        let recorders_at = offsets_at + (n_keys + 1) * 8;
        check_size("callee-print table", &src, recorders_at + n_owners * 4)?;
        Ok(CalleePrintTable {
            keys: KeyColumn::open(&src, keys_at, n_keys)?,
            src,
            offsets_at,
            recorders_at,
        })
    }

    /// The owners whose rows record callee print `key`, ascending; empty:
    /// no row of the slice records it.
    pub fn recorders(&self, key: u64) -> Vec<u32> {
        let r = self.keys.equal_range(&self.src, key);
        if r.is_empty() {
            return Vec::new();
        }
        let ends = self.src.u64s(self.offsets_at + r.start * 8, 2);
        self.src.u32s(
            self.recorders_at + ends[0] * 4,
            (ends[1] - ends[0]) as usize,
        )
    }
}

// -------------------------------------------------------- literal table

/// `literals.bin`: header (magic, literals, files, postings, file
/// literals, literal packages), the key column, per literal its package
/// count (u32), string offsets and posting offsets (literals + 1 each),
/// per file its owner (u32) and literal offsets (files + 1), then the
/// string bytes, the postings (file ids, u32) and the files' literal ids
/// (u32, sorted per file).
pub struct LiteralTable {
    src: Source,
    key: fn(&str) -> u64,
    keys: KeyColumn,
    literal_packages: u32,
    df_at: u64,
    str_off_at: u64,
    post_off_at: u64,
    owner_at: u64,
    file_off_at: u64,
    strings_at: u64,
    postings_at: u64,
    file_lits_at: u64,
}

/// The encoder's per-literal facts, in id order.
struct LiteralColumns<'r> {
    keys: Vec<u64>,
    strings: Vec<&'r str>,
    postings: Vec<Vec<u32>>,
    df: Vec<u32>,
    file_lits: Vec<Vec<u32>>,
}

fn literal_columns<'r>(rows: &'r [LiteralRow], key: fn(&str) -> u64) -> LiteralColumns<'r> {
    let distinct: BTreeSet<(u64, &str)> = rows
        .iter()
        .flat_map(|r| r.lits.iter().map(|l| (key(l), l.as_str())))
        .collect();
    let ids: HashMap<&str, u32> = distinct
        .iter()
        .enumerate()
        .map(|(i, &(_, l))| (l, i as u32))
        .collect();
    let mut postings: Vec<Vec<u32>> = vec![Vec::new(); distinct.len()];
    let mut file_lits = Vec::with_capacity(rows.len());
    for (f, r) in rows.iter().enumerate() {
        let mut held: Vec<u32> = r.lits.iter().map(|l| ids[l.as_str()]).collect();
        held.sort_unstable();
        held.dedup();
        for &id in &held {
            postings[id as usize].push(f as u32);
        }
        file_lits.push(held);
    }
    let df = postings
        .iter()
        .map(|files| {
            files
                .iter()
                .map(|&f| rows[f as usize].owner.as_str())
                .collect::<BTreeSet<_>>()
                .len() as u32
        })
        .collect();
    LiteralColumns {
        keys: distinct.iter().map(|&(k, _)| k).collect(),
        strings: distinct.into_iter().map(|(_, l)| l).collect(),
        postings,
        df,
        file_lits,
    }
}

impl LiteralTable {
    /// `rows` in file order (a file's id is its row's position).
    pub fn encode(rows: &[LiteralRow], packages: &[String]) -> Vec<u8> {
        Self::encode_keyed(rows, packages, literal_key)
    }

    /// [`LiteralTable::encode`] under another key function (the tests'
    /// way to force key collisions).
    pub(crate) fn encode_keyed(
        rows: &[LiteralRow],
        packages: &[String],
        key: fn(&str) -> u64,
    ) -> Vec<u8> {
        let c = literal_columns(rows, key);
        let owners: BTreeSet<&str> = rows.iter().map(|r| r.owner.as_str()).collect();
        let n_post: usize = c.postings.iter().map(Vec::len).sum();
        let n_file_lits: usize = c.file_lits.iter().map(Vec::len).sum();
        let mut out = Out::default();
        out.0.extend_from_slice(LITERALS_MAGIC);
        out.u64s([
            c.keys.len() as u64,
            rows.len() as u64,
            n_post as u64,
            n_file_lits as u64,
            owners.len() as u64,
        ]);
        out.0.extend(KeyColumnBytes::encode(&c.keys).bytes);
        out.u32s(c.df);
        out.u64s(offsets(c.strings.iter().map(|s| s.len())));
        out.u64s(offsets(c.postings.iter().map(Vec::len)));
        out.u32s(rows.iter().map(|r| owner_id(packages, &r.owner)));
        out.u64s(offsets(c.file_lits.iter().map(Vec::len)));
        for s in &c.strings {
            out.0.extend_from_slice(s.as_bytes());
        }
        out.u32s(c.postings.into_iter().flatten());
        out.u32s(c.file_lits.into_iter().flatten());
        out.0
    }

    pub fn open(src: Source) -> Result<LiteralTable, String> {
        Self::open_keyed(src, literal_key)
    }

    /// [`LiteralTable::open`] under the key function the table was
    /// encoded with.
    pub(crate) fn open_keyed(src: Source, key: fn(&str) -> u64) -> Result<LiteralTable, String> {
        let h = header(&src, LITERALS_MAGIC, "literal table", 5)?;
        let (n_lits, n_files, n_post, n_file_lits) = (h[0], h[1], h[2], h[3]);
        let literal_packages =
            u32::try_from(h[4]).map_err(|_| "literal table: package count".to_string())?;
        let keys_at = 8 + 5 * 8;
        let df_at = keys_at + KeyColumn::bytes(n_lits);
        let str_off_at = df_at + n_lits * 4;
        let post_off_at = str_off_at + (n_lits + 1) * 8;
        let owner_at = post_off_at + (n_lits + 1) * 8;
        let file_off_at = owner_at + n_files * 4;
        let strings_at = file_off_at + (n_files + 1) * 8;
        if strings_at > src.len() {
            return Err(format!(
                "literal table is {} bytes, shorter than its header says",
                src.len()
            ));
        }
        let n_string_bytes = src.u64s(str_off_at + n_lits * 8, 1)[0];
        let postings_at = strings_at + n_string_bytes;
        let file_lits_at = postings_at + n_post * 4;
        check_size("literal table", &src, file_lits_at + n_file_lits * 4)?;
        Ok(LiteralTable {
            keys: KeyColumn::open(&src, keys_at, n_lits)?,
            src,
            key,
            literal_packages,
            df_at,
            str_off_at,
            post_off_at,
            owner_at,
            file_off_at,
            strings_at,
            postings_at,
            file_lits_at,
        })
    }

    /// How many packages hold any literal.
    pub fn literal_packages(&self) -> u32 {
        self.literal_packages
    }

    /// `[start, end)` of entry `i` in an offsets column at `at`.
    fn span(&self, at: u64, i: u32) -> (u64, u64) {
        let v = self.src.u64s(at + u64::from(i) * 8, 2);
        (v[0], v[1])
    }

    /// The literal's id, when the index holds it.
    pub fn resolve(&self, literal: &str) -> Option<u32> {
        self.keys
            .equal_range(&self.src, (self.key)(literal))
            .map(|id| id as u32)
            .find(|&id| {
                let (s, e) = self.span(self.str_off_at, id);
                e - s == literal.len() as u64
                    && self.src.bytes(self.strings_at + s, (e - s) as usize) == literal.as_bytes()
            })
    }

    /// How many packages hold literal `id`.
    pub fn package_df(&self, id: u32) -> u32 {
        self.src.u32s(self.df_at + u64::from(id) * 4, 1)[0]
    }

    /// The files holding literal `id`, ascending.
    pub fn postings(&self, id: u32) -> Vec<u32> {
        let (s, e) = self.span(self.post_off_at, id);
        self.src.u32s(self.postings_at + s * 4, (e - s) as usize)
    }

    /// File `f`'s owner id.
    pub fn file_owner(&self, f: u32) -> u32 {
        self.src.u32s(self.owner_at + u64::from(f) * 4, 1)[0]
    }

    /// File `f`'s literal ids, ascending.
    pub fn file_literals(&self, f: u32) -> Vec<u32> {
        let (s, e) = self.span(self.file_off_at, f);
        self.src.u32s(self.file_lits_at + s * 4, (e - s) as usize)
    }
}
