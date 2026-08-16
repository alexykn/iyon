import type { ModelApi, ModelErrorKind, ModelRequest, ModelStreamEvent } from "iyon:api";
import type { JsonValue } from "@iyon/sdk";
import { DEFAULT_MODEL, buildRequestBody } from "./serialize.ts";
import { parseSse } from "./sse.ts";
import { createStreamState, normalizeEvent } from "./normalize.ts";

export const DEFAULT_BASE_URL = "https://chatgpt.com/backend-api";
export interface CodexProviderConfig {
  readonly accessToken: string;
  readonly accountId: string;
  readonly model?: string;
  readonly baseUrl?: string;
  readonly fetch?: typeof fetch;
  readonly sleep?: (ms: number) => Promise<void>;
  readonly sessionId?: () => string;
}

export class OpenAICodexProvider implements ModelApi {
  private readonly config: Required<Pick<CodexProviderConfig, "accessToken" | "accountId" | "model" | "baseUrl">> & CodexProviderConfig;

  constructor(config: CodexProviderConfig) {
    this.config = { ...config, model: config.model ?? DEFAULT_MODEL, baseUrl: config.baseUrl ?? DEFAULT_BASE_URL };
  }

  async *stream(request: ModelRequest, options: { readonly signal?: AbortSignal } = {}): AsyncIterable<ModelStreamEvent> {
    const sessionId = request.metadata?.sessionId ?? this.config.sessionId?.() ?? `iyon_${crypto.randomUUID()}`;
    const response = await this.sendWithRetry(request, sessionId, options.signal);
    if (!response.body) throw modelError("Codex returned an empty response stream", "provider");
    const state = createStreamState();
    yield { type: "started" };
    for await (const data of parseSse(response.body)) {
      if (data === "[DONE]") continue;
      let parsed: JsonValue;
      try { parsed = JSON.parse(data) as JsonValue; } catch (error) { throw modelError(`invalid codex event json: ${error instanceof Error ? error.message : "parse failure"}`, "provider"); }
      for (const event of normalizeEvent(parsed, state)) yield event;
    }
    if (state.sawToolCall && state.stopReason === "stop") state.stopReason = "toolUse";
    yield { type: "done", stopReason: state.stopReason };
  }

  private async sendWithRetry(request: ModelRequest, sessionId: string, signal?: AbortSignal): Promise<Response> {
    const fetcher = this.config.fetch ?? fetch;
    const sleep = this.config.sleep ?? ((ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms)));
    let lastError: unknown;
    for (let attempt = 0; attempt <= 3; attempt += 1) {
      try {
        const response = await fetcher(this.endpoint(), {
          method: "POST",
          signal,
          headers: {
            authorization: `Bearer ${this.config.accessToken}`,
            "chatgpt-account-id": this.config.accountId,
            originator: "iyon",
            session_id: sessionId,
            "x-client-request-id": sessionId,
            "openai-beta": "responses=experimental",
            accept: "text/event-stream",
            "content-type": "application/json",
          },
          body: JSON.stringify(buildRequestBody(request, sessionId, this.config.model)),
        });
        if (response.ok) return response;
        const error = await httpError(response);
        if (!retryable(response.status) || attempt === 3) throw error;
        lastError = error;
      } catch (error) {
        lastError = error;
        if (isModelError(error) && !retryableKind(error.kind)) throw error;
        if (attempt === 3) throw error;
      }
      await sleep(400 * 2 ** attempt);
    }
    throw lastError ?? modelError("codex request failed", "unknown");
  }

  private endpoint(): string {
    const base = this.config.baseUrl.replace(/\/$/, "");
    return base.endsWith("/codex/responses") ? base : base.endsWith("/codex") ? `${base}/responses` : `${base}/codex/responses`;
  }
}

export async function createCodexProvider(config: Partial<CodexProviderConfig> & { readonly credentials?: import("@iyon/sdk").CredentialStore } = {}): Promise<OpenAICodexProvider> {
  const { loadValidCredentials } = await import("./auth.ts");
  const credentials = await loadValidCredentials({ credentials: config.credentials, fetch: config.fetch });
  if (!credentials) throw modelError("OpenAI Codex credentials are unavailable", "authentication");
  return new OpenAICodexProvider({ ...config, accessToken: credentials.access, accountId: credentials.accountId, model: config.model ?? DEFAULT_MODEL, baseUrl: config.baseUrl ?? DEFAULT_BASE_URL });
}

async function httpError(response: Response): Promise<Error & { readonly kind: ModelErrorKind }> {
  const body = (await response.text()).slice(0, 512);
  let detail = body;
  try { const parsed = JSON.parse(body) as Record<string, unknown>; if (typeof parsed.detail === "string") detail = parsed.detail; else if (typeof parsed.message === "string") detail = parsed.message; } catch { /* bounded text is enough */ }
  const kind: ModelErrorKind = response.status === 401 || response.status === 403 ? "authentication" : response.status === 429 ? "rateLimited" : response.status === 400 ? "invalidRequest" : response.status >= 500 ? "transport" : "provider";
  return modelError(`codex request failed (${response.status}): ${detail}`, kind);
}
function retryable(status: number): boolean { return status === 429 || status >= 500; }
function retryableKind(kind: ModelErrorKind): boolean { return kind === "transport" || kind === "rateLimited"; }
function isModelError(error: unknown): error is { readonly kind: ModelErrorKind } { return !!error && typeof error === "object" && typeof (error as { kind?: unknown }).kind === "string"; }
function modelError(message: string, kind: ModelErrorKind): Error & { readonly kind: ModelErrorKind } { return Object.assign(new Error(message), { kind }); }
