//! The per-file library-import names (finding #91): each split file names
//! its own library imports the way people write them, no numbers.

use super::{FileLibraryNames, name_library_imports_in_file, name_library_imports_in_tree};
use crate::naming::plumbing::conventional_import_name;
use crate::rename::eligibility::{Eligibility, NeverRename};
use crate::rename::name_profile::NameProfile;

fn run(text: &str) -> FileLibraryNames {
    let eligible = Eligibility::new(NeverRename::default());
    name_library_imports_in_file("src/a.js", text, &eligible, NameProfile::Bun)
}

fn text_of(out: &FileLibraryNames) -> &str {
    out.text.as_deref().expect("the file changed")
}

/// A split file's own wrapper form: the module-level binding written once
/// inside the lazy-init closure, through the bundle's require.
fn split_file(binding: &str, spec: &str, extra: &str) -> String {
    format!(
        "const __bundle = require(\"../.humanify/_bundle.js\");\n\
         var {binding};\n\
         var initThing = (0, __bundle.__esm)(() => {{\n  {binding} = (0, __bundle.require)(\"{spec}\");\n}});\n\
         function useIt(a) {{\n  return {binding}.join(a, \"x\");\n}}\n{extra}"
    )
}

#[test]
fn the_conventional_name_is_the_packages_own_identifier() {
    let cases = [
        ("path", Some("path")),
        ("node:path", Some("path")),
        ("fs", Some("fs")),
        ("fs/promises", Some("fsPromises")),
        ("node:fs/promises", Some("fsPromises")),
        ("child_process", Some("childProcess")),
        ("path/posix", Some("pathPosix")),
        ("@aws-sdk/client-s3", Some("clientS3")),
        ("string_decoder", Some("stringDecoder")),
        ("module", Some("module")),
        ("./local.js", None),
        ("/$bunfs/root/x.node", None),
        ("bun:ffi", None),
        ("@scope", None),
        ("3d-lib", None),
    ];
    for (spec, want) in cases {
        assert_eq!(conventional_import_name(spec).as_deref(), want, "{spec}");
    }
}

#[test]
fn three_files_each_name_their_path_import_path() {
    let root = std::env::temp_dir().join(format!("humanify-libnames-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let files = ["src/a.js", "src/b.js", "src/c.js"];
    for (i, f) in files.iter().enumerate() {
        let binding = if i == 0 {
            "pathModule".to_string()
        } else {
            format!("pathModule{}", i + 1)
        };
        std::fs::create_dir_all(root.as_path().join("src")).unwrap();
        std::fs::write(root.as_path().join(f), split_file(&binding, "path", "")).unwrap();
    }
    let report = name_library_imports_in_tree(
        root.as_path(),
        &files,
        NeverRename::default(),
        NameProfile::Bun,
    )
    .unwrap();
    assert_eq!(report.named, 3);
    for f in files {
        let text = std::fs::read_to_string(root.as_path().join(f)).unwrap();
        assert!(
            text.contains("path = (0, __bundle.require)(\"path\");"),
            "{text}"
        );
        assert!(text.contains("return path.join(a, \"x\");"), "{text}");
        assert!(!text.contains("pathModule"), "{text}");
    }
    assert_eq!(report.trail.len(), 3);
}

#[test]
fn a_file_binding_its_own_path_gets_path_module() {
    let out = run(&split_file(
        "pathModule54",
        "path",
        "function other(path) {\n  return path;\n}\n",
    ));
    let text = text_of(&out);
    assert!(
        text.contains("pathModule = (0, __bundle.require)(\"path\");"),
        "{text}"
    );
    assert!(text.contains("function other(path)"), "{text}");
    assert_eq!(
        out.named,
        vec![("pathModule54".into(), "pathModule".into())]
    );
}

#[test]
fn node_module_never_shadows_the_commonjs_module() {
    let out = run(&split_file("moduleModule3", "module", ""));
    assert!(
        text_of(&out).contains("nodeModule = (0, __bundle.require)(\"module\");"),
        "{}",
        text_of(&out)
    );
}

#[test]
fn a_global_name_gets_the_module_suffix() {
    let out = run(&split_file("processModule2", "process", ""));
    assert_eq!(
        out.named,
        vec![("processModule2".into(), "processModule".into())]
    );
}

/// `crypto` is on the name-legality owner's global list
/// (`rename::validated::target`), which the naming stage obeys too: never
/// a target, read or not.
#[test]
fn a_listed_global_is_never_a_plain_name() {
    let out = run(&split_file("cryptoModule7", "crypto", ""));
    assert_eq!(
        out.named,
        vec![("cryptoModule7".into(), "cryptoModule".into())]
    );
}

#[test]
fn a_name_the_file_reads_as_a_free_name_gets_the_module_suffix() {
    let out = run(&split_file("osModule4", "os", "console.log(os);\n"));
    assert_eq!(out.named, vec![("osModule4".into(), "osModule".into())]);
}

#[test]
fn two_imports_of_one_package_in_a_file_take_both_plain_names() {
    let text = "const __bundle = require(\"../.humanify/_bundle.js\");\n\
         var fsModule2, fsModule9;\n\
         var initA = (0, __bundle.__esm)(() => {\n  fsModule2 = (0, __bundle.require)(\"fs\");\n});\n\
         var initB = (0, __bundle.__esm)(() => {\n  fsModule9 = (0, __bundle.require)(\"fs\");\n});\n\
         console.log(fsModule2.x, fsModule9.y);\n";
    let out = run(text);
    assert_eq!(
        out.named,
        vec![
            ("fsModule2".into(), "fs".into()),
            ("fsModule9".into(), "fsModule".into()),
        ]
    );
}

#[test]
fn an_exported_import_keeps_its_export_key() {
    let out = run(&split_file(
        "pathModule3",
        "path",
        "Object.defineProperty(module.exports, \"pathModule3\", { get: () => pathModule3, enumerable: true });\nconst bag = { pathModule3 };\n",
    ));
    let text = text_of(&out);
    assert!(
        text.contains("Object.defineProperty(module.exports, \"pathModule3\", { get: () => path,"),
        "{text}"
    );
    assert!(
        text.contains("const bag = { pathModule3: path };"),
        "{text}"
    );
}

#[test]
fn the_free_require_declarator_form_is_named() {
    let out = run("var fsModule2 = require(\"node:fs\");\nconsole.log(fsModule2.readFileSync);\n");
    assert_eq!(
        text_of(&out),
        "var fs = require(\"node:fs\");\nconsole.log(fs.readFileSync);\n"
    );
}

#[test]
fn what_is_not_a_library_import_is_left_alone() {
    // a relative module, a second write, a local `require`, an inner binding
    for text in [
        "var helperModule = require(\"./helper.js\");\nconsole.log(helperModule);\n",
        "var pathModule2 = require(\"path\");\npathModule2 = null;\n",
        "function require(x) {\n  return x;\n}\nvar pathModule2 = require(\"path\");\nconsole.log(pathModule2);\n",
        "function f() {\n  var pathModule2 = require(\"path\");\n  return pathModule2;\n}\nf();\n",
    ] {
        let out = run(text);
        assert!(out.text.is_none() && out.named.is_empty(), "{text}");
    }
}

#[test]
fn a_binding_already_conventional_is_untouched() {
    let out = run("var path = require(\"path\");\nconsole.log(path);\n");
    assert!(out.text.is_none() && out.named.is_empty());
}
