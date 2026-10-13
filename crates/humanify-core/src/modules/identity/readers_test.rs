//! The declared-identity readers, one at a time, on the shapes the census
//! found in real bundles (/work/library-identity-2026-10-10/declared-info.md)
//! — including its measured false positives.

use super::readers::{
    self, Claim, ModuleView, aws_sdk_client, export_names, hint_aws_sdk_client, hint_repository,
    hint_version, parse_banner, version_constant,
};
use super::{DeclaredPath, ModuleProvenance, PathKind};
use crate::modules::lex::lex_raw;

fn with<T>(body: &str, provenance: &ModuleProvenance, f: impl Fn(&ModuleView<'_>) -> T) -> T {
    let tokens = lex_raw(body);
    f(&ModuleView {
        body,
        tokens: &tokens,
        provenance,
    })
}

fn read(body: &str, reader: fn(&ModuleView<'_>) -> Option<Claim>) -> Option<Claim> {
    with(body, &ModuleProvenance::default(), reader)
}

// ---- inlined package.json ---------------------------------------------------

#[test]
fn an_inlined_package_json_names_its_package_and_version() {
    let body = r#"(cGg,k1d)=>{k1d.exports={name:"gaxios",version:"7.1.4",description:"A simple common HTTP client.",main:"build/cjs/src/index.js",files:["build/"]}}"#;
    let c = read(body, readers::read_package_json).expect("declared");
    assert_eq!(c.package.as_deref(), Some("gaxios"));
    assert_eq!(c.version.as_deref(), Some("7.1.4"));
    assert_eq!(
        c.member.as_deref(),
        Some("package.json"),
        "the module IS the package's package.json"
    );
}

#[test]
fn a_scoped_package_json_keeps_its_scope() {
    let body = r#"(BYy,qJp)=>{qJp.exports={name:"@grpc/grpc-js",version:"1.14.4",description:"gRPC Library for Node",homepage:"https://grpc.io/",repository:"https://github.com/grpc/grpc-node/tree/master/packages/grpc-js",main:"build/src/index.js"}}"#;
    let c = read(body, readers::read_package_json).unwrap();
    assert_eq!(c.package.as_deref(), Some("@grpc/grpc-js"));
    assert_eq!(c.version.as_deref(), Some("1.14.4"));
}

#[test]
fn a_package_json_in_any_key_order_names_its_package() {
    // sharp's package.json puts `description` between `name` and `version`
    // (Claude Code 2.1.216, the one inlined package.json the first-two-keys
    // rule missed).
    let body = r#"(x0v,GDg)=>{GDg.exports={name:"sharp",description:"High performance Node.js image processing",version:"0.34.5",author:"Lovell Fuller <npm@lovell.info>",homepage:"https://sharp.pixelplumbing.com",main:"lib/index.js"}}"#;
    let c = read(body, readers::read_package_json).expect("declared");
    assert_eq!(c.package.as_deref(), Some("sharp"));
    assert_eq!(c.version.as_deref(), Some("0.34.5"));
    assert_eq!(c.member.as_deref(), Some("package.json"));
    // `version` first, `name` later.
    let body =
        r#"(e,t)=>{t.exports={version:"2.0.1",license:"MIT",name:"@scope/pkg",main:"x.js"}}"#;
    let c = read(body, readers::read_package_json).expect("declared");
    assert_eq!(c.package.as_deref(), Some("@scope/pkg"));
}

#[test]
fn a_nested_name_or_version_is_not_the_package_json_s_own() {
    // `name` and `version` must be keys of the SAME object.
    let body = r#"(e,t)=>{t.exports={name:"x-client",main:"a.js",engines:{version:"1.0.0"}}}"#;
    assert!(read(body, readers::read_package_json).is_none());
}

#[test]
fn package_metadata_inside_a_larger_module_names_the_package_only() {
    let body = r#"(e,t)=>{var n={name:"@azure/msal-common",version:"15.13.1",license:"MIT"};function r(){return n.version}t.exports={meta:n,get:r}}"#;
    let c = read(body, readers::read_package_json).unwrap();
    assert_eq!(c.package.as_deref(), Some("@azure/msal-common"));
    assert_eq!(c.member, None, "not the package.json module itself");
}

#[test]
fn a_name_version_pair_without_any_package_json_key_is_not_a_package() {
    // A dependency table entry, a test record: no other package.json key.
    let body =
        r#"(e,t)=>{t.exports=[{name:"left-pad",version:"1.3.0"},{name:"ms",version:"2.1.3"}]}"#;
    assert!(read(body, readers::read_package_json).is_none());
}

#[test]
fn two_different_package_json_objects_abstain() {
    let body = r#"(e,t)=>{var a={name:"one",version:"1.0.0",main:"a.js"},b={name:"two",version:"2.0.0",main:"b.js"};t.exports=[a,b]}"#;
    assert!(read(body, readers::read_package_json).is_none());
}

#[test]
fn a_non_semver_version_or_a_non_npm_name_is_not_a_package_json() {
    let body = r#"(e,t)=>{t.exports={name:"My App",version:"1.0.0",main:"x.js"}}"#;
    assert!(read(body, readers::read_package_json).is_none());
    let body = r#"(e,t)=>{t.exports={name:"x",version:"latest",main:"x.js"}}"#;
    assert!(read(body, readers::read_package_json).is_none());
}

// ---- banner -----------------------------------------------------------------

fn banner(text: &str) -> Option<Claim> {
    let provenance = ModuleProvenance {
        kept_comments: vec![text.to_string()],
        paths: Vec::new(),
    };
    with("(e,t)=>{}", &provenance, readers::read_banner)
}

#[test]
fn a_package_banner_names_the_package_and_version() {
    let c = banner("! @azure/msal-common v15.13.1 2025-10-29 ").unwrap();
    assert_eq!(c.package.as_deref(), Some("@azure/msal-common"));
    assert_eq!(c.version.as_deref(), Some("15.13.1"));
    let c = banner("! safe-buffer. MIT License. Feross Aboukhadijeh ").unwrap();
    assert_eq!(c.package.as_deref(), Some("safe-buffer"));
}

#[test]
fn a_copyright_banner_names_no_package() {
    assert!(banner("! Copyright 2013 Lovell Fuller and others. ").is_none());
    assert!(
        banner("! Sharp ").is_none(),
        "one word, no version, no shape"
    );
}

#[test]
fn parse_banner_keeps_its_rules() {
    assert_eq!(
        parse_banner("! axios v1.2.3 ").pkg.as_deref(),
        Some("axios")
    );
    assert_eq!(
        parse_banner("! axios v1.2.3 ").version.as_deref(),
        Some("1.2.3")
    );
    assert_eq!(parse_banner("!the end").pkg, None);
}

// ---- highlight.js language definitions -------------------------------------

#[test]
fn a_highlight_js_grammar_declares_its_language_and_aliases() {
    let body = r#"(e,t)=>{function n(r){let o=/([*]|[a-zA-Z_$][a-zA-Z0-9_$]*)/,s={className:"rest_arg",begin:/[.]{3}/,end:o,relevance:10};return{name:"ActionScript",aliases:["as"],keywords:{keyword:["as","break","case"]},contains:[s]}}t.exports=n}"#;
    let c = read(body, readers::read_highlight_js_grammar).unwrap();
    assert_eq!(c.package.as_deref(), Some("highlight.js"));
    assert_eq!(c.member.as_deref(), Some("ActionScript"));
    assert_eq!(c.aliases, vec!["as".to_string()]);
}

#[test]
fn the_last_grammar_assignment_wins() {
    // Arduino inlines the whole C++ grammar, then renames it.
    let body = r#"(e,t)=>{function c(r){return{name:"C++",aliases:["cc","c++"],keywords:{},contains:[]}}function a(r){let n=c(r);n.name="Arduino";n.aliases=["ino"];n.supersetOf="cpp";return n}t.exports=a}"#;
    let c = read(body, readers::read_highlight_js_grammar).unwrap();
    assert_eq!(c.member.as_deref(), Some("Arduino"));
    assert_eq!(c.aliases, vec!["ino".to_string()]);
    // TypeScript inlines JavaScript, then assigns its own object.
    let body = r#"(e,t)=>{function j(r){return{name:"JavaScript",aliases:["js"],keywords:{},contains:[]}}function ts(r){let n=j(r);Object.assign(n,{name:"TypeScript",aliases:["ts","tsx"],keywords:{}});return n}t.exports=ts}"#;
    let c = read(body, readers::read_highlight_js_grammar).unwrap();
    assert_eq!(c.member.as_deref(), Some("TypeScript"));
    assert_eq!(c.aliases, vec!["ts".to_string(), "tsx".to_string()]);
}

#[test]
fn a_name_field_that_is_not_a_grammar_is_not_a_language() {
    // `name:` is a common field (231 modules, 188 grammars): only a
    // grammar-shaped object counts.
    let body = r#"(e,t)=>{t.exports={name:"ValidationError",message:"bad",code:"E1"}}"#;
    assert!(read(body, readers::read_highlight_js_grammar).is_none());
    // A `.name=` assignment with no grammar object is not one either.
    let body = r#"(e,t)=>{function n(){}n.name="Arduino";t.exports=n}"#;
    assert!(read(body, readers::read_highlight_js_grammar).is_none());
    // The core's plain-text pseudo-language puts `name` second.
    let body = r#"(e,t)=>{var p={disableAutodetect:!0,name:"Plain text",contains:[]};t.exports=p}"#;
    assert!(read(body, readers::read_highlight_js_grammar).is_none());
}

#[test]
fn a_declared_name_is_read_with_its_escapes_decoded() {
    // Minified string literals keep non-ASCII as escapes.
    let body = r#"(e,t)=>{function n(r){return{name:"Backus–Naur Form",aliases:["bnf\x21"],contains:[]}}t.exports=n}"#;
    let c = read(body, readers::read_highlight_js_grammar).unwrap();
    assert_eq!(c.member.as_deref(), Some("Backus\u{2013}Naur Form"));
    assert_eq!(c.aliases, vec!["bnf!".to_string()]);
    let body = r#"(e,t)=>{function n(r){return{name:"Cach\xE9 Object Script",aliases:["cls"],keywords:{}}}t.exports=n}"#;
    assert_eq!(
        read(body, readers::read_highlight_js_grammar)
            .unwrap()
            .member
            .as_deref(),
        Some("Caché Object Script")
    );
}

#[test]
fn a_one_letter_language_is_declared() {
    let body = r#"(e,t)=>{function n(r){return{name:"Go",aliases:["golang"],keywords:{},illegal:"</"}}t.exports=n}"#;
    assert_eq!(
        read(body, readers::read_highlight_js_grammar)
            .unwrap()
            .member
            .as_deref(),
        Some("Go")
    );
}

#[test]
fn typescript_renames_the_javascript_grammar_through_object_assign() {
    // highlight.js 11's typescript.js, as Claude Code 2.1.216 ships it: the
    // JavaScript definition, then `Object.assign(r,{name, aliases})` with no
    // keywords of its own.
    let body = r#"(zIR,Plp)=>{function j(e){return{name:"JavaScript",aliases:["js","jsx","mjs","cjs"],keywords:a,illegal:/#/,contains:[]}}function ts(e){let r=j(e);r.contains=r.contains.concat([u]);return Object.assign(r,{name:"TypeScript",aliases:["ts","tsx","mts","cts"]}),r}Plp.exports=ts}"#;
    let c = read(body, readers::read_highlight_js_grammar).unwrap();
    assert_eq!(c.member.as_deref(), Some("TypeScript"));
    assert_eq!(c.aliases, vec!["ts", "tsx", "mts", "cts"]);
}

#[test]
fn a_grammar_of_grammar_keys_only_is_a_grammar() {
    // highlight.js's plaintext.js: no keywords, no contains — but every key
    // is a language-definition key, and one is not `aliases`.
    let body = r#"(tIR,Lsp)=>{function swy(e){return{name:"Plain text",aliases:["text","txt"],disableAutodetect:!0}}Lsp.exports=swy}"#;
    let c = read(body, readers::read_highlight_js_grammar).unwrap();
    assert_eq!(c.member.as_deref(), Some("Plain text"));
    assert_eq!(c.aliases, vec!["text", "txt"]);
    // `{name, aliases}` alone is any registry.
    let body = r#"(e,t)=>{t.exports=[{name:"teleport",aliases:["tp"]}]}"#;
    assert!(read(body, readers::read_highlight_js_grammar).is_none());
    // An `Object.assign` rename with no grammar before it names nothing.
    let body = r#"(e,t)=>{t.exports=Object.assign(c,{name:"teleport",aliases:["tp"]})}"#;
    assert!(read(body, readers::read_highlight_js_grammar).is_none());
}

#[test]
fn a_slash_command_table_is_not_a_grammar() {
    // The LOOSE shape `{name, aliases}` is also Claude Code's own
    // slash-command table: only `{name, …keywords|contains}` is a grammar.
    let body = r#"(e,t)=>{var c=[{name:"teleport",aliases:["tp"],description:"Resume a remote session",isEnabled:()=>!0,type:"local-jsx"},{name:"schedule",aliases:[],description:"Manage schedules",argumentHint:"[name]"}];t.exports=c}"#;
    assert!(read(body, readers::read_highlight_js_grammar).is_none());
    let body =
        r#"(e,t)=>{t.exports={name:"remote-control",aliases:["rc"],illegal:"no",description:"x"}}"#;
    assert!(
        read(body, readers::read_highlight_js_grammar).is_none(),
        "a grammar key after `name` is not enough without keywords or contains"
    );
}

// ---- a scoped package quoting itself ----------------------------------------

#[test]
fn a_scoped_package_quoting_itself_in_an_error_names_it() {
    let body = r#"(e,t)=>{function n(r){if(typeof r!=="string")throw Error("@smithy/util-base64: toBase64 encoder function only accepts string | Uint8Array.");return r}t.exports=n}"#;
    let c = read(body, readers::read_scoped_error_prefix).unwrap();
    assert_eq!(c.package.as_deref(), Some("@smithy/util-base64"));
    let body = r#"(e,t)=>{function r(s,a){if(n[s]){let i=Error(`@opentelemetry/api: Attempted duplicate registration of API: ${s}`);return i}}}"#;
    assert_eq!(
        read(body, readers::read_scoped_error_prefix)
            .unwrap()
            .package
            .as_deref(),
        Some("@opentelemetry/api")
    );
    let body = r#"(e,t)=>{async function f(){throw Error("@aws-sdk/core::resolveAwsSdkSigV4Config - `credentials` missing")}}"#;
    assert_eq!(
        read(body, readers::read_scoped_error_prefix)
            .unwrap()
            .package
            .as_deref(),
        Some("@aws-sdk/core")
    );
}

#[test]
fn a_scoped_name_that_is_not_a_message_prefix_is_not_an_identity() {
    // A dependency table lists names; an install hint names another package.
    let body = r#"(e,t)=>{t.exports={deps:["@smithy/core","@smithy/util-utf8"],hint:"install @aws-sdk/client-s3: it is missing"}}"#;
    assert!(read(body, readers::read_scoped_error_prefix).is_none());
    // A message that quotes a scoped name mid-sentence.
    let body = r#"(e,t)=>{throw Error("please install @smithy/core: missing")}"#;
    assert!(read(body, readers::read_scoped_error_prefix).is_none());
    // Two different packages quoting themselves: abstain.
    let body = r#"(e,t)=>{throw Error("@a/one: x"),Error("@b/two: y")}"#;
    assert!(read(body, readers::read_scoped_error_prefix).is_none());
}

#[test]
fn an_unscoped_message_prefix_is_not_an_identity() {
    // The census: `pkg: …` without a scope names its own package 13 times
    // in 210 (`choices:`, `host:`, `getAliasesFromMetadata:`).
    let body = r#"(e,t)=>{throw Error("choices: must be an array"),Error("commander: bad")}"#;
    assert!(read(body, readers::read_scoped_error_prefix).is_none());
}

// ---- paths the bundler wrote ------------------------------------------------

fn from_paths(paths: &[(PathKind, &str)]) -> Option<Claim> {
    let provenance = ModuleProvenance {
        kept_comments: Vec::new(),
        paths: paths
            .iter()
            .map(|(kind, p)| DeclaredPath {
                kind: *kind,
                path: p.to_string(),
            })
            .collect(),
    };
    with("(e,t)=>{}", &provenance, readers::read_source_path)
}

#[test]
fn a_module_file_path_names_the_package_and_the_file() {
    let c = from_paths(&[(PathKind::ModuleFile, "node_modules/meta-cjs/lib/index.js")]).unwrap();
    assert_eq!(c.package.as_deref(), Some("meta-cjs"));
    assert_eq!(c.member.as_deref(), Some("lib/index.js"));
    let c = from_paths(&[(
        PathKind::ModuleFile,
        "node_modules/@aws-sdk/core/dist-cjs/index.js",
    )])
    .unwrap();
    assert_eq!(c.package.as_deref(), Some("@aws-sdk/core"));
}

#[test]
fn an_inlined_absolute_path_names_the_package_only() {
    let c = from_paths(&[(
        PathKind::Inlined,
        "/home/runner/work/app/node_modules/@grpc/grpc-js/build/src",
    )])
    .unwrap();
    assert_eq!(c.package.as_deref(), Some("@grpc/grpc-js"));
    assert_eq!(c.member, None);
}

#[test]
fn app_paths_and_conflicting_packages_name_nothing() {
    assert!(from_paths(&[(PathKind::ModuleFile, "src/util/x.js")]).is_none());
    assert!(
        from_paths(&[
            (PathKind::Inlined, "/w/node_modules/a/x"),
            (PathKind::Inlined, "/w/node_modules/b/y"),
        ])
        .is_none()
    );
    // A nested node_modules: the innermost package is the module's.
    let c = from_paths(&[(PathKind::Inlined, "/w/node_modules/a/node_modules/b/lib")]).unwrap();
    assert_eq!(c.package.as_deref(), Some("b"));
}

// ---- hints: shown to the model, never a name --------------------------------

fn hint(body: &str, f: fn(&ModuleView<'_>) -> Option<String>) -> Option<String> {
    with(body, &ModuleProvenance::default(), f)
}

#[test]
fn a_repository_url_is_a_hint_not_a_name() {
    assert_eq!(
        hint(
            r#"(e,t)=>{throw Error("see https://github.com/lovell/sharp.git#readme")}"#,
            hint_repository
        )
        .as_deref(),
        Some("repository github.com/lovell/sharp")
    );
    // The census's misnamings: standards domains and property chains are
    // not repositories at all.
    assert_eq!(
        hint(
            r#"(e,t)=>{t.exports={$id:"http://json-schema.org/draft-07/schema#"}}"#,
            hint_repository
        ),
        None
    );
    assert_eq!(
        hint(r#"(e,t)=>{if(x.DepType.DEV)return 1}"#, hint_repository),
        None
    );
    assert_eq!(
        hint(
            r#"(e,t)=>{t.exports=["github.com/a/b","github.com/c/d"]}"#,
            hint_repository
        ),
        None,
        "two repositories: abstain"
    );
}

#[test]
fn a_version_constant_declares_a_version_only() {
    // A TypeScript `version.ts` (@opentelemetry/*): a version, no name.
    let body = r#"(e6n)=>{Object.defineProperty(e6n,"__esModule",{value:!0});e6n.VERSION=void 0;e6n.VERSION="1.9.0"}"#;
    let c = read(body, version_constant).expect("a version");
    assert_eq!(c.version.as_deref(), Some("1.9.0"));
    assert_eq!(c.package, None, "a version never names a package");
    assert_eq!(c.member, None);
    // axios: the literal sits in a variable the export reads.
    let body = r#"(e,t)=>{var Nvt="1.15.2";rB.CancelToken=DSl;rB.isCancel=RLr;rB.VERSION=Nvt;rB.toFormData=x}"#;
    assert_eq!(
        read(body, version_constant).unwrap().version.as_deref(),
        Some("1.15.2")
    );
    // An ES module's export getter (`export { Nvt as VERSION }`).
    let body = r#"(e,t)=>{var Nvt="1.15.2";X(t,{VERSION:()=>Nvt,default:()=>rB})}"#;
    assert_eq!(
        read(body, version_constant).unwrap().version.as_deref(),
        Some("1.15.2")
    );
    // acorn: `e.version=Oo` on its exports.
    let body = r#"(e,t)=>{var Oo="8.15.0";e.version=Oo}"#;
    assert_eq!(
        read(body, version_constant).unwrap().version.as_deref(),
        Some("8.15.0")
    );
}

#[test]
fn a_version_field_or_comparison_is_not_a_version_constant() {
    // An object's `version:` field is a record (an MCP client's own
    // name/version, a build macro object), not the module's version.
    let body = r#"(e,t)=>{var c=new Client({name:"claude-code",version:"1.0.0"});var m={ISSUES:"x",VERSION:"2.1.216"}.VERSION;t.exports=c}"#;
    assert!(read(body, version_constant).is_none());
    // A comparison reads a version, it does not declare one.
    let body = r#"(e,t)=>{var Oar="0.0.0";function f(t){if(t.version===Oar)return[]}}"#;
    assert!(read(body, version_constant).is_none());
    // Two different versions: abstain.
    let body = r#"(e,t)=>{e.VERSION="1.9.0";e.version="2.0.0"}"#;
    assert!(read(body, version_constant).is_none());
}

#[test]
fn a_version_constant_is_a_hint() {
    assert_eq!(
        hint(r#"(e,t)=>{e.VERSION="1.9.0"}"#, hint_version).as_deref(),
        Some("version 1.9.0")
    );
    assert_eq!(
        hint(
            r#"(e,t)=>{e.VERSION="1.9.0";e.version="2.0.0"}"#,
            hint_version
        ),
        None
    );
}

// ---- the export list: a FILE fingerprint ------------------------------------

#[test]
fn the_export_list_reads_the_renamed_exports_parameter() {
    let body = r#"(P,M)=>{Object.defineProperty(P,"__esModule",{value:!0});P.getSSOTokenFromFile=P.tokenIntercept=void 0;P.tokenIntercept=function(){};Object.defineProperty(P,"other",{get:()=>1});if(P.tokenIntercept==null)return}"#;
    assert_eq!(
        export_names(&lex_raw(body)),
        vec!["getSSOTokenFromFile", "tokenIntercept", "other"]
    );
    let body = r#"function(e,t){t.exports.parse=function(){};t.exports.stringify=1}"#;
    assert_eq!(export_names(&lex_raw(body)), vec!["parse", "stringify"]);
}

// ---- AWS SDK v3 client/command pairs: a hint, never a package ---------------

#[test]
fn an_aws_sdk_command_declares_its_client() {
    let body = r#"(e,t)=>{class Q9 extends B.classBuilder().ep(K).m(function(r,o,n,i){return[Y(n,this.serialize,this.deserialize)]}).s("AmazonBedrockControlPlaneService","ListTagsForResource",{}).n("BedrockClient","ListTagsForResourceCommand").f(void 0,void 0).ser(a).de(b).build(){}}"#;
    let c = with(body, &ModuleProvenance::default(), aws_sdk_client).expect("a client");
    assert_eq!(c.client, "BedrockClient");
    assert_eq!(c.commands, vec!["ListTagsForResourceCommand".to_string()]);
    assert_eq!(
        hint(body, hint_aws_sdk_client).as_deref(),
        Some("AWS SDK v3 client BedrockClient (1 command: ListTagsForResourceCommand)")
    );
}

#[test]
fn an_aws_client_name_is_not_its_package() {
    // `STSClient` ships in BOTH @aws-sdk/client-sts and
    // @aws-sdk/nested-clients (the census corpus): the pair names the
    // service, so it is a hint and no identity reader reads it.
    let body = r#"(e,t)=>{class A extends C{}.n("STSClient","AssumeRoleCommand").build(){}}"#;
    assert!(
        super::declare(body, &ModuleProvenance::default())
            .identity
            .is_none()
    );
    // Not a Client/Command pair: nothing.
    for body in [
        r#"(e,t)=>{x.n("BedrockClient","list")}"#,
        r#"(e,t)=>{x.n("Foo","BarCommand")}"#,
        r#"(e,t)=>{var a="BedrockClient",b="ListTagsForResourceCommand"}"#,
    ] {
        assert!(with(body, &ModuleProvenance::default(), aws_sdk_client).is_none());
    }
    // Two clients in one module: abstain.
    let body = r#"(e,t)=>{a.n("STSClient","AssumeRoleCommand");b.n("SSOOIDCClient","CreateTokenCommand")}"#;
    assert!(with(body, &ModuleProvenance::default(), aws_sdk_client).is_none());
}
