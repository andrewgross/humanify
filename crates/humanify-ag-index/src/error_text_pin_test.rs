//! The index builder's typed errors print the text the sites printed when
//! their errors were strings: an error's text reaches stderr, a shard's
//! notes, a sidecar's `failed` reasons and the run's notices, so a changed
//! word is a changed output. Each expected string is the format string the
//! site used, filled with the same sample values.

use std::path::{Path, PathBuf};

use humanify_ag_core::libindex::format::{RECIPE_VERSION, UnitRow};
use humanify_ag_core::libindex::recipe::{Recipe, RecipeError, RecipeFault};
use humanify_ag_core::libindex::table::{TableError, TableWriteError};
use humanify_ag_io::subprocess::SubprocessError;

use crate::asof::{AsOfError, PackumentError};
use crate::build::{BuildError, NpmError};
use crate::cache::{BUILD_SIDECAR_SCHEMA, CacheError};
use crate::cli::CommandError;
use crate::error::{at, op_at};
use crate::merge::MergeError;
use crate::planner::{ListError, PlanError};
use crate::recipe::RecipeRunError;
use crate::registry::RegistryError;
use crate::replay::ReplayError;
use crate::shard::ShardError;
use crate::slim::{Malformed, SlimError};
use crate::sourcemap::MapError;
use crate::spec::{SPEC_SCHEMA, SpecError};
use crate::tools::{TOOLS_BEFORE, ToolError};

fn io(why: &str) -> std::io::Error {
    std::io::Error::other(why.to_string())
}

fn json_error() -> serde_json::Error {
    serde_json::from_str::<u8>("x").unwrap_err()
}

fn spawn() -> SubprocessError {
    SubprocessError::Spawn("no such file".into())
}

fn same(got: impl std::fmt::Display, want: &str) {
    assert_eq!(got.to_string(), want);
}

fn p(s: &str) -> PathBuf {
    PathBuf::from(s)
}

#[test]
fn file_and_map_errors_keep_their_words() {
    same(at(Path::new("w/a.map"))(io("gone")), "w/a.map: gone");
    same(
        op_at("mkdir ", Path::new("store/x"))(io("gone")),
        "mkdir store/x: gone",
    );
    same(
        MapError::Json(json_error()),
        &format!("map: {}", json_error()),
    );
    same(MapError::NoSources, "map: no sources");
    same(MapError::BadVlq('!'), "bad VLQ character '!'");
    same(MapError::TruncatedVlq, "truncated VLQ");
    same(Malformed("expected ':'"), "map: expected ':'");
    same(
        SlimError::Unrecordable(p("w/a.map")),
        "w/a.map: a path the journal cannot record",
    );
    same(
        SlimError::Slimmed {
            path: p("w/a.map"),
            source: MapError::NoSources,
        },
        "w/a.map: slimmed map: map: no sources",
    );
    same(
        SlimError::ReadsDifferently(p("w/a.map")),
        "w/a.map: the slimmed map reads differently",
    );
    same(SlimError::NoPathLine(p("j/1-2")), "j/1-2: no path line");
    same(
        SlimError::AnotherFile {
            entry: p("j/1-2"),
            target: p("w/a.map"),
        },
        "j/1-2: w/a.map is another file now",
    );
}

