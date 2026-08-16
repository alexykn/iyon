import type { ModelApi, ModelRequest, ModelStreamEvent } from "iyon:api";
import type { ExtensionAPI } from "iyon:plugins";
import type { KernelSession, ModelTurnResult } from "@iyon/sdk";

export interface CustomAgentContext {
  readonly session: KernelSession;
  readonly model: ModelApi;
  readonly signal: AbortSignal;
  readonly tools?: { list(): readonly { readonly value: unknown }[] };
}

export class CustomAgent {
  constructor(private readonly context: CustomAgentContext) {}

  async run(): Promise<ModelTurnResult> {
    const request = buildCustomRequest(this.context);
    const turn = this.context.session.beginModelTurn({ request }) as unknown as PublicTurn;
    const stream = await this.context.model.stream(request, { signal: this.context.signal });
    for await (const event of stream) await turn.push(event, this.context.signal);
    return turn.finish();
  }
}

export function activate(api: ExtensionAPI): void {
  api.agents.register({ id: "custom", create: (context) => new CustomAgent(context as CustomAgentContext) });
}

export function buildCustomRequest(context: CustomAgentContext): ModelRequest {
  const entries = context.session.snapshot().entries.filter((entry) => entry.kind === "message" && entry.role === "user");
  const last = entries.at(-1);
  const messages = last && last.role === "user" ? [{ role: "user" as const, content: last.content }] : [];
  const tools = (context.tools?.list() ?? [])
    .map((entry) => entry.value)
    .filter(isModelTool)
    .slice(0, 1)
    .map((tool) => ({ name: tool.name, description: tool.description, inputSchema: tool.inputSchema }));
  return { systemPrompt: "custom-agent-only", messages, tools, params: {}, metadata: { sessionId: String(context.session.snapshot().sessionId) } };
}

interface PublicTurn {
  push(event: ModelStreamEvent, signal?: AbortSignal): Promise<void>;
  finish(): Promise<ModelTurnResult>;
}

function isModelTool(value: unknown): value is { name: string; description: string; inputSchema: import("@iyon/sdk").JsonValue } {
  return !!value && typeof value === "object" && typeof (value as { name?: unknown }).name === "string" && typeof (value as { description?: unknown }).description === "string" && "inputSchema" in value;
}
