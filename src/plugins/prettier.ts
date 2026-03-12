import prettier from "prettier";
import type { Profiler } from "../profiling/profiler.js";
import { NULL_PROFILER } from "../profiling/profiler.js";

export function createPrettierPlugin(options?: { profiler?: Profiler }) {
  const profiler = options?.profiler ?? NULL_PROFILER;
  return async (code: string): Promise<string> => {
    const span = profiler.startSpan("prettier", "pipeline");
    const result = await prettier.format(code, { parser: "babel" });
    span.end();
    return result;
  };
}

const prettierFormat = async (code: string): Promise<string> =>
  prettier.format(code, { parser: "babel" });
