import { register } from "../core/registry";

export function featureMain(): string {
  register("feature");
  return "feature-ran";
}
