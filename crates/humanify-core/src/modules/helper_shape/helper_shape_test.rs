//! The module helper by shape: every form Bun and esbuild write, and the
//! texts that only look like one. The forms are copied from real builds
//! (esbuild 0.27.2 / 0.28.2, Bun 1.3.14, wrangler 3.114.0 re-minified by
//! esbuild 0.28.2 — experiments/095 discovery, Results §5).

use crate::modules::helper_shape::find_module_helper;
use crate::modules::identify_cjs_factory;

fn shape(src: &str) -> (String, u32, bool, bool) {
    let s = find_module_helper(src).expect("module helper recognised");
    (
        s.helper.name.clone(),
        s.helper.start_offset,
        s.guarded,
        s.indirect,
    )
}

/// Bun, and esbuild <= 0.27 `--minify`: the plain arrow — both write it.
#[test]
fn the_plain_minified_arrow() {
    let src = "var T=(t,e)=>()=>(t&&(e=t(t=0)),e);var n=(t,e)=>()=>(e||t((e={exports:{}}).exports,e),e.exports),X=1;";
    let at = src.find("var n=").unwrap() as u32;
    assert_eq!(shape(src), ("n".into(), at, false, false));
    let s = find_module_helper(src).unwrap();
    assert!(!s.esbuild_only());
}

/// esbuild >= 0.28 `--minify`: the thunk guards the call with `try`.
#[test]
fn the_guarded_arrow_of_esbuild_0_28() {
    let src = "var f=(n,e,o)=>()=>{if(o)throw o[0];try{return n&&(e=n(n=0)),e}catch(l){throw o=[l],l}};var a=(n,e)=>()=>{try{return e||n((e={exports:{}}).exports,e),e.exports}catch(o){throw e=0,o}},h=(n,e)=>{};";
    assert_eq!(shape(src).0, "a");
    assert!(shape(src).2, "guarded");
    assert!(find_module_helper(src).unwrap().esbuild_only());
}

/// `--minify-identifiers` alone: renamed, but spaced and on many lines.
#[test]
fn the_spaced_form_of_minify_identifiers() {
    let src = "var g = (n, e) => () => {\n  try {\n    return e || n((e = { exports: {} }).exports, e), e.exports;\n  } catch (o) {\n    throw e = 0, o;\n  }\n};\n";
    assert_eq!(shape(src), ("g".into(), 0, true, false));
}

/// esbuild's unminified runtime: the indirect `(0, cb[…])` call.
#[test]
fn the_unminified_esbuild_runtime() {
    let src = "var __getOwnPropNames = Object.getOwnPropertyNames;\nvar __commonJS = (cb, mod) => function __require() {\n  return mod || (0, cb[__getOwnPropNames(cb)[0]])((mod = { exports: {} }).exports, mod), mod.exports;\n};\n";
    let (name, offset, guarded, indirect) = shape(src);
    assert_eq!(name, "__commonJS");
    assert_eq!(&src[offset as usize..offset as usize + 3], "var");
    assert!(!guarded);
    assert!(indirect);
}

/// wrangler's cli.js re-minified: every helper JOINED into one `var`,
/// the function form, no `;` anywhere before the marker. The helper is
/// the declarator holding the marker, not the statement's first one.
#[test]
fn a_joined_declaration_names_the_declarator_holding_the_marker() {
    let src = "(function(){var cye=Object.create,fp=Object.defineProperty,UD=Object.getOwnPropertyNames,a=(e,t)=>fp(e,\"name\",{value:t,configurable:!0}),q=(e,t)=>function(){return e&&(t=(0,e[UD(e)[0]])(e=0)),t},ee=(e,t)=>function(){return t||(0,e[UD(e)[0]])((t={exports:{}}).exports,t),t.exports},Fn=(e,t)=>{};var x=ee((t,n)=>{n.exports=1});})();";
    let (name, offset, guarded, indirect) = shape(src);
    assert_eq!(name, "ee");
    assert_eq!(&src[offset as usize..offset as usize + 4], ",ee=");
    assert!(!guarded);
    assert!(indirect);
    // The one owner every classification site asks agrees.
    assert_eq!(identify_cjs_factory(src).unwrap().name, "ee");
}

/// Bun's minified runtime: the helper after a destructuring declarator.
#[test]
fn bun_minified_runtime() {
    let src = "var{defineProperty:t,getOwnPropertyNames:p}=Object,b=Object.prototype.hasOwnProperty,s,g=(e,n)=>()=>(n||e((n={exports:{}}).exports,n),n.exports);var h=(e)=>e;";
    assert_eq!(shape(src).0, "g");
    assert!(!find_module_helper(src).unwrap().esbuild_only());
}

