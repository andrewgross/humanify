//! Bun's EMBEDDED FILES (exp217): the non-JS files a compiled app carries
//! beside its chunks, as `bun build --compile --splitting` (Bun 1.4.1)
//! writes their uses — a text import becomes a run-time
//! `import.meta.require("/$bunfs/root/<name>")` (through an alias of it),
//! whose value is the file's TEXT; a `file` import becomes the path
//! string `"/$bunfs/root/<name>"`, which the app reads itself. Linked, a
//! text read is the text (what Bun writes for a text import in one file)
//! and a path is the copy the output tree carries under `embedded/`.

use std::path::{Path, PathBuf};

use super::{EMBEDDED_DIR, EmbeddedFile, LinkReport};
use crate::unpack::input::read_input;

const HEAD: &str = "// @bun\n";

/// Bun's ESM runtime chunk: the alias of `import.meta.require` its
/// chunks import (`var C=import.meta.require;export{C as Hxd}`).
const RUNTIME: (&str, &[u8]) = (
    "rt.js",
    b"// @bun\nvar C=import.meta.require;export{C as Hxd};\n",
);

/// A chunk the entry imports, so the input IS the code-split layout
/// (an entry with a `/$bunfs/root/<chunk>.js` sibling).
const CHUNK: (&str, &[u8]) = ("c.js", b"// @bun\n");

/// Write a layout (any bytes) into a fresh directory; the first file is
/// the entry.
fn layout(files: &[(&str, &[u8])]) -> PathBuf {
    let dir = crate::scratch::temp_dir("chunk-embedded").unwrap();
    for (name, bytes) in files {
        let path = dir.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }
    dir.join(files[0].0)
}

fn linked(entry: &Path) -> (String, LinkReport) {
    let bundle = read_input(entry).unwrap();
    let report = bundle.link.expect("the layout is linked");
    (bundle.text, report)
}

