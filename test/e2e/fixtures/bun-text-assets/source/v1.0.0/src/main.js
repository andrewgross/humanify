import systemPrompt from "./prompts/system.cjs";
import reviewPrompt from "./prompts/review.cjs";
import yamlish from "./vendor/yamlish.cjs";
import template from "./vendor/template.cjs";
import { stepsReport } from "./steps.js";

function firstLine(text) {
  return text.split("\n")[0];
}

function main() {
  console.log(`system: ${firstLine(systemPrompt)} (${systemPrompt.length})`);
  console.log(`review: ${firstLine(reviewPrompt)} (${reviewPrompt.length})`);
  console.log(`yaml: ${yamlish.load("a, b ,c")}`);
  try {
    yamlish.load(42);
  } catch (e) {
    console.log(`yaml error: ${e.message}`);
  }
  console.log(`dump: ${yamlish.dump({ k: 1 }).length}`);
  console.log(`template: ${template.render("x")}`);
  console.log(stepsReport(2));
}
main();
