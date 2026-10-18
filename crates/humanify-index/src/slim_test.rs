//! The build-cache slimmer (exp131): a sourcemap loses its
//! `sourcesContent` and nothing the builder reads; a file shared by hard
//! links is rewritten once, through its inode, and stays shared.

use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use super::slim::{SlimOutcome, cache_maps, recover, slim_map_file, strip_sources_content};
use super::sourcemap::SourceMap;

fn temp(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("humanify-index-slim-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("mkdir");
    d
}

fn write(p: &Path, text: &str) {
    std::fs::create_dir_all(p.parent().expect("dir")).expect("mkdir");
    std::fs::write(p, text).expect("write");
}

/// A map whose sourcesContent holds every character a naive scanner trips
/// on: quotes, escapes, brackets, braces and commas inside strings.
const TRICKY: &str = r#"["a\"]},{[", "x\\", null, "A\n"]"#;

#[test]
fn sources_content_is_cut_out_and_every_other_byte_is_kept() {
    let cases = [
        // first member
        (
            format!(
                r#"{{"sourcesContent":{TRICKY},"version":3,"sources":["a.js"],"mappings":"AAAA"}}"#
            ),
            r#"{"version":3,"sources":["a.js"],"mappings":"AAAA"}"#.to_string(),
        ),
        // middle, with the whitespace a pretty-printer writes
        (
            format!(
                "{{\n  \"version\": 3,\n  \"sources\": [\"a.js\"],\n  \"sourcesContent\": {TRICKY},\n  \"mappings\": \"AAAA\"\n}}\n"
            ),
            "{\n  \"version\": 3,\n  \"sources\": [\"a.js\"],\n  \"mappings\": \"AAAA\"\n}\n"
                .to_string(),
        ),
        // last member: the comma before it goes
        (
            format!(
                r#"{{"version":3,"sources":["a.js"],"mappings":"AAAA","sourcesContent":{TRICKY}}}"#
            ),
            r#"{"version":3,"sources":["a.js"],"mappings":"AAAA"}"#.to_string(),
        ),
        // the only member
        (
            format!(r#"{{"sourcesContent":{TRICKY}}}"#),
            "{}".to_string(),
        ),
    ];
    for (input, want) in cases {
        let got = strip_sources_content(&input)
            .expect("parses")
            .expect("present");
        assert_eq!(got, want, "{input}");
        serde_json::from_str::<serde_json::Value>(&got).expect("still JSON");
    }
}

#[test]
fn a_map_without_sources_content_is_left_alone_and_garbage_is_refused() {
    let plain = r#"{"version":3,"sources":["a.js"],"names":["sourcesContent"],"mappings":"AAAA"}"#;
    assert_eq!(strip_sources_content(plain), Ok(None));
    // a key NESTED deeper than the top level is not the map's
    let nested = r#"{"x":{"sourcesContent":["a"]},"sources":[],"mappings":""}"#;
    assert_eq!(strip_sources_content(nested), Ok(None));
    for bad in [
        "",
        "[1]",
        r#"{"sources":["a.js"]"#,
        r#"{"a" 1}"#,
        r#"{"a":"unterminated}"#,
    ] {
        assert!(strip_sources_content(bad).is_err(), "{bad:?}");
    }
}

fn map_text() -> String {
    format!(
        r#"{{"version":3,"sources":["../../../node_modules/dep/a.js","../../../entries/req.cjs"],"sourcesContent":{TRICKY},"mappings":"AAAA,EACA;ACAA","names":[]}}"#
    )
}

#[test]
fn a_hard_linked_map_is_rewritten_once_through_its_inode_and_stays_shared() {
    let root = temp("links");
    let a = root.join("w1/v@1/build/s/req/out.js.map");
    let b = root.join("w2/v@1/build/s/req/out.js.map");
    let c = root.join("w3/out.js.map");
    write(&a, &map_text());
    std::fs::create_dir_all(b.parent().unwrap()).unwrap();
    std::fs::hard_link(&a, &b).unwrap();
    std::fs::create_dir_all(c.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(&a, &c).unwrap();
    let old = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
    std::fs::File::options()
        .write(true)
        .open(&a)
        .unwrap()
        .set_modified(old)
        .unwrap();
    let before = SourceMap::parse(&map_text()).unwrap();
    let ino = std::fs::metadata(&a).unwrap().ino();

    let journal = root.join("journal");
    std::fs::create_dir_all(&journal).unwrap();
    let outcome = slim_map_file(&a, Some(&journal)).expect("slims");
    let SlimOutcome::Stripped {
        before: n0,
        after: n1,
    } = outcome
    else {
        panic!("{outcome:?}")
    };
    assert!(n1 < n0);

    let text = std::fs::read_to_string(&b).unwrap();
    assert!(!text.contains("sourcesContent"), "{text}");
    assert_eq!(text.len() as u64, n1);
    let (ma, mb) = (
        std::fs::metadata(&a).unwrap(),
        std::fs::metadata(&b).unwrap(),
    );
    assert_eq!(
        (ma.ino(), mb.ino(), ma.nlink()),
        (ino, ino, 2),
        "still one shared inode"
    );
    assert_eq!(ma.modified().unwrap(), old, "mtime kept");
    assert!(
        std::fs::symlink_metadata(&c)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(std::fs::read_to_string(&c).unwrap(), text);
    let after = SourceMap::parse(&text).unwrap();
    assert_eq!(
        (after.sources, after.segments),
        (before.sources, before.segments)
    );
    assert_eq!(
        std::fs::read_dir(&journal).unwrap().count(),
        0,
        "journal cleared"
    );
    // idempotent: the second pass finds nothing to cut
    assert_eq!(slim_map_file(&b, Some(&journal)), Ok(SlimOutcome::Absent));
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn an_unreadable_map_is_left_byte_identical() {
    let root = temp("bad");
    let p = root.join("out.js.map");
    write(
        &p,
        r#"{"sources":["a.js"],"sourcesContent":["x"],"mappings":"!!"}"#,
    );
    assert!(matches!(
        slim_map_file(&p, None),
        Ok(SlimOutcome::Unreadable(_))
    ));
    assert_eq!(
        std::fs::read_to_string(&p).unwrap(),
        r#"{"sources":["a.js"],"sourcesContent":["x"],"mappings":"!!"}"#
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn an_interrupted_rewrite_is_finished_from_the_journal() {
    let root = temp("recover");
    let p = root.join("w/v@1/build/s/req/out.js.map");
    write(&p, "{\"half-written");
    let stripped = strip_sources_content(&map_text()).unwrap().unwrap();
    let journal = root.join("journal");
    let ino = std::fs::metadata(&p).unwrap().ino();
    let dev = std::fs::metadata(&p).unwrap().dev();
    write(
        &journal.join(format!("{dev}-{ino}")),
        &format!("{}\n{stripped}", p.display()),
    );
    // an entry never committed (still .tmp) means the original was never touched
    write(&journal.join("1-2.tmp"), "junk");
    assert_eq!(recover(&journal), Ok(1));
    assert_eq!(std::fs::read_to_string(&p).unwrap(), stripped);
    assert_eq!(std::fs::read_dir(&journal).unwrap().count(), 0);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn the_cache_maps_are_build_outputs_only_one_path_per_inode() {
    let root = temp("walk");
    let (w1, w2) = (root.join("w1"), root.join("w2"));
    let m1 = w1.join("a@1/build/esbuild-node-cjs-min/req/out.js.map");
    let m2 = w1.join("a@1/build/esbuild-node-cjs-min/dyn/out.js.map");
    write(&m1, &map_text());
    write(&m2, &map_text());
    write(&w1.join("a@1/build/esbuild-node-cjs-min/req/out.js"), "x");
    // never node_modules, never outside build/
    write(&w1.join("a@1/node_modules/dep/index.js.map"), &map_text());
    write(&w1.join("a@1/entries/x.map"), &map_text());
    // a hard-link copy of the work dir: the same inodes
    let l1 = w2.join("a@1/build/esbuild-node-cjs-min/req/out.js.map");
    std::fs::create_dir_all(l1.parent().unwrap()).unwrap();
    std::fs::hard_link(&m1, &l1).unwrap();
    // a symlinked build dir and node_modules are not followed
    std::os::unix::fs::symlink(w1.join("a@1/node_modules"), w2.join("a@1/node_modules")).unwrap();
    std::fs::create_dir_all(w2.join("b@1/build")).unwrap();
    std::os::unix::fs::symlink(
        w1.join("a@1/build/esbuild-node-cjs-min"),
        w2.join("b@1/build/esbuild-node-cjs-min"),
    )
    .unwrap();
    let (maps, skipped) = cache_maps(&[w1.clone(), w2.clone()]);
    assert_eq!(maps, vec![m2, m1]);
    assert_eq!(skipped, 1, "one symlink not followed");
    std::fs::remove_dir_all(&root).ok();
}
