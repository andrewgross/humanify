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
