import { buildTask } from "./build";
import { leftPad } from "../../vendorish/left-pad";

export default function runTests(): string {
  return leftPad(String(buildTask()), 3);
}