/// Run a linked program as Bun's CommonJS output runs, from a directory
/// that holds the embedded files the way the output tree does
/// (`<dir>/embedded/<name>`). Returns stdout.
fn run_with(text: &str, embedded: &[EmbeddedFile]) -> String {
    let dir = crate::scratch::temp_dir("chunk-embedded-run").unwrap();
    for f in embedded {
        let to = dir.join(EMBEDDED_DIR).join(&f.rel);
        std::fs::create_dir_all(to.parent().unwrap()).unwrap();
        std::fs::copy(&f.source, to).unwrap();
    }
    let file = dir.join("linked.js");
    std::fs::write(&file, text).unwrap();
    let loader = "const p=process.argv[1];\
        const t=require('fs').readFileSync(p,'utf8').split('\\n').slice(1).join('\\n');\
        const f=require('vm').runInThisContext(t,{filename:p});\
        f(exports,require,module,p,require('path').dirname(p));";
    let out = std::process::Command::new("node")
        .args(["-e", loader])
        .arg(&file)
        .output()
        .expect("node is on PATH (the gate, cargo dev check, needs it)");
    assert!(
        out.status.success(),
        "the linked program failed under Node:\n{}\n--- program ---\n{text}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn utf16le(s: &str) -> Vec<u8> {
    s.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

/// The entry reads `name` through the runtime's alias, inside an init
/// (Bun's shape: `var t;var i=e(()=>{t=r("/$bunfs/root/<name>")})`), and
/// prints it as JSON.
fn reads_text(name: &str) -> String {
    format!(
        "{HEAD}import{{Hxd as r}}from\"/$bunfs/root/rt.js\";var t;function i(){{t=r(\"/$bunfs/root/{name}\")}}i();console.log(JSON.stringify(t));\n"
    )
}

#[test]
fn a_required_text_file_becomes_its_text() {
    let text = "# Note\r\nback`tick` ${not} \\slash\ttab\n'single' \"double\"\n";
    let entry = layout(&[
        ("index.js", reads_text("note-07qcyhv4.md").as_bytes()),
        RUNTIME,
        ("note-07qcyhv4.md", text.as_bytes()),
    ]);
    let (linked_text, report) = linked(&entry);
    assert_eq!(
        run_with(&linked_text, &[]),
        format!("{}\n", serde_json::to_string(text).unwrap())
    );
    assert!(
        !linked_text.contains("$bunfs"),
        "the read is the text, no path left:\n{linked_text}"
    );
    assert_eq!(report.embedded_text, 1);
    assert!(report.embedded.is_empty(), "a text read copies nothing");
}

#[test]
fn utf16_text_is_decoded() {
    // Bun stores a text file that is not pure ASCII as UTF-16LE (the
    // module record's encoding byte, which the unpacked file loses): one
    // with ASCII in it has NULs, one with none (pure CJK) has none.
    let mixed = "em \u{2014} caf\u{e9} \u{4e2d}\u{6587} \u{1f600}\n";
    let cjk = "\u{4e2d}\u{6587}";
    for text in [mixed, cjk] {
        let entry = layout(&[
            ("index.js", reads_text("u-1.md").as_bytes()),
            RUNTIME,
            ("u-1.md", &utf16le(text)),
        ]);
        let (linked_text, _) = linked(&entry);
        assert_eq!(
            run_with(&linked_text, &[]),
            format!("{}\n", serde_json::to_string(text).unwrap())
        );
    }
}

#[test]
fn a_direct_import_meta_require_of_a_text_file_is_its_text() {
    let entry = layout(&[
        (
            "index.js",
            b"// @bun\nimport\"/$bunfs/root/c.js\";console.log(import.meta.require(\"/$bunfs/root/plain-x.txt\"));\n",
        ),
        ("plain-x.txt", b"plain text"),
        CHUNK,
    ]);
    let (linked_text, report) = linked(&entry);
    assert_eq!(run_with(&linked_text, &[]), "plain text\n");
    assert_eq!(
        report.chunks_linked, 2,
        "the entry and c.js; the text file is no chunk"
    );
}

#[test]
fn a_path_string_names_the_trees_copy() {
    // Bun's `file` loader: the import is the path; the app reads it.
    let entry = layout(&[
        (
            "index.js",
            b"// @bun\nimport\"/$bunfs/root/c.js\";import{readFileSync as f}from\"fs\";var p=\"/$bunfs/root/payload-6qp9xeht.bin\";console.log(f(p,\"utf8\"));\n",
        ),
        ("payload-6qp9xeht.bin", b"BIN\x01"),
        CHUNK,
    ]);
    let (linked_text, report) = linked(&entry);
    assert!(
        linked_text.contains("__dirname+\"/embedded/payload-6qp9xeht.bin\""),
        "{linked_text}"
    );
    assert_eq!(
        report.embedded,
        vec![EmbeddedFile {
            rel: "payload-6qp9xeht.bin".into(),
            source: entry.parent().unwrap().join("payload-6qp9xeht.bin"),
        }]
    );
    assert_eq!(report.embedded_paths, 1);
    assert_eq!(run_with(&linked_text, &report.embedded), "BIN\u{1}\n");
}

#[test]
fn a_native_addon_is_required_from_the_trees_copy() {
    // Bun's `napi` loader: `require("/$bunfs/root/x.node")` stays a
    // require — of the copy.
    let entry = layout(&[
        (
            "index.js",
            b"// @bun\nimport{Hxd as r}from\"/$bunfs/root/rt.js\";var m=()=>r(\"/$bunfs/root/addon.node\");console.log(typeof m);\n",
        ),
        RUNTIME,
        ("addon.node", b"\x7fELF"),
    ]);
    let (linked_text, report) = linked(&entry);
    assert!(
        linked_text.contains("require(__dirname+\"/embedded/addon.node\")"),
        "{linked_text}"
    );
    assert_eq!(report.embedded.len(), 1);
    assert_eq!(report.embedded_text, 0);
}

#[test]
fn a_path_passed_to_any_other_function_is_a_path() {
    let entry = layout(&[
        (
            "index.js",
            b"// @bun\nimport\"/$bunfs/root/c.js\";function g(p){return require(\"fs\").readFileSync(p,\"utf8\")}console.log(g(\"/$bunfs/root/sub/n.md\"));\n",
        ),
        ("sub/n.md", b"from a path"),
        CHUNK,
    ]);
    let (linked_text, report) = linked(&entry);
    assert_eq!(report.embedded[0].rel, "sub/n.md");
    assert_eq!(run_with(&linked_text, &report.embedded), "from a path\n");
}

#[test]
fn the_same_file_named_twice_is_copied_once() {
    let entry = layout(&[
        (
            "index.js",
            b"// @bun\nimport\"/$bunfs/root/c.js\";var a=\"/$bunfs/root/p.bin\",b=\"/$bunfs/root/p.bin\";console.log(a===b);\n",
        ),
        ("p.bin", b"x"),
        CHUNK,
    ]);
    let (linked_text, report) = linked(&entry);
    assert_eq!(report.embedded.len(), 1);
    assert_eq!(report.embedded_paths, 2);
    assert_eq!(run_with(&linked_text, &report.embedded), "true\n");
}

#[test]
fn a_missing_embedded_file_is_refused() {
    let entry = layout(&[("index.js", reads_text("gone-1.md").as_bytes()), RUNTIME]);
    let err = read_input(&entry).unwrap_err();
    assert!(err.contains("gone-1.md"), "{err}");
}

#[test]
fn a_name_that_leaves_the_root_is_refused() {
    let entry = layout(&[
        (
            "app/index.js",
            b"// @bun\nimport\"/$bunfs/root/c.js\";var p=\"/$bunfs/root/../secret.txt\";\n",
        ),
        ("app/c.js", b"// @bun\n"),
        ("secret.txt", b"s"),
    ]);
    let err = read_input(&entry).unwrap_err();
    assert!(err.contains("secret.txt"), "{err}");
}

#[test]
fn a_path_to_a_bun_module_and_the_bare_root_are_left_alone() {
    // A worker entry Bun built (its first line is Bun's pragma) is code,
    // not an embedded file; `"/$bunfs/root/"` alone names no file.
    let entry = layout(&[
        (
            "index.js",
            b"// @bun\nimport\"/$bunfs/root/c.js\";var w={URL:\"/$bunfs/root/src/w/worker.js\"},r=\"/$bunfs/root/\";\n",
        ),
        ("c.js", b"// @bun\n"),
        ("src/w/worker.js", b"// @bun\nimport\"/$bunfs/root/c.js\";\n"),
    ]);
    let (linked_text, report) = linked(&entry);
    assert!(
        linked_text.contains("URL:\"/$bunfs/root/src/w/worker.js\""),
        "{linked_text}"
    );
    assert!(linked_text.contains("r=\"/$bunfs/root/\""), "{linked_text}");
    assert!(report.embedded.is_empty());
    assert_eq!(report.bun_module_paths, 1);
}

#[test]
fn a_path_where_dirname_is_rebound_is_refused() {
    let entry = layout(&[
        (
            "index.js",
            b"// @bun\nimport\"/$bunfs/root/c.js\";function f(__dirname){return\"/$bunfs/root/p.bin\"}\n",
        ),
        ("p.bin", b"x"),
        CHUNK,
    ]);
    let err = read_input(&entry).unwrap_err();
    assert!(err.contains("__dirname"), "{err}");
}

#[test]
fn the_report_counts_embedded_files() {
    let entry = layout(&[
        ("index.js", reads_text("note-1.md").as_bytes()),
        RUNTIME,
        ("note-1.md", b"n"),
    ]);
    let (_, report) = linked(&entry);
    let json = report.to_json();
    assert_eq!(json["embeddedText"], 1);
    assert_eq!(json["embeddedFiles"], 0);
    assert_eq!(json["embeddedPaths"], 0);
    assert_eq!(json["bunModulePaths"], 0);
}

#[test]
fn a_require_with_more_than_the_name_is_refused() {
    // Bun's `sqlite_embedded` loader writes
    // `import.meta.require("<name>", {with: {type: "sqlite"}}).db`
    // (src/bundler/ParseTask.rs, the Sqlite arm): a load this linker does
    // not reproduce, so it says so rather than guess.
    let entry = layout(&[
        (
            "index.js",
            b"// @bun\nimport\"/$bunfs/root/c.js\";var d=import.meta.require(\"/$bunfs/root/db-1.sqlite\",{with:{type:\"sqlite\"}}).db;\n",
        ),
        CHUNK,
        ("db-1.sqlite", b"SQLite format 3\x00"),
    ]);
    let err = read_input(&entry).unwrap_err();
    assert!(err.contains("db-1.sqlite"), "{err}");
}
