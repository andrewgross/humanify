//! The library-import pass's name rule and precision edges. The driver
//! tests hold the end-to-end behaviour (`naming::driver::driver_test`,
//! the library-import tests).

use oxc_allocator::Allocator;

use super::{library_module_name, name_library_imports};
use crate::rename::transfer::lifecycle::Lifecycle;
use crate::rename::validated::RenameState;
use crate::toolchain::BundleLayout;

#[test]
fn a_specifier_becomes_a_module_name() {
    let cases = [
        ("path", Some("pathModule")),
        ("node:path", Some("pathModule")),
        ("fs/promises", Some("fsPromisesModule")),
        ("child_process", Some("childProcessModule")),
        ("@aws-sdk/client-s3", Some("awsSdkClientS3Module")),
        ("string_decoder", Some("stringDecoderModule")),
        ("./local.js", None),
        ("../up.js", None),
        ("/$bunfs/root/image-processor.node", None),
        ("data:text/plain", None),
        ("3d-lib", None),
        ("", None),
    ];
    for (spec, want) in cases {
        assert_eq!(library_module_name(spec).as_deref(), want, "{spec}");
    }
}

/// `(name before, name after or why)` pairs.
type Pairs = Vec<(String, String)>;

/// Run the pass over `text`; the names it produced and declined.
fn run(text: &str) -> (Pairs, Pairs) {
    let allocator = Allocator::default();
    let ingest = crate::prior::parse_side(&allocator, text, "input.js").expect("parses");
    let json = crate::ingest::program_estree_json(ingest.program);
    let parts = crate::prior::build_side_parts(
        &ingest,
        &json,
        "input.js",
        crate::graph::Eligibility::All,
        BundleLayout::SingleWrapperFunction,
    );
    let semantic = ingest.semantic();
    let mut state = RenameState::new(
        semantic,
        crate::trail::Anchor::Fresh,
        crate::rename::name_profile::NameProfile::Bun,
    );
    let n = parts.graph.module_bindings.len();
    let mut lifecycle = vec![Lifecycle::Pending; n];
    let out = name_library_imports(
        semantic,
        &parts.graph,
        &mut state,
        &mut lifecycle,
        &vec![None; n],
    );
    (out.named, out.declined)
}

/// A local function called `require` is not the program's require.
#[test]
fn a_shadowing_local_require_is_not_a_library_import() {
    let (named, _) = run(
        "var a;\nfunction f() {\n  function require(x) {\n    return x;\n  }\n  a = require(\"path\");\n}\nf();\nconsole.log(a);\n",
    );
    assert!(named.is_empty(), "{named:?}");
}

/// The declarator form, and numbering that skips a name already held.
#[test]
fn the_numbering_skips_a_name_already_held() {
    let (named, _) = run(
        "var pathModule = 1;\nvar a = require(\"path\");\nvar b = require(\"path\");\nconsole.log(a, b, pathModule);\n",
    );
    assert_eq!(
        named,
        vec![
            ("a".to_string(), "pathModule2".to_string()),
            ("b".to_string(), "pathModule3".to_string()),
        ]
    );
}

/// A compound write or an update disqualifies the binding.
#[test]
fn any_other_write_keeps_the_binding_the_models() {
    let (named, _) = run(
        "var a = require(\"path\");\nvar b = require(\"os\");\na += 1;\nb++;\nconsole.log(a, b);\n",
    );
    assert!(named.is_empty(), "{named:?}");
}
