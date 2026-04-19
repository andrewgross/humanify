import { join } from "node:path";
import type { CorpusItem } from "./types.js";

const FIXTURES_ROOT = join(
  import.meta.dirname,
  "..",
  "..",
  "test",
  "e2e",
  "fixtures"
);

export const CORPUS: CorpusItem[] = [
  {
    id: "r1b-synthetic",
    sourcePath: join(
      FIXTURES_ROOT,
      "r1b-synthetic",
      "build",
      "v1.0.0",
      "build",
      "index.js"
    ),
    description: "13-function store with paired identical helpers"
  },
  {
    id: "disambiguation",
    sourcePath: join(
      FIXTURES_ROOT,
      "disambiguation",
      "build",
      "v1.0.0",
      "build",
      "index.js"
    ),
    description: "13-function store (identical to r1b-synthetic v1)"
  }
];

export function getCorpusItem(id: string): CorpusItem {
  const item = CORPUS.find((c) => c.id === id);
  if (!item) throw new Error(`Unknown corpus item: ${id}`);
  return item;
}
