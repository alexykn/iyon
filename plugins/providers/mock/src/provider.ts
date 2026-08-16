import type { ModelApi, ModelMessage, ModelRequest, ModelStreamEvent } from "iyon:api";

export interface MockProviderConfig {
  readonly initialDelayMs?: number;
  readonly chunkDelayMs?: number;
}

export class MockProvider implements ModelApi {
  private readonly initialDelayMs: number;
  private readonly chunkDelayMs: number;

  constructor(config: MockProviderConfig = {}) {
    this.initialDelayMs = config.initialDelayMs ?? 1_000;
    this.chunkDelayMs = config.chunkDelayMs ?? 20;
  }

  async *stream(request: ModelRequest, options: { readonly signal?: AbortSignal } = {}): AsyncIterable<ModelStreamEvent> {
    const signal = options.signal;
    await wait(this.initialDelayMs, signal);
    if (signal?.aborted) {
      yield { type: "done", stopReason: "aborted" };
      return;
    }

    const prompt = lastUserText(request.messages) ?? "there";
    const response = `Mock response to: ${prompt}`;
    yield { type: "started" };
    yield { type: "textStart", contentIndex: 0 };

    for (const chunk of splitInclusiveSpace(response)) {
      await wait(this.chunkDelayMs, signal);
      if (signal?.aborted) {
        yield { type: "done", stopReason: "aborted" };
        return;
      }
      yield { type: "textDelta", contentIndex: 0, delta: chunk };
    }

    yield { type: "textEnd", contentIndex: 0, text: response };
    yield { type: "done", stopReason: "stop" };
  }
}

function lastUserText(messages: readonly ModelMessage[]): string | undefined {
  for (let index = messages.length - 1; index >= 0; index -= 1) {
    const message = messages[index];
    if (message.role !== "user") continue;
    const text = message.content
      .filter((block): block is Extract<typeof block, { type: "text" }> => block.type === "text")
      .map((block) => block.text)
      .join(" ");
    if (text.length > 0) return text;
  }
  return undefined;
}

function splitInclusiveSpace(value: string): string[] {
  return value.match(/.*?\s|.+$/g) ?? [];
}

async function wait(ms: number, signal?: AbortSignal): Promise<void> {
  if (ms <= 0 || signal?.aborted) return;
  await new Promise<void>((resolve, reject) => {
    const timer = setTimeout(resolve, ms);
    const abort = () => {
      clearTimeout(timer);
      resolve();
    };
    signal?.addEventListener("abort", abort, { once: true });
  }).catch(() => undefined);
}