#[test]
fn bundler_and_tool_errors_keep_their_words() {
    same(RecipeRunError::Run(spawn()), &format!("{}", spawn()));
    same(RecipeRunError::Bundler("err\nout".into()), "err\nout");
    same(
        RecipeRunError::Rename("bun output", io("gone")),
        "bun output: gone",
    );
    same(
        RecipeRunError::Version {
            bin: p("t/bun"),
            why: spawn(),
        },
        &format!("t/bun: {}", spawn()),
    );
    same(
        RecipeRunError::VersionFailed(p("t/bun")),
        "t/bun --version failed",
    );
    same(
        ToolError::NotInstalled {
            label: "esbuild",
            tools: p("s/tools"),
            specs: "esbuild@0.25.10".into(),
            dir: p("s/tools/esbuild-0.25.10"),
        },
        &format!(
            "no esbuild under s/tools (by hand: npm install esbuild@0.25.10 --before={TOOLS_BEFORE} --prefix s/tools/esbuild-0.25.10 \
             --ignore-scripts)"
        ),
    );
    let fetch = |why| ToolError::Fetch {
        label: "Bun",
        version: "1.2.0".into(),
        dir: p("s/tools/bun-1.2.0"),
        why: Box::new(why),
    };
    let head = "fetching Bun 1.2.0 into s/tools/bun-1.2.0";
    same(fetch(ToolError::Io(io("gone"))), &format!("{head}: gone"));
    same(
        fetch(ToolError::NpmRun(spawn())),
        &format!("{head}: npm install: {}", spawn()),
    );
    same(
        fetch(ToolError::Npm("E404".into())),
        &format!("{head}: npm install: E404"),
    );
    same(
        fetch(ToolError::Answered {
            bin: p("t/bun"),
            said: "1.1.0".into(),
        }),
        &format!("{head}: t/bun --version answered \"1.1.0\""),
    );
    same(
        fetch(ToolError::NoPlatformBinary(p("t/node_modules/@oven"))),
        &format!("{head}: no platform binary under t/node_modules/@oven"),
    );
    same(
        op_at("placing ", Path::new("t/x/bun"))(io("gone")),
        "placing t/x/bun: gone",
    );
}

#[test]
fn registry_errors_keep_their_words() {
    same(
        PackumentError::Json(json_error()),
        &format!("packument: {}", json_error()),
    );
    same(PackumentError::Missing("`name`"), "registry doc: no `name`");
    same(
        PackumentError::Missing("`versions` map"),
        "registry doc: no `versions` map",
    );
    same(
        PackumentError::Unpublished("gone-pkg".into()),
        "registry says 404 (E404) (unpublished tombstone: gone-pkg)",
    );
    same(
        AsOfError("12/08/2020".into()),
        "--as-of 12/08/2020: expected YYYY-MM-DD or a UTC time YYYY-MM-DDTHH:MM:SS[.mmm]Z",
    );
    same(
        RegistryError::Offline {
            name: "ms".into(),
            path: p("r/ms.json"),
        },
        "ms: no recorded packument at r/ms.json (--offline)",
    );
    same(
        RegistryError::OfflineWave {
            count: 3,
            first: "ms".into(),
        },
        "3 packages are missing from the registry snapshot (--offline), e.g. ms; drop \
         --offline or take the snapshot first",
    );
    let url = "https://registry.npmjs.org/ms";
    same(
        RegistryError::Curl {
            url: url.into(),
            why: spawn(),
        },
        &format!("curl {url}: {}", spawn()),
    );
    same(
        RegistryError::CurlFailed {
            url: url.into(),
            stderr: "reset".into(),
        },
        &format!("curl {url}: reset"),
    );
    same(
        RegistryError::Http {
            url: url.into(),
            code: "503".into(),
        },
        &format!("curl {url}: registry answered HTTP 503"),
    );
    same(
        RegistryError::Doc {
            url: url.into(),
            source: json_error(),
        },
        &format!("curl {url}: {}", json_error()),
    );
    same(
        RegistryError::Retried {
            site: url.into(),
            first: Box::new(RegistryError::CurlFailed {
                url: url.into(),
                stderr: "a".into(),
            }),
            second: Box::new(RegistryError::CurlFailed {
                url: url.into(),
                stderr: "b".into(),
            }),
        },
        &format!("{url}: curl {url}: a; retry: curl {url}: b"),
    );
    same(
        RegistryError::Encode(json_error()),
        &json_error().to_string(),
    );
}

