//! Vectors recorded from Python 3.11's `os.path` (posixpath), the oracle.

use super::*;

#[test]
fn normpath_matches_posixpath() {
    for (p, want) in [
        ("", "."),
        ("/", "/"),
        ("//", "//"),
        ("///", "/"),
        ("//a/b", "//a/b"),
        ("a/../..", ".."),
        ("../a/./b//c/", "../a/b/c"),
        ("/a/../../b", "/b"),
        ("a/b/..", "a"),
        ("/x/node_modules/@s/p/../../q.js", "/x/node_modules/q.js"),
    ] {
        assert_eq!(normpath(p), want, "{p:?}");
    }
}

#[test]
fn join_dirname_relpath_match_posixpath() {
    for (a, b, want) in [
        ("/a", "b", "/a/b"),
        ("/a/", "b", "/a/b"),
        ("/a", "/b", "/b"),
        ("", "b", "b"),
    ] {
        assert_eq!(join(a, b), want);
    }
    for (p, want) in [
        ("/a/b", "/a"),
        ("/a/b/", "/a/b"),
        ("/", "/"),
        ("//x", "//"),
        ("a", ""),
        ("/a//b//", "/a//b"),
    ] {
        assert_eq!(dirname(p), want, "{p:?}");
    }
    for (p, s, want) in [
        ("/a/b/c", "/a", "b/c"),
        ("/a/b", "/a/b", "."),
        ("/a/x", "/a/b/c", "../../x"),
    ] {
        assert_eq!(relpath(p, s), want);
    }
}

/// Files of a directory first (sorted), then each subdirectory in turn; a
/// directory named `skip` is not entered.
#[test]
fn walk_files_lists_files_before_subdirectories_and_skips_by_name() {
    let d = std::env::temp_dir().join(format!("dev-walk-{}", std::process::id()));
    for f in ["b.js", "a.js", "sub/c.js", "node_modules/x/d.js"] {
        let p = d.join(f);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, "").unwrap();
    }
    let rel = |skip| {
        let mut out = Vec::new();
        walk_files(&d, skip, &mut out);
        out.iter()
            .map(|p| p.strip_prefix(&d).unwrap().to_string_lossy().to_string())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        rel(None),
        ["a.js", "b.js", "node_modules/x/d.js", "sub/c.js"]
    );
    assert_eq!(rel(Some("node_modules")), ["a.js", "b.js", "sub/c.js"]);
    std::fs::remove_dir_all(&d).unwrap();
}
