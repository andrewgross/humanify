//! The chunk linker on hand-written Bun code-split layouts: the shapes
//! `bun build --compile --splitting --format esm` writes (an entry that
//! imports `/$bunfs/root/<chunk>.js` siblings), linked back into one
//! program and RUN by Node where behaviour is the question.

use std::path::{Path, PathBuf};

use super::{LinkReport, link, read_input};

/// Write a layout into a fresh directory; the first file is the entry.
fn layout(files: &[(&str, &str)]) -> PathBuf {
    let dir = crate::scratch::temp_dir("chunk-link").unwrap();
    for (name, text) in files {
        std::fs::write(dir.join(name), text).unwrap();
    }
    dir.join(files[0].0)
}

/// Run a linked program the way Bun's CommonJS output is run: strip the
/// pragma line, evaluate the wrapper, call it with the CommonJS bindings.
/// Returns stdout; a failing run fails the test with its stderr.
fn run(text: &str) -> String {
    let dir = crate::scratch::temp_dir("chunk-link-run").unwrap();
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

fn linked(entry: &Path) -> (String, LinkReport) {
    let bundle = read_input(entry).unwrap();
    let report = bundle.link.expect("the layout is linked");
    (bundle.text, report)
}

const HEAD: &str = "// @bun\n";

#[test]
fn a_single_file_bundle_is_read_unchanged() {
    let text = "// @bun @bun-cjs\n(function(exports, require, module, __filename, __dirname) {console.log(1)})\n";
    let entry = layout(&[("index.js", text)]);
    let bundle = read_input(&entry).unwrap();
    assert!(bundle.link.is_none());
    assert_eq!(bundle.text, text);
}

#[test]
fn an_esm_file_without_bunfs_siblings_is_read_unchanged() {
    // An absolute bunfs specifier whose file is not on disk, and a
    // relative one: neither is this layout.
    let text = format!(
        "{HEAD}import{{a as b}}from\"/$bunfs/root/chunk-missing.js\";import\"./chunk-x.js\";b();\n"
    );
    let entry = layout(&[("index.js", &text), ("chunk-x.js", "export{};")]);
    let bundle = read_input(&entry).unwrap();
    assert!(bundle.link.is_none());
    assert_eq!(bundle.text, text);
}

#[test]
fn named_imports_become_the_exporting_chunks_binding() {
    let entry = layout(&[
        (
            "index.js",
            &format!(
                "{HEAD}// Version: 1.0.0\nimport{{xA as a}}from\"/$bunfs/root/chunk-one.js\";console.log(a(2));\n"
            ),
        ),
        (
            "chunk-one.js",
            &format!("{HEAD}function r(n){{return n*21}}\nexport{{r as xA}};\n"),
        ),
    ]);
    let (text, report) = linked(&entry);
    assert_eq!(run(&text), "42\n");
    assert!(text.starts_with(
        "// @bun @bun-cjs\n(function(exports, require, module, __filename, __dirname) {"
    ));
    assert!(
        text.contains("// Version: 1.0.0"),
        "the entry's header is kept:\n{text}"
    );
    assert!(
        !text.contains("import{") && !text.contains("export{"),
        "{text}"
    );
    assert_eq!(report.chunks_linked, 2);
}

#[test]
fn colliding_top_level_names_are_renamed_apart() {
    // Both chunks bind `a` and `n` at the top level; the entry's nested
    // scope binds `b`, the name its import alias stands for.
    let entry = layout(&[
        (
            "index.js",
            &format!(
                "{HEAD}import{{p as b}}from\"/$bunfs/root/chunk-p.js\";import{{q as c}}from\"/$bunfs/root/chunk-q.js\";\
                 var a=\"entry\";function show(){{let n=b(),q=c();console.log(a,n,q)}}show();\n"
            ),
        ),
        (
            "chunk-p.js",
            &format!("{HEAD}var a=\"P\";function n(){{return a}}export{{n as p}};\n"),
        ),
        (
            "chunk-q.js",
            &format!("{HEAD}var a=\"Q\";function n(){{let q=a;return q}}export{{n as q}};\n"),
        ),
    ]);
    let (text, report) = linked(&entry);
    assert_eq!(run(&text), "entry P Q\n");
    assert!(report.renamed > 0);
}

#[test]
fn a_re_export_chain_resolves_to_the_defining_binding() {
    let entry = layout(&[
        (
            "index.js",
            &format!(
                "{HEAD}import{{Y as y}}from\"/$bunfs/root/chunk-facade.js\";console.log(y());\n"
            ),
        ),
        (
            "chunk-facade.js",
            &format!(
                "{HEAD}import{{X as a}}from\"/$bunfs/root/chunk-real.js\";export{{a as Y}};\n"
            ),
        ),
        (
            "chunk-real.js",
            &format!("{HEAD}function f(){{return\"real\"}}export{{f as X}};\n"),
        ),
    ]);
    let (text, _) = linked(&entry);
    assert_eq!(run(&text), "real\n");
}

#[test]
fn static_imports_evaluate_in_module_order() {
    // ESM order: a module's imports (depth first, in source order) run
    // before its own body, each module once.
    let entry = layout(&[
        (
            "index.js",
            &format!(
                "{HEAD}import\"/$bunfs/root/chunk-a.js\";import\"/$bunfs/root/chunk-b.js\";console.log(\"entry\");\n"
            ),
        ),
        (
            "chunk-a.js",
            &format!("{HEAD}import\"/$bunfs/root/chunk-c.js\";console.log(\"a\");\n"),
        ),
        (
            "chunk-b.js",
            &format!("{HEAD}import\"/$bunfs/root/chunk-c.js\";console.log(\"b\");\n"),
        ),
        ("chunk-c.js", &format!("{HEAD}console.log(\"c\");\n")),
    ]);
    let (text, report) = linked(&entry);
    assert_eq!(run(&text), "c\na\nb\nentry\n");
    assert_eq!(report.eager, 4);
    assert_eq!(report.lazy, 0);
}

#[test]
fn a_lazily_imported_chunk_runs_only_when_imported_and_once() {
    let entry = layout(&[
        (
            "index.js",
            &format!(
                "{HEAD}async function main(){{console.log(\"start\");\
                 let{{lazy:l}}=await import(\"/$bunfs/root/chunk-l.js\");console.log(l());\
                 let m=await import(\"/$bunfs/root/chunk-l.js\");console.log(m.lazy===l)}}main();\n"
            ),
        ),
        (
            "chunk-l.js",
            &format!(
                "{HEAD}import{{d as e}}from\"/$bunfs/root/chunk-d.js\";var t=[];t.push(e);console.log(\"l ran\");\
                 function f(){{return t.length}}export{{f as lazy}};\n"
            ),
        ),
        (
            "chunk-d.js",
            &format!("{HEAD}console.log(\"d ran\");var x=7;export{{x as d}};\n"),
        ),
    ]);
    let (text, report) = linked(&entry);
    assert_eq!(run(&text), "start\nd ran\nl ran\n1\ntrue\n");
    assert_eq!(report.lazy, 2);
    assert_eq!(report.dynamic_imports, 2);
}

#[test]
fn a_lazy_chunks_pure_declarations_stay_at_the_top_level() {
    let entry = layout(&[
        (
            "index.js",
            &format!("{HEAD}import(\"/$bunfs/root/chunk-l.js\").then((m)=>console.log(m.g()));\n"),
        ),
        (
            "chunk-l.js",
            &format!("{HEAD}function pureFn(){{return 5}}var k=pureFn();export{{pureFn as g}};\n"),
        ),
    ]);
    let (text, report) = linked(&entry);
    assert_eq!(run(&text), "5\n");
    // The function is a top-level statement; only the call moves into the
    // chunk's init.
    assert!(text.contains("function pureFn(){return 5}var k;"), "{text}");
    assert!(text.contains("k=pureFn();"), "{text}");
    assert_eq!(report.inits, 1);
    // The namespace object is Bun's: `var ns={};__export(ns,{g:()=>…})`,
    // whose arrows are not functions with names to transfer.
    assert!(text.contains("={};__export("), "{text}");
    assert!(text.contains("g:()=>pureFn"), "{text}");
}

#[test]
fn a_default_export_expression_is_bound() {
    let entry = layout(&[
        (
            "index.js",
            &format!(
                "{HEAD}import(\"/$bunfs/root/chunk-d.js\").then((m)=>console.log(m.default.v));\n"
            ),
        ),
        (
            "chunk-d.js",
            &format!("{HEAD}function mk(){{return{{v:9}}}}export default mk();\n"),
        ),
    ]);
    let (text, _) = linked(&entry);
    assert_eq!(run(&text), "9\n");
}

#[test]
fn import_meta_becomes_the_commonjs_bindings() {
    let entry = layout(&[
        (
            "index.js",
            &format!("{HEAD}import{{m as m}}from\"/$bunfs/root/chunk-m.js\";m();\n"),
        ),
        (
            "chunk-m.js",
            &format!(
                "{HEAD}var r=import.meta.require;function m(){{console.log(import.meta.dirname===__dirname,\
                 import.meta.url===require(\"url\").pathToFileURL(__filename).href,r(\"path\").sep,\
                 import.meta.dir===__dirname,import.meta.path===__filename,import.meta.filename===__filename,\
                 import.meta.file===require(\"path\").basename(__filename),typeof import.meta.main)}}export{{m}};\n"
            ),
        ),
    ]);
    let (text, _) = linked(&entry);
    assert!(!text.contains("import.meta"), "{text}");
    assert_eq!(run(&text), "true true / true true true true boolean\n");
}

#[test]
fn import_meta_require_of_a_chunk_initializes_it_synchronously() {
    let entry = layout(&[
        (
            "index.js",
            &format!(
                "{HEAD}console.log(\"a\");console.log(import.meta.require(\"/$bunfs/root/chunk-s.js\").s());\n"
            ),
        ),
        (
            "chunk-s.js",
            &format!(
                "{HEAD}console.log(\"s ran\");function f(){{return\"sync\"}}export{{f as s}};\n"
            ),
        ),
    ]);
    let (text, report) = linked(&entry);
    assert_eq!(run(&text), "a\ns ran\nsync\n");
    assert_eq!(report.sync_requires, 1);
}

#[test]
fn external_imports_become_requires() {
    let entry = layout(&[(
        "index.js",
        &format!(
            "{HEAD}import p from\"path\";import{{join as j}}from\"path\";import*as q from\"path\";console.log(p.sep,j(\"a\",\"b\"),q.sep);\n"
        ),
    )]);
    // A lone entry with only externals is not a chunked layout ...
    assert!(read_input(&entry).unwrap().link.is_none());
    // ... but the linker still maps its externals when it links one.
    let (text, _) = link(&entry).unwrap();
    assert_eq!(run(&text), "/ a/b /\n");
}

/// Bun's `__toESM` (the runtime chunk of a `--compile --splitting` build).
const TO_ESM: &str = "var f=Object.create;var{getPrototypeOf:h,defineProperty:d,getOwnPropertyNames:i}=Object;\
var p=Object.prototype.hasOwnProperty;function s(r){return this[r]}var x,b,g=(r,e,t)=>{var u=r!=null&&typeof r===\"object\";\
if(u){var a=e?x??=new WeakMap:b??=new WeakMap,c=a.get(r);if(c)return c}t=r!=null?f(h(r)):{};\
let n=e||!r||!r.__esModule?d(t,\"default\",{value:r,enumerable:!0}):t;for(let o of i(r))if(!p.call(n,o))\
d(n,o,{get:s.bind(r,o),enumerable:!0});if(u)a.set(r,n);return n};";

#[test]
fn external_imports_take_buns_commonjs_shape() {
    // Bun's CommonJS output (the shape the prior release was matched in):
    // a named import reads a member of `require(m)`; a default import
    // reads `.default` of `__toESM(require(m))`.
    let entry = layout(&[
        (
            "index.js",
            &format!(
                "{HEAD}import{{t as t}}from\"/$bunfs/root/chunk-rt.js\";import p from\"path\";import{{join as j,sep}}from\"path\";\
                 import*as q from\"path\";function show(){{console.log(p.sep,j(\"a\",\"b\"),{{sep}}.sep,q.sep,typeof t)}}show();\n"
            ),
        ),
        ("chunk-rt.js", &format!("{HEAD}{TO_ESM}export{{g as t}};\n")),
    ]);
    let (text, _) = linked(&entry);
    assert_eq!(run(&text), "/ a/b / / function\n");
    let body = text.split("function show()").nth(1).unwrap();
    assert!(body.contains(".default.sep"), "{body}");
    assert!(body.contains(".join(\"a\",\"b\")"), "{body}");
    assert!(body.contains("{sep: "), "{body}");
    assert!(text.contains("=g(require(\"path\"))"), "{text}");
}

#[test]
fn a_shorthand_property_keeps_its_key_when_the_binding_is_renamed() {
    let entry = layout(&[
        (
            "index.js",
            &format!(
                "{HEAD}import{{v as w}}from\"/$bunfs/root/chunk-v.js\";var a=1;console.log(JSON.stringify(w()),a);\n"
            ),
        ),
        (
            "chunk-v.js",
            &format!("{HEAD}var a=2;function v(){{return{{a}}}}export{{v}};\n"),
        ),
    ]);
    let (text, _) = linked(&entry);
    assert_eq!(run(&text), "{\"a\":2} 1\n");
}

#[test]
fn top_level_await_in_a_chunk_is_refused() {
    let entry = layout(&[
        (
            "index.js",
            &format!("{HEAD}import{{v as w}}from\"/$bunfs/root/chunk-v.js\";w();\n"),
        ),
        (
            "chunk-v.js",
            &format!("{HEAD}var a=await Promise.resolve(1);function v(){{return a}}export{{v}};\n"),
        ),
    ]);
    let err = read_input(&entry).unwrap_err();
    assert!(err.contains("top-level await"), "{err}");
}

#[test]
fn an_import_of_a_name_the_chunk_does_not_export_is_refused() {
    let entry = layout(&[
        (
            "index.js",
            &format!("{HEAD}import{{nope as w}}from\"/$bunfs/root/chunk-v.js\";w();\n"),
        ),
        (
            "chunk-v.js",
            &format!("{HEAD}function v(){{}}export{{v}};\n"),
        ),
    ]);
    let err = read_input(&entry).unwrap_err();
    assert!(err.contains("nope"), "{err}");
}

#[test]
fn linking_is_deterministic() {
    let files = [
        (
            "index.js",
            format!(
                "{HEAD}import{{p as b}}from\"/$bunfs/root/chunk-p.js\";import(\"/$bunfs/root/chunk-q.js\");b();\n"
            ),
        ),
        (
            "chunk-p.js",
            format!("{HEAD}var a=1;function n(){{return a}}export{{n as p}};\n"),
        ),
        (
            "chunk-q.js",
            format!("{HEAD}var a=2;console.log(a);function n(){{return a}}export{{n as q}};\n"),
        ),
    ];
    let files: Vec<(&str, &str)> = files.iter().map(|(n, t)| (*n, t.as_str())).collect();
    let one = linked(&layout(&files)).0;
    let two = linked(&layout(&files)).0;
    assert_eq!(one, two);
}

#[test]
fn a_lazy_chunk_is_written_beside_the_chunk_that_loads_it() {
    // Bun's single-file layout keeps a lazily loaded module near its
    // importer: the entry's static imports A then B, A loads L lazily —
    // written L, A, B, entry (each chunk after what it reaches).
    // Evaluation order is unchanged (L runs only when loaded).
    let entry = layout(&[
        (
            "index.js",
            &format!(
                "{HEAD}import\"/$bunfs/root/chunk-a.js\";import\"/$bunfs/root/chunk-b.js\";console.log(\"ENTRY\");\n"
            ),
        ),
        (
            "chunk-a.js",
            &format!(
                "{HEAD}console.log(\"A\");export function la(){{return import(\"/$bunfs/root/chunk-l.js\")}}\n"
            ),
        ),
        ("chunk-b.js", &format!("{HEAD}console.log(\"B\");\n")),
        ("chunk-l.js", &format!("{HEAD}console.log(\"L\");\n")),
    ]);
    let (text, _) = linked(&entry);
    assert_eq!(run(&text), "A\nB\nENTRY\n");
    let at = |s: &str| text.find(s).unwrap();
    assert!(
        at("\"L\"") < at("\"A\"") && at("\"A\"") < at("\"B\"") && at("\"B\"") < at("\"ENTRY\""),
        "{text}"
    );
}