#[test]
fn spec_errors_keep_their_words() {
    same(
        SpecError::Schema("x/1".into()),
        &format!("spec: schema \"x/1\" (this builder reads {SPEC_SCHEMA:?})"),
    );
    same(
        SpecError::AsOf(AsOfError("x".into())),
        "spec: --as-of x: expected YYYY-MM-DD or a UTC time YYYY-MM-DDTHH:MM:SS[.mmm]Z",
    );
    let recipe = RecipeError {
        text: "bun".into(),
        why: RecipeFault::NoColon,
    };
    let recipe_text = recipe.to_string();
    same(
        SpecError::Recipe(recipe),
        &format!("spec: build.recipes: {recipe_text}"),
    );
    same(
        SpecError::RecipeTwice("bun:target=node,format=cjs".into()),
        "spec: build.recipes: bun:target=node,format=cjs given twice",
    );
    same(
        SpecError::SpongeRegions("keep".into()),
        "spec: build.sponge_regions \"keep\" (drop, origin)",
    );
    same(
        SpecError::NotABundler("vite".into()),
        "spec: tools.vite: not a recipe bundler",
    );
    same(
        SpecError::ToolVersion {
            name: "bun".into(),
            version: "x".into(),
        },
        "spec: tools.bun = \"x\": not a version",
    );
    same(
        SpecError::NoTool {
            recipe: "bun:target=node,format=cjs".into(),
            bundler: "bun",
        },
        "spec: recipe bun:target=node,format=cjs needs tools.bun (its pinned version)",
    );
    same(
        SpecError::UnusedTool("rollup"),
        "spec: tools.rollup pins a bundler no recipe uses",
    );
    same(
        SpecError::Invalid {
            ctx: "select \"a\"".into(),
            why: "needs a reason",
        },
        "select \"a\": needs a reason",
    );
    same(
        SpecError::UnknownPolicy {
            ctx: "spec: versions.policy".into(),
            text: "newest".into(),
        },
        "spec: versions.policy: unknown version policy \"newest\" (as-of, latest[-<N>], \
         recent-<N>mo, year-all+minor, year-all+major, year-last<N>+major[<M>])",
    );
    same(
        SpecError::PolicyTwice {
            ctx: "c".into(),
            text: "as-of".into(),
        },
        "c: policy \"as-of\" is listed twice",
    );
    same(
        SpecError::BadId {
            what: "select",
            id: "a b".into(),
        },
        "select id \"a b\": use letters, digits, '-' and '_'",
    );
    same(
        SpecError::IdTwice {
            what: "refuse",
            id: "a".into(),
        },
        "refuse id \"a\" given twice",
    );
    same(
        SpecError::NamePattern {
            ctx: "c".into(),
            pattern: "*".into(),
        },
        "c: name pattern \"*\" (an exact name, or a prefix ending in one '*'; a bare '*' \
         is not a pattern)",
    );
    same(
        SpecError::ListLine {
            file: "top.js",
            line: "x".into(),
        },
        "top.js: expected a quoted name, got \"x\"",
    );
    same(
        SpecError::ListOpen("top.js"),
        "top.js: the list never closes",
    );
    same(
        SpecError::List {
            ctx: "c".into(),
            list: Some("all".into()),
        },
        "c: list Some(\"all\") (top, top-download, top-dependent)",
    );
    same(
        SpecError::List {
            ctx: "c".into(),
            list: None,
        },
        "c: list None (top, top-download, top-dependent)",
    );
    same(
        SpecError::Sha256 {
            ctx: "c".into(),
            sha256: "ab".into(),
        },
        "c: sha256 \"ab\" is not 64 hex digits",
    );
    same(
        SpecError::NotEarlier {
            ctx: "c".into(),
            of: "x".into(),
        },
        "c: `of` names \"x\", not an earlier selection",
    );
    same(
        SpecError::Source {
            ctx: "c".into(),
            given: "pypi".into(),
        },
        "c: source \"pypi\" (npm-high-impact, dependencies)",
    );
    same(
        SpecError::NotNameAtVersion {
            ctx: "c".into(),
            spec: "ms".into(),
        },
        "c: \"ms\" is not name@version",
    );
    same(
        SpecError::NotASelection {
            ctx: "c".into(),
            selection: "x".into(),
        },
        "c: selection \"x\" is not a selection id",
    );
    let toml_error = toml::from_str::<toml::Table>("=").unwrap_err();
    same(
        SpecError::Toml(toml_error.clone()),
        &format!("spec: {toml_error}"),
    );
}