/// Texts that only LOOK like the helper are refused.
#[test]
fn look_alikes_are_refused() {
    for src in [
        // A bare marker, no helper around it.
        "var m={exports:{}};m.exports.x=1;",
        // The assigned name is not one of the arrow's parameters.
        "var n=(t,e)=>()=>(z||t((z={exports:{}}).exports,z),z.exports);",
        // The core's tail names a different binding.
        "var n=(t,e)=>()=>(e||t((e={exports:{}}).exports,t),e.exports);",
        // A `;` between the arrow and the marker: another statement's marker.
        "var n=(t,e)=>1;f((e={exports:{}}).exports,e);",
        // A comparison, not a declarator.
        "if(n==(t,e)=>()=>(e||t((e={exports:{}}).exports,e),e.exports))1;",
        // No declaration at all.
        "n=(t,e)=>()=>(e||t((e={exports:{}}).exports,e),e.exports);",
    ] {
        assert!(find_module_helper(src).is_none(), "{src}");
    }
}

/// esbuild's `__export` registrar closes its descriptor after
/// `enumerable`; Bun's goes on with `configurable` and a bound `set`, and
/// `__copyProps`'s getter is an arrow. The FIRST registrar is read.
#[test]
fn the_export_registrars() {
    use crate::modules::helper_shape::{ExportRegistrar, first_export_registrar};
    for src in [
        "h=(n,e)=>{for(var o in e)r(n,o,{get:e[o],enumerable:!0})}",
        "for (var name in all)\n    __defProp(target, name, { get: all[name], enumerable: true });",
        // esbuild's first, Bun's after.
        "h=(n,e)=>{for(var o in e)r(n,o,{get:e[o],enumerable:!0})};for(var o in n)t(e,o,{get:n[o],enumerable:!0,configurable:!0,set:q.bind(n,o)})",
    ] {
        assert_eq!(
            first_export_registrar(src),
            Some(ExportRegistrar::Esbuild),
            "{src}"
        );
    }
    for src in [
        "for(var o in n)t(e,o,{get:n[o],enumerable:!0,configurable:!0,set:q.bind(n,o)})",
        "for (var name in all)\n    __defProp(target, name, {\n      get: all[name],\n      enumerable: true,\n      configurable: true,\n      set: __exportSetter.bind(all, name)\n    });",
        // Bun's first, esbuild's after.
        "for(var o in n)t(e,o,{get:n[o],enumerable:!0,configurable:!0,set:q.bind(n,o)});h=(n,e)=>{for(var o in e)r(n,o,{get:e[o],enumerable:!0})}",
    ] {
        assert_eq!(
            first_export_registrar(src),
            Some(ExportRegistrar::Bun),
            "{src}"
        );
    }
    for src in [
        // esbuild's __copyProps.
        "r(n,t,{get:()=>e[t],enumerable:!(l=b(e,t))||l.enumerable})",
        // A word that only contains `enumerable`.
        "{get:e[o],isenumerable:!0}",
        // `configurable` without the setter is neither.
        "t(e,o,{get:n[o],enumerable:!0,configurable:!0})",
    ] {
        assert_eq!(first_export_registrar(src), None, "{src}");
    }
}

/// The prologue ends where the helper first WRAPS a module (`= NAME(`
/// with a factory): a stray `.f(` or a plain call of a same-named local
/// does not end it.
#[test]
fn the_runtime_prologue_ends_at_the_first_module_wrapper() {
    use crate::modules::helper_shape::runtime_prologue;
    let head = "var f=(t,n)=>()=>(n||t((n={exports:{}}).exports,n),n.exports);var g=x.f(1),h=f(2);";
    for (call, what) in [
        ("var p=f(function(x,y){y.exports=1});", "function factory"),
        ("var p=f((x,y)=>{y.exports=1});", "arrow factory"),
        ("var p=f(x=>{x.a=1});", "one-param arrow"),
        (
            "var p=f({\"a.js\"(x,y){y.exports=1}});",
            "esbuild's keyed factory",
        ),
    ] {
        let src = format!("{head}{call}");
        assert_eq!(
            runtime_prologue(&src),
            &src[..head.len() + "var p=".len()],
            "{what}"
        );
    }
    // No helper, or a helper never called: the whole text.
    assert_eq!(runtime_prologue("var a=1;"), "var a=1;");
    assert_eq!(runtime_prologue(head), head);
}
