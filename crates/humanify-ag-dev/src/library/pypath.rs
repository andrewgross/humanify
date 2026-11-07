//! Python's `os.path` string functions, where a layer's bytes depend on
//! them: `normpath`, `join`, `dirname`, `basename`, `relpath` (POSIX, on
//! absolute or relative strings, never touching the disk), and `os.walk`'s
//! traversal order (sorted, not following symlinked directories).

use std::path::{Path, PathBuf};

/// `os.path.normpath` (POSIX).
pub fn normpath(path: &str) -> String {
    if path.is_empty() {
        return ".".into();
    }
    let initial = if path.starts_with("//") && !path.starts_with("///") {
        2
    } else if path.starts_with('/') {
        1
    } else {
        0
    };
    let mut comps: Vec<&str> = Vec::new();
    for c in path.split('/') {
        if c.is_empty() || c == "." {
            continue;
        }
        if c != ".." || (initial == 0 && comps.is_empty()) || comps.last() == Some(&"..") {
            comps.push(c);
        } else if !comps.is_empty() {
            comps.pop();
        }
    }
    let body = comps.join("/");
    let out = format!("{}{body}", "/".repeat(initial));
    if out.is_empty() { ".".into() } else { out }
}

/// `os.path.join(a, b)`.
pub fn join(a: &str, b: &str) -> String {
    if b.starts_with('/') {
        b.to_string()
    } else if a.is_empty() || a.ends_with('/') {
        format!("{a}{b}")
    } else {
        format!("{a}/{b}")
    }
}

/// `os.path.dirname`.
pub fn dirname(p: &str) -> String {
    let i = p.rfind('/').map_or(0, |i| i + 1);
    let head = &p[..i];
    if !head.is_empty() && head != "/".repeat(head.len()) {
        head.trim_end_matches('/').to_string()
    } else {
        head.to_string()
    }
}

/// `os.path.basename`.
pub fn basename(p: &str) -> &str {
    p.rsplit('/').next().unwrap_or(p)
}

/// `os.path.relpath(path, start)` for absolute paths.
pub fn relpath(path: &str, start: &str) -> String {
    let split = |p: &str| -> Vec<String> {
        normpath(p)
            .split('/')
            .filter(|x| !x.is_empty())
            .map(String::from)
            .collect()
    };
    let (p, s) = (split(path), split(start));
    let common = p.iter().zip(&s).take_while(|(a, b)| a == b).count();
    let mut out: Vec<String> = vec!["..".to_string(); s.len() - common];
    out.extend(p[common..].iter().cloned());
    if out.is_empty() {
        ".".into()
    } else {
        out.join("/")
    }
}

/// `for root, dirs, files in os.walk(d): dirs.sort(); for f in
/// sorted(files)`: every file path, in that order. A symlinked directory
/// is listed as a directory and not descended into; unreadable
/// directories are skipped.
pub fn walk_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    let mut dirs: Vec<(std::ffi::OsString, bool)> = Vec::new();
    let mut files: Vec<std::ffi::OsString> = Vec::new();
    for e in read.flatten() {
        let name = e.file_name();
        let path = e.path();
        let is_link = e.file_type().is_ok_and(|t| t.is_symlink());
        if path.is_dir() {
            dirs.push((name, is_link));
        } else {
            files.push(name);
        }
    }
    files.sort();
    dirs.sort();
    for f in files {
        out.push(dir.join(f));
    }
    for (d, is_link) in dirs {
        if !is_link {
            walk_files(&dir.join(d), out);
        }
    }
}

#[cfg(test)]
#[path = "pypath_test.rs"]
mod pypath_test;
