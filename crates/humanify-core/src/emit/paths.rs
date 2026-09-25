//! Relative-import path computation shared by the runnable-tree emitters
//! (TS `src/split/emitter.ts`: the runnable emit here, the Bun re-link in
//! WP5.4). Paths are relative to the output root (`src/app.js`,
//! `lib/shared.js`); `/` is the only separator.

/// `computeRelativeImportPath(fromFile, toFile)`: the specifier a file at
/// `from_file` uses to require `to_file`. Only `./` and `../` mark a
/// specifier as relative — a bare `.humanify/…` would resolve as a package
/// name, so a same-or-descendant path always gets `./`.
pub fn compute_relative_import_path(from_file: &str, to_file: &str) -> String {
    let from_dir = from_file.rfind('/').map_or("", |i| &from_file[..i]);
    let (to_dir, to_basename) = match to_file.rfind('/') {
        Some(i) => (&to_file[..i], &to_file[i + 1..]),
        None => ("", to_file),
    };
    if from_dir == to_dir {
        return format!("./{to_basename}");
    }
    let from_parts: Vec<&str> = if from_dir.is_empty() {
        Vec::new()
    } else {
        from_dir.split('/').collect()
    };
    let to_parts: Vec<&str> = if to_dir.is_empty() {
        Vec::new()
    } else {
        to_dir.split('/').collect()
    };
    let mut common = 0;
    while common < from_parts.len()
        && common < to_parts.len()
        && from_parts[common] == to_parts[common]
    {
        common += 1;
    }
    let mut segments: Vec<&str> = vec![".."; from_parts.len() - common];
    segments.extend(&to_parts[common..]);
    segments.push(to_basename);
    let rel = segments.join("/");
    if rel.starts_with("../") {
        rel
    } else {
        format!("./{rel}")
    }
}

#[cfg(test)]
mod paths_test;
