import { bValue } from "./b";

export function aValue(): number {
  return 1;
}
export function aPlusB(): number {
  return aValue() + bValue();
}
