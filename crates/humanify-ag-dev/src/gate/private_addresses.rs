//! `cargo dev no-private-addresses`: no tracked file holds a private
//! network address (RFC 1918: 10/8, 172.16/12, 192.168/16). This repo is
//! PUBLIC; such an address is someone's local hosting detail — the LLM
//! server's address sat in 91 places until 2026-10-05. It lives in the
//! git-ignored `.humanify.local.json` now, and docs write `<llm-host>`.
//! The successor of test/no-private-addresses.test.ts, as a gate stage.

use std::path::Path;

use crate::HostEnv;
use crate::settings::Parsed;

/// Tracked files that genuinely need such an address, each with its
/// reason. Empty: nothing in the tree needs one.
pub const ALLOWED: &[(&str, &str)] = &[];

/// One decimal octet 0-255 without a leading zero, at the start of `s`:
/// its value and length.
fn octet(s: &[u8]) -> Option<(u32, usize)> {
    let len = s.iter().take(3).take_while(|b| b.is_ascii_digit()).count();
    if len == 0 || (len > 1 && s[0] == b'0') {
        return None;
    }
    let v: u32 = std::str::from_utf8(&s[..len]).ok()?.parse().ok()?;
    // A fourth digit makes it no octet at all.
    if v > 255 || s.get(len).is_some_and(u8::is_ascii_digit) {
        return None;
    }
    Some((v, len))
}

/// The dotted quad at the start of `s`, if any: its octets and length.
fn quad(s: &[u8]) -> Option<([u32; 4], usize)> {
    let mut at = 0;
    let mut parts = [0u32; 4];
    for (i, part) in parts.iter_mut().enumerate() {
        if i > 0 {
            if s.get(at) != Some(&b'.') {
                return None;
            }
            at += 1;
        }
        let (v, len) = octet(&s[at..])?;
        *part = v;
        at += len;
    }
    // Not part of a longer dotted number (a fifth `.N`, or more digits).
    let rest = &s[at..];
    let longer = rest.first().is_some_and(u8::is_ascii_digit)
        || (rest.first() == Some(&b'.') && rest.get(1).is_some_and(u8::is_ascii_digit));
    (!longer).then_some((parts, at))
}

fn is_private(p: [u32; 4]) -> bool {
    p[0] == 10 || (p[0] == 172 && (16..=31).contains(&p[1])) || (p[0] == 192 && p[1] == 168)
}

/// Every private address in `text`, in order. A match must not follow a
/// digit or a dot (a version string like `v1.<quad>` is not an address).
pub fn private_addresses(text: &str) -> Vec<String> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let starts =
            b[i].is_ascii_digit() && (i == 0 || !(b[i - 1].is_ascii_digit() || b[i - 1] == b'.'));
        if starts && let Some((parts, len)) = quad(&b[i..]) {
            if is_private(parts) {
                out.push(text[i..i + len].to_string());
            }
            i += len;
            continue;
        }
        i += 1;
    }
    out
}

/// Every hit in `files` (path, bytes) not on `allowed`, as `path: a, b`.
pub fn scan(files: &[(String, Vec<u8>)], allowed: &[(&str, &str)]) -> Vec<String> {
    let mut hits = Vec::new();
    for (path, bytes) in files {
        if allowed.iter().any(|(a, _)| a == path) {
            continue;
        }
        // Latin-1: every byte is one char, so binary files scan too.
        let text: String = bytes.iter().map(|&c| c as char).collect();
        let found = private_addresses(&text);
        let mut unique: Vec<String> = Vec::new();
        for f in found {
            if !unique.contains(&f) {
                unique.push(f);
            }
        }
        if !unique.is_empty() {
            hits.push(format!("{path}: {}", unique.join(", ")));
        }
    }
    hits
}

/// Every tracked file of `repo` with its bytes (a tracked path missing
/// from the working tree is skipped).
pub fn tracked(repo: &Path) -> Result<Vec<(String, Vec<u8>)>, String> {
    let listing = crate::proc::git(repo, &["ls-files", "-z"])?;
    Ok(listing
        .split('\0')
        .filter(|p| !p.is_empty())
        .filter_map(|p| {
            let bytes = std::fs::read(repo.join(p)).ok()?;
            Some((p.to_string(), bytes))
        })
        .collect())
}

/// The failure message for `hits`.
pub fn report(hits: &[String]) -> String {
    format!(
        "private-network address in a tracked file — this repo is public. Write `<llm-host>` in \
         docs; the endpoint is read from the git-ignored .humanify.local.json.\n  {}\n",
        hits.join("\n  ")
    )
}

/// Run the check over this repository's tracked files.
pub fn run(_p: &Parsed, _host: &HostEnv) -> (u8, String, String) {
    let repo = crate::paths::repo_root();
    let stale: Vec<&str> = ALLOWED.iter().map(|(p, _)| *p).collect();
    let files = match tracked(&repo) {
        Ok(f) => f,
        Err(e) => return (1, String::new(), format!("no-private-addresses: {e}\n")),
    };
    let missing: Vec<&&str> = stale
        .iter()
        .filter(|p| !files.iter().any(|(f, _)| f == **p))
        .collect();
    if !missing.is_empty() {
        return (
            1,
            String::new(),
            format!("no-private-addresses: stale ALLOWED entries (not tracked): {missing:?}\n"),
        );
    }
    let hits = scan(&files, ALLOWED);
    if hits.is_empty() {
        (
            0,
            format!("no private address in {} tracked files\n", files.len()),
            String::new(),
        )
    } else {
        (1, String::new(), report(&hits))
    }
}

#[cfg(test)]
#[path = "private_addresses_test.rs"]
mod private_addresses_test;
