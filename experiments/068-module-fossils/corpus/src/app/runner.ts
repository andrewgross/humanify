import * as util from "../util";
import env from "../config/env";
import { buildTask } from "./tasks/build";
import runTests from "./tasks/test";
import { aPlusB } from "../cycle/a";
import { doubled } from "../legacy/bridge";

export function runAll(): string {
  const label = util.capitalize(util.kebab(env.name));
  return `${label}:${buildTask()}:${runTests()}:${aPlusB()}:${doubled(2)}`;
}

export async function runLazy(): Promise<string> {
  const feature = await import("../lazy/feature");
  return feature.featureMain();
}