#[test]
fn shard_replay_and_merge_errors_keep_their_words() {
    same(ShardError::Encode(json_error()), &json_error().to_string());
    same(
        ReplayError::Manifest(json_error()),
        &format!("replay: manifest.json: {}", json_error()),
    );
    same(
        ReplayError::RecipeVersion {
            slice: "bun".into(),
            built: "recipe/1".into(),
        },
        &format!(
            "replay: slice bun was built under recipe/1, this binary bundles under {RECIPE_VERSION} — \
             its stored builds are not what this binary would build; make the index from a spec"
        ),
    );
    let recipe = Recipe::parse("bun").unwrap_err();
    let recipe_text = recipe.to_string();
    same(
        ReplayError::Recipe(recipe),
        &format!("replay: {recipe_text}"),
    );
    same(
        MergeError::Stream("write the table", io("full")),
        "write the table: full",
    );
    same(
        op_at("create ", Path::new("units.bun.jsonl"))(io("full")),
        "create units.bun.jsonl: full",
    );
    same(
        MergeError::OutOfOrder("modules/ms@1.0.0".into()),
        "shard modules/ms@1.0.0 added out of label order",
    );
    same(
        MergeError::ShardHash {
            label: "modules/ms@1".into(),
            what: "unit",
            hash: "zz".into(),
        },
        "shard modules/ms@1: unit hash \"zz\" is not 16 lowercase hex digits",
    );
    same(
        MergeError::ShardHash {
            label: "modules/ms@1".into(),
            what: "callee",
            hash: "zz".into(),
        },
        "shard modules/ms@1: callee hash \"zz\" is not 16 lowercase hex digits",
    );
    same(
        MergeError::NamesRowUnheld(0xab),
        "names row 00000000000000ab: no unit row of the slice holds it",
    );
    same(
        MergeError::RowHash("zz".into()),
        "unit row hash \"zz\" is not 16 lowercase hex digits",
    );
    let row = UnitRow {
        hash: "00000000000000ab".into(),
        owner: "ms".into(),
        file: "index.js".into(),
        nodes: 1,
        content: 2,
        module_bytes: 3,
        versions: Vec::new(),
    };
    same(
        MergeError::NotInterned {
            what: "owner",
            row: Box::new(row.clone()),
        },
        &format!("unit row owner {row:?}: the merge never interned it"),
    );
    same(
        MergeError::NameNotInterned("x".into()),
        "names list: name \"x\" is not interned",
    );
    same(
        MergeError::NoUnitRows("bun".into()),
        "names rows for slice bun: the slice has no unit rows",
    );
    let table = TableError::LiteralShort { len: 3 };
    let table_text = table.to_string();
    same(MergeError::Table(table), &table_text);
    same(
        MergeError::TableWrite(Box::new(TableWriteError::Sink(MergeError::GatePoisoned))),
        "the giant-shard gate lock is poisoned",
    );
}

#[test]
fn build_errors_keep_their_words() {
    let late = "npm error code ETARGET ... with a date before 4/27/2026";
    same(
        BuildError::Refuse("ms@2.1.1".into()),
        "--refuse ms@2.1.1: expected name@version:reason",
    );
    same(
        BuildError::Install {
            label: "npm install ms@2.1.1".into(),
            npm: NpmError::Failed("E404".into()),
        },
        "npm install ms@2.1.1: E404",
    );
    same(
        BuildError::Install {
            label: "npm install ms@2.1.1".into(),
            npm: NpmError::Grace(
                Box::new(NpmError::from_stderr(late)),
                Box::new(NpmError::Failed("E404".into())),
            ),
        },
        &format!("npm install ms@2.1.1: {late} / grace: E404"),
    );
    same(
        NpmError::Both(
            Box::new(NpmError::Failed("a".into())),
            Box::new(NpmError::Run(spawn())),
        ),
        &format!("a / {}", spawn()),
    );
    same(
        NpmError::LockChanged(Box::new(NpmError::Failed("a".into()))),
        "a / the lock-guided install changed the lock",
    );
    same(
        NpmError::LockIncomplete,
        "the tree lacks a package its lock lists",
    );
    same(NpmError::Lock(io("gone")), "gone");
    same(
        BuildError::Bundle("Could not resolve".into()),
        "Could not resolve",
    );
    same(BuildError::NoRecipe, "no recipe for slice");
    same(
        BuildError::Stopped,
        "the run stopped the build: its wait deadline (library.index.wait) expired",
    );
    same(
        BuildError::MergeJobs,
        "--merge-jobs 0: the merge takes at least one worker",
    );
    same(
        op_at("", Path::new("entries/req.cjs"))(io("gone")),
        "entries/req.cjs: gone",
    );
}

