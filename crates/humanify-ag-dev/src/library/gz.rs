//! gzip exactly as Python's `gzip.GzipFile(fileobj=open(path, "wb"),
//! mode="wb", mtime=0)` writes it (lib096 `write_gz_json` and the layer
//! builders): header `1f 8b 08`, FNAME set to the file's basename minus
//! `.gz`, mtime 0, XFL 2 (level 9), OS 255; a raw deflate stream from the
//! SYSTEM zlib at level 9 (window 15, memLevel 8, the same library
//! Python links); CRC-32 and size. Equal content is equal bytes, so a
//! frozen layer's sha can be reproduced.

use std::io::Write;
use std::path::Path;

use flate2::Compression;
use flate2::write::DeflateEncoder;

/// The gzip bytes Python writes for `data` at `path`.
pub fn gzip_bytes(path: &Path, data: &[u8]) -> Result<Vec<u8>, String> {
    let base = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let fname = base.strip_suffix(".gz").unwrap_or(&base);
    let mut out = vec![0x1f, 0x8b, 0x08];
    out.push(if fname.is_empty() { 0 } else { 0x08 });
    out.extend_from_slice(&[0, 0, 0, 0, 0x02, 0xff]);
    if !fname.is_empty() {
        out.extend_from_slice(fname.as_bytes());
        out.push(0);
    }
    let mut enc = DeflateEncoder::new(out, Compression::new(9));
    enc.write_all(data).map_err(|e| e.to_string())?;
    let mut out = enc.finish().map_err(|e| e.to_string())?;
    let mut crc = flate2::Crc::new();
    crc.update(data);
    out.extend_from_slice(&crc.sum().to_le_bytes());
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    Ok(out)
}

/// Write `data` gzipped to `path` (directories created).
pub fn write_gz(path: &Path, data: &[u8]) -> Result<(), String> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
    }
    std::fs::write(path, gzip_bytes(path, data)?).map_err(|e| format!("{}: {e}", path.display()))
}

/// The decompressed bytes of a gzip file.
pub fn read_gz(path: &Path) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut out = Vec::new();
    flate2::read::GzDecoder::new(&bytes[..])
        .read_to_end(&mut out)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(out)
}

#[cfg(test)]
#[path = "gz_test.rs"]
mod gz_test;
