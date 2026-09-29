/**
 * The deterministic stub LLM — the OpenAI-compatible chat-completions
 * server the e2e gate and the ask-trace harness share.
 *
 * An answer policy derives the response ONLY from the request bytes (never
 * randomness, never the clock), so a pipeline run against the stub is
 * reproducible end to end: the pipeline is completion-order-independent by
 * design, and the stub answers deterministically.
 *
 * The request's user prompt carries one "Identifiers to rename: a, b" line
 * per naming ask (crates/humanify-core/src/naming/prompts.ts, batch and
 * module builders alike), so the policy has everything it needs.
 */
import * as http from "node:http";
import type { AddressInfo } from "node:net";

/** Every identifier a naming prompt asks for, in the order it listed them. */
export function askedIdentifiers(requestBody: string): string[] {
  let parsed: { messages?: Array<{ content?: string }> };
  try {
    parsed = JSON.parse(requestBody);
  } catch {
    return [];
  }
  for (const m of parsed.messages ?? []) {
    const hit = /Identifiers to rename: ([^\n]*)/.exec(m.content ?? "");
    if (hit) {
      return hit[1]
        .split(",")
        .map((s) => s.trim())
        .filter(Boolean);
    }
  }
  return [];
}

/**
 * The default answer policy (the e2e gate's): every asked identifier maps
 * to `<id>Renamed` — every rename lands, nothing collides.
 */
export function stubAnswer(requestBody: string): string {
  const out: Record<string, string> = {};
  for (const id of askedIdentifiers(requestBody)) {
    if (/^[A-Za-z_$][\w$]*$/.test(id)) out[id] = `${id}Renamed`;
  }
  return JSON.stringify(out);
}

/**
 * A collision-forcing answer policy (ask-trace's `--collide <name>`): every
 * asked identifier gets the SAME name. Within one batch that raises the
 * duplicate failure (the lane loop's disclosed round-2); across lanes and
 * nodes it forces the barrier collision retry — the 2026-09-28
 * collision-retry path, stub-verifiable with zero LLM calls.
 */
export function collideAnswer(name: string): (requestBody: string) => string {
  return (requestBody: string) => {
    const out: Record<string, string> = {};
    for (const id of askedIdentifiers(requestBody)) {
      if (/^[A-Za-z_$][\w$]*$/.test(id)) out[id] = name;
    }
    return JSON.stringify(out);
  };
}

export interface StubLlm {
  server: http.Server;
  /** The `--endpoint` value the pipeline takes. */
  url: string;
}

/** The stub on an ephemeral localhost port. Caller closes `server`. */
export function startStubLlm(
  answer: (requestBody: string) => string = stubAnswer
): Promise<StubLlm> {
  const server = http.createServer((req, res) => {
    let body = "";
    req.on("data", (c) => {
      body += c;
    });
    req.on("end", () => {
      res.setHeader("content-type", "application/json");
      res.end(
        JSON.stringify({
          id: "stub",
          object: "chat.completion",
          created: 0,
          model: "stub",
          choices: [
            {
              index: 0,
              finish_reason: "stop",
              message: { role: "assistant", content: answer(body) }
            }
          ],
          usage: { prompt_tokens: 1, completion_tokens: 1, total_tokens: 2 }
        })
      );
    });
  });
  return new Promise((resolve) =>
    server.listen(0, "127.0.0.1", () => {
      const { port } = server.address() as AddressInfo;
      resolve({ server, url: `http://127.0.0.1:${port}/v1` });
    })
  );
}