#[test]
fn cache_plan_and_command_errors_keep_their_words() {
    same(
        CacheError::Sidecar {
            path: p("i/build.json"),
            source: json_error(),
        },
        &format!("i/build.json: {}", json_error()),
    );
    same(
        CacheError::SidecarSchema {
            path: p("i/build.json"),
            schema: "x/0".into(),
        },
        &format!("i/build.json: schema x/0 (this build reads {BUILD_SIDECAR_SCHEMA})"),
    );
    same(
        CacheError::TooMany { count: 9, max: 1 },
        "the bundle reveals 9 package(s), over library.index_build_max_packages = 1: the \
         run-time build refuses to index that much — curate the bundle's evidence or raise \
         the cap",
    );
    same(
        CacheError::Silent {
            name: "ms".into(),
            source: RegistryError::Http {
                url: "u".into(),
                code: "500".into(),
            },
        },
        "the bundle reveals package ms, but the registry answers nothing: curl u: registry \
         answered HTTP 500",
    );
    same(
        CacheError::Build(BuildError::NoRecipe),
        "the run-time index build failed: no recipe for slice",
    );
    same(
        CacheError::NothingIndexed("ms@1: refused: x".into()),
        "the run-time index build failed (install/bundle): no version could be indexed — \
         ms@1: refused: x",
    );
    same(
        CacheError::Move {
            dir: p("s/abc"),
            source: io("busy"),
        },
        "moving the fresh index into place (s/abc): busy",
    );
    same(
        PlanError::NoMetadata("ms".into()),
        "ms: the packument snapshot has no per-version metadata (fetched with the time map \
         since exp184); re-take the snapshot",
    );
    same(
        PlanError::Outage {
            absent: 400,
            total: 1000,
            first: "@a/b".into(),
        },
        "400 of 1000 list members came back absent — an outage, not a tail; first-check the \
         fetcher (a mis-encoding bug 404s a whole class of names) first @a/b",
    );
    same(
        ListError::Pack {
            version: "4.0.0".into(),
            why: "E404".into(),
        },
        "npm pack npm-high-impact@4.0.0: E404",
    );
    same(
        ListError::Tar {
            file: "lib/top.js",
            why: "bad".into(),
        },
        "tar lib/top.js: bad",
    );
    same(
        ListError::Sha {
            version: "4.0.0".into(),
            file: "lib/top.js",
            got: "aa".into(),
            want: "bb".into(),
        },
        "npm-high-impact@4.0.0 lib/top.js: sha256 aa does not match the spec's bb — the spec \
         pins a list this tarball does not contain",
    );
    same(CommandError::MapsFailed(2), "2 maps failed");
    same(
        CommandError::Chunk {
            text: "3/2".into(),
            why: "need 0 <= I < N",
        },
        "--chunk 3/2: need 0 <= I < N",
    );
    same(
        CommandError::SpecTools(ToolError::NoPlatformBinary(p("x"))),
        "spec tools: no platform binary under x",
    );
    same(
        CommandError::NoOut,
        "--out is required unless --dry-run or --chunk",
    );
    same(
        CommandError::SpongeRegions("keep".into()),
        "--sponge-regions \"keep\" (drop, origin)",
    );
    same(CommandError::OneOf, "exactly one of --spec and --replay");
}
