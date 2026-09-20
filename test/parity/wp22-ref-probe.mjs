// probe: babel ground truth for collectReferencedBindingIds — which
// identifier positions count as isReferencedIdentifier, and which node a
// redeclared binding's path points at (the holdingBinding guard).
import { parseSync } from "@babel/core";
import traverseMod from "@babel/traverse";
const traverse = traverseMod.default ?? traverseMod;

const cases = {
  "simple assign": "var mb = 1; function f() { mb = 2; }",
  "compound assign": "var mb = 1; function f() { mb += 2; }",
  update: "var mb = 1; function f() { mb++; }",
  "destructure write": "var mb = 1; function f() { ({x: mb} = o); }",
  "destructure shorthand write": "var mb = 1; function f() { ({mb} = o); }",
  "for-of write": "var mb = 1; function f() { for (mb of xs) {} }",
  "key not ref": "var mb = 1; function f() { ({mb: 1}); }"
};

for (const [label, code] of Object.entries(cases)) {
  const ast = parseSync(code, { sourceType: "script", filename: "input.js" });
  const out = [];
  traverse(ast, {
    Identifier(path) {
      if (path.node.name !== "mb") return;
      out.push(`${path.node.start}:${path.isReferencedIdentifier()}`);
    }
  });
  console.log(label, "->", out.join(" "));
}

// redeclared binding: which declarator does binding.path point at?
const ast2 = parseSync("var x; var x = () => 1; var x = 2;", {
  sourceType: "script",
  filename: "input.js"
});
traverse(ast2, {
  VariableDeclarator(path) {
    const name = path.node.id.name;
    if (name !== "x") return;
    const binding = path.scope.getBinding("x");
    console.log(
      "redecl: declarator@" + path.node.start,
      "binding.path@" + (binding ? binding.path.node.start : "null")
    );
  }
});
// function declaration redeclare
const ast3 = parseSync("function g(){} function g(){ return 1; }", {
  sourceType: "script",
  filename: "input.js"
});
traverse(ast3, {
  FunctionDeclaration(path) {
    const binding =
      path.parentPath.scope.getBinding("g") ?? path.scope.getBinding("g");
    console.log(
      "fndecl@" + path.node.start,
      "binding.path===" +
        (binding && binding.path.node === path.node
          ? "SELF"
          : "other@" + (binding ? binding.path.node.start : "null"))
    );
  }
});
// named function expression: holdingBinding's arm 2
const ast4 = parseSync("var h = function named() { return 1; };", {
  sourceType: "script",
  filename: "input.js"
});
traverse(ast4, {
  FunctionExpression(path) {
    const parent = path.parentPath;
    if (parent.isVariableDeclarator()) {
      const binding = parent.scope.getBinding(parent.node.id.name);
      console.log(
        "namedfn@" + path.node.start,
        "holding===" +
          (binding && binding.path.node === parent.node
            ? "DECLARATOR"
            : "other")
      );
    }
    const self = path.scope.getBinding("named");
    console.log(
      "namedfn self-name binding scope is own scope:",
      self ? "yes" : "no"
    );
  }
});
