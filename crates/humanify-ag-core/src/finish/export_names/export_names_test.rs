use std::collections::BTreeMap;

use super::follow_export_renames;
use crate::finish::reconcile::PostSplitRename;

fn rename(file: &str, from: &str, to: &str, top_level: bool) -> PostSplitRename {
    PostSplitRename {
        file: file.to_string(),
        from_name: from.to_string(),
        to_name: to.to_string(),
        kind: "test",
        votes: 1,
        top_level,
        key_moved: false,
        locator: None,
    }
}

fn tree(files: &[(&str, &str)]) -> BTreeMap<String, String> {
    files
        .iter()
        .map(|(f, t)| (f.to_string(), t.to_string()))
        .collect()
}

const HOME: &str = "Object.defineProperty(module.exports, \"initNew\", { get: () => initOld, enumerable: true, configurable: true });\nvar initOld = 1;\n";

#[test]
fn an_export_key_and_its_uses_follow_the_renamed_binding() {
    let files = tree(&[
        ("src/a.js", HOME),
        (
            "src/sub/b.js",
            "const a = require(\"../a.js\");\nconst s = \"a.initNew\";\n(0, a.initNew)();\nfunction f(a) {\n  return a.initNew;\n}\n",
        ),
        (
            "src/c.js",
            "const other = require(\"./d.js\");\nother.initNew();\n",
        ),
    ]);
    let followed = follow_export_renames(&[rename("src/a.js", "initNew", "initOld", true)], &files);
    assert_eq!(
        followed.moved.into_iter().collect::<Vec<_>>(),
        vec![("src/a.js".to_string(), "initNew".to_string())]
    );
    assert_eq!(
        followed.changed,
        vec![
            (
                "src/a.js".to_string(),
                HOME.replace("\"initNew\"", "\"initOld\"")
            ),
            (
                "src/sub/b.js".to_string(),
                "const a = require(\"../a.js\");\nconst s = \"a.initNew\";\n(0, a.initOld)();\nfunction f(a) {\n  return a.initNew;\n}\n"
                    .to_string()
            ),
        ]
    );
}

#[test]
fn a_rename_inside_a_function_moves_no_export_key() {
    let files = tree(&[("src/a.js", HOME)]);
    let changed = follow_export_renames(&[rename("src/a.js", "initNew", "initOld", false)], &files);
    assert_eq!(changed, super::Followed::default());
}

#[test]
fn a_key_the_file_already_exports_is_left_alone() {
    let home = format!(
        "Object.defineProperty(module.exports, \"initOld\", {{ get: () => other, enumerable: true, configurable: true }});\n{HOME}"
    );
    let files = tree(&[
        ("src/a.js", home.as_str()),
        ("src/b.js", "const a = require(\"./a.js\");\na.initNew();\n"),
    ]);
    let changed = follow_export_renames(&[rename("src/a.js", "initNew", "initOld", true)], &files);
    assert_eq!(changed, super::Followed::default());
}
