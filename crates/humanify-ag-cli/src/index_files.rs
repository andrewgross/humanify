//! The shell's side of core's library-index reads
//! (`humanify_ag_core::libindex::files`): a directory of index files, and
//! each table opened as a file read by position, so core reads only the
//! pages a lookup touches and never a whole table. Also the text reader
//! library detection reads unpacked files through.

use std::path::{Path, PathBuf};

use humanify_ag_core::libindex::files::{IndexFiles, ReadAt};

/// An index directory on disk.
pub struct IndexDir(pub PathBuf);

impl IndexDir {
    /// `dir`'s files, boxed for `LibraryIndex::load`.
    pub fn boxed(dir: &Path) -> Box<dyn IndexFiles> {
        Box::new(IndexDir(dir.to_path_buf()))
    }
}

impl IndexFiles for IndexDir {
    fn read_text(&self, name: &str) -> std::io::Result<String> {
        std::fs::read_to_string(self.0.join(name))
    }

    fn open(&self, name: &str) -> std::io::Result<Box<dyn ReadAt>> {
        Ok(Box::new(FileReadAt::open(&self.0.join(name))?))
    }
}

/// A file read by position (`pread`). Its length is read once, at open.
pub struct FileReadAt {
    file: std::fs::File,
    len: u64,
}

impl FileReadAt {
    /// Open `path`; an error names the path.
    pub fn open(path: &Path) -> std::io::Result<FileReadAt> {
        let named =
            |e: std::io::Error| std::io::Error::new(e.kind(), format!("{}: {e}", path.display()));
        let file = std::fs::File::open(path).map_err(named)?;
        let len = file.metadata().map_err(named)?.len();
        Ok(FileReadAt { file, len })
    }
}

impl ReadAt for FileReadAt {
    fn size(&self) -> u64 {
        self.len
    }

    #[cfg(unix)]
    fn read_exact_at(&self, at: u64, buf: &mut [u8]) -> std::io::Result<()> {
        std::os::unix::fs::FileExt::read_exact_at(&self.file, buf, at)
    }

    #[cfg(windows)]
    fn read_exact_at(&self, mut at: u64, mut buf: &mut [u8]) -> std::io::Result<()> {
        while !buf.is_empty() {
            let n = std::os::windows::fs::FileExt::seek_read(&self.file, buf, at)?;
            if n == 0 {
                return Err(std::io::ErrorKind::UnexpectedEof.into());
            }
            at += n as u64;
            buf = &mut buf[n..];
        }
        Ok(())
    }
}

/// `fs.readFile(path, "utf-8")`: invalid bytes become U+FFFD (library
/// detection's reader, `libdetect::ReadText`).
pub fn read_utf8(path: &Path) -> std::io::Result<String> {
    std::fs::read(path).map(|b| String::from_utf8_lossy(&b).into_owned())
}
