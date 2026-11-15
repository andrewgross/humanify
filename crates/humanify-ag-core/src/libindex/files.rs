//! How core reaches a library index's files without touching the disk:
//! the shell hands [`LibraryIndex::load`](super::lookup::LibraryIndex::load)
//! an [`IndexFiles`] (the cli's is file-backed), and core asks it for a
//! small file's text or a table's [`ReadAt`] reader by file name.
//!
//! The tables stay lazily read: [`super::table::Source`] keeps its own page
//! cache over a [`ReadAt`] and asks only for the pages a lookup touches,
//! not the whole table. [`MemoryIndexFiles`] is the in-memory set (an
//! index built from rows, the tests').

use std::collections::BTreeMap;
use std::io::{Error, ErrorKind};

/// A table file's bytes, read by position. `size` is its byte length when
/// it was opened; a read past it, or a file that changed under the run,
/// is an error.
pub trait ReadAt: Send + Sync {
    fn size(&self) -> u64;
    fn read_exact_at(&self, at: u64, buf: &mut [u8]) -> std::io::Result<()>;
}

/// The files of one library index, by file name (`manifest.json`,
/// `units.<slice>.bin`, `modules.<slice>.jsonl`). An error's text is the
/// whole reason (the file-backed set names the path); the caller prefixes
/// the file name.
pub trait IndexFiles: Send + Sync {
    /// A small file, whole, as UTF-8 text.
    fn read_text(&self, name: &str) -> std::io::Result<String>;
    /// A file to read by position: a table, or a row file streamed once.
    fn open(&self, name: &str) -> std::io::Result<Box<dyn ReadAt>>;
}

impl ReadAt for Vec<u8> {
    fn size(&self) -> u64 {
        self.len() as u64
    }

    fn read_exact_at(&self, at: u64, buf: &mut [u8]) -> std::io::Result<()> {
        let end = at.checked_add(buf.len() as u64);
        let bytes = end
            .filter(|end| *end <= self.size())
            .and_then(|end| self.get(at as usize..end as usize))
            .ok_or_else(|| Error::from(ErrorKind::UnexpectedEof))?;
        buf.copy_from_slice(bytes);
        Ok(())
    }
}

/// An index's files held in memory, by name. A name it lacks is
/// `NotFound`, exactly as a missing file is: an index built from rows has
/// no files, so a names-mode answers read over it refuses.
#[derive(Default)]
pub struct MemoryIndexFiles(pub BTreeMap<String, Vec<u8>>);

impl MemoryIndexFiles {
    fn get(&self, name: &str) -> std::io::Result<&Vec<u8>> {
        self.0
            .get(name)
            .ok_or_else(|| Error::new(ErrorKind::NotFound, format!("no file {name}")))
    }
}

impl IndexFiles for MemoryIndexFiles {
    fn read_text(&self, name: &str) -> std::io::Result<String> {
        String::from_utf8(self.get(name)?.clone())
            .map_err(|e| Error::new(ErrorKind::InvalidData, e))
    }

    fn open(&self, name: &str) -> std::io::Result<Box<dyn ReadAt>> {
        Ok(Box::new(self.get(name)?.clone()))
    }
}

/// A [`ReadAt`] read front to back, as `std::io::Read` (a row file
/// streamed line by line, not held whole).
pub struct Sequential {
    reader: Box<dyn ReadAt>,
    at: u64,
}

impl Sequential {
    pub fn new(reader: Box<dyn ReadAt>) -> Sequential {
        Sequential { reader, at: 0 }
    }
}

impl std::io::Read for Sequential {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let left = self.reader.size().saturating_sub(self.at);
        let n = (buf.len() as u64).min(left) as usize;
        self.reader.read_exact_at(self.at, &mut buf[..n])?;
        self.at += n as u64;
        Ok(n)
    }
}
