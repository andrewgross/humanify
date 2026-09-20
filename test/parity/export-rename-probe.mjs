import * as babel from "@babel/core";
import * as babelGenerator from "@babel/generator";

const code =
  "export const foo = (e) => { return e; };\nexport { foo as bar };\n";
const ast = babel.parseSync(code, {
  sourceType: "module",
  configFile: false,
  babelrc: false
});

// Babel's own renamer (the fallback path the pipeline uses for
// export-involved bindings):
babel.traverse(ast, {
  Scope(p) {
    const b = p.scope.bindings.foo;
    if (b) {
      console.log("found binding foo, kind:", b.kind);
      p.scope.rename("foo", "createEventEmitter");
      p.scope.rename("bar", "baz");
    }
  }
});
const out = babelGenerator.generate(ast, { compact: false }).code;
console.log("=== after scope.rename:");
console.log(out);
