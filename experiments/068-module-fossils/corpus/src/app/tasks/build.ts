import { MAX_RETRIES } from "../../config/constants";
import { clamp } from "../../util/math";

export function buildTask(): number {
  return clamp(MAX_RETRIES, 0, 5);
}
