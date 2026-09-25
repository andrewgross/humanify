//! The two Babel helpers the explicit-resource-management transform can
//! inject — `@babel/helpers` 7.29.7's `helpers-generated.js` sources,
//! verbatim — as helper DECLARATIONS: parsed, `loc`-less (Babel's
//! `template.program.ast` strips positions, keeping `extra`), the function
//! renamed to its uid, and `_compact` (`file.addHelper`).

use oxc_allocator::Allocator;

use super::ast::{Kind, Node};
use super::convert::Converter;
use crate::ingest::Ingest;

/// `helpers.usingCtx` (minVersion 7.23.9).
pub const USING_CTX_SOURCE: &str = r#"function _usingCtx(){var r="function"==typeof SuppressedError?SuppressedError:function(r,e){var n=Error();return n.name="SuppressedError",n.error=r,n.suppressed=e,n},e={},n=[];function using(r,e){if(null!=e){if(Object(e)!==e)throw new TypeError("using declarations can only be used with objects, functions, null, or undefined.");if(r)var o=e[Symbol.asyncDispose||Symbol.for("Symbol.asyncDispose")];if(void 0===o&&(o=e[Symbol.dispose||Symbol.for("Symbol.dispose")],r))var t=o;if("function"!=typeof o)throw new TypeError("Object is not disposable.");t&&(o=function(){try{t.call(e)}catch(r){return Promise.reject(r)}}),n.push({v:e,d:o,a:r})}else r&&n.push({d:e,a:r});return e}return{e:e,u:using.bind(null,!1),a:using.bind(null,!0),d:function(){var o,t=this.e,s=0;function next(){for(;o=n.pop();)try{if(!o.a&&1===s)return s=0,n.push(o),Promise.resolve().then(next);if(o.d){var r=o.d.call(o.v);if(o.a)return s|=2,Promise.resolve(r).then(next,err)}else s|=1}catch(r){return err(r)}if(1===s)return t!==e?Promise.reject(t):Promise.resolve();if(t!==e)throw t}function err(n){return t=t!==e?new r(n,t):n,next()}return next()}}}"#;

/// `helpers.setFunctionName` (minVersion 7.23.6).
pub const SET_FUNCTION_NAME_SOURCE: &str = r#"function setFunctionName(e,t,n){"symbol"==typeof t&&(t=(t=t.description)?"["+t+"]":"");try{Object.defineProperty(e,"name",{configurable:!0,value:n?n+" "+t:t})}catch(e){}return e}"#;

fn strip_locs(node: &mut Node) {
    node.loc = None;
    for (child, _) in super::transform::children_mut(node) {
        strip_locs(child);
    }
}

/// The helper's declaration node, named `uid`.
pub fn helper_declaration(name: &str, uid: &str) -> Result<Node, String> {
    let source = match name {
        "usingCtx" => USING_CTX_SOURCE,
        "setFunctionName" => SET_FUNCTION_NAME_SOURCE,
        other => return Err(format!("unknown Babel helper {other}")),
    };
    let allocator = Allocator::default();
    let ingest = Ingest::parse(&allocator, source, "helper.js");
    if !ingest.errors.is_empty() {
        return Err(format!("helper {name} does not parse"));
    }
    let program = Converter::new(source).program(ingest.program)?;
    let Kind::Program { mut body, .. } = program.kind else {
        return Err("helper: not a program".into());
    };
    let mut decl = body.pop().ok_or("helper: empty")?;
    strip_locs(&mut decl);
    let Kind::FunctionDeclaration(f) = &mut decl.kind else {
        return Err("helper: not a function declaration".into());
    };
    f.id = Some(Box::new(Node::ident(uid)));
    decl.compact = true;
    Ok(decl)
}
