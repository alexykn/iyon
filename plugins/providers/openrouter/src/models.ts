import type { ReasoningLevel } from "iyon:api";
import type { JsonValue, ProviderCapabilities, ProviderModel } from "@iyon/sdk";

export const ALL_REASONING_LEVELS: readonly ReasoningLevel[] = ["none", "minimal", "low", "medium", "high", "xhigh", "max"];

export function parseModelCatalog(value: unknown): ProviderModel[] {
  if (!value || typeof value !== "object" || Array.isArray(value)) throw catalogError("model catalog must be an object");
  const data = value as { data?: unknown };
  if (!Array.isArray(data.data)) throw catalogError("model catalog is missing data");
  return data.data.map((entry, index) => {
    if (!entry || typeof entry !== "object" || Array.isArray(entry) || typeof (entry as { id?: unknown }).id !== "string") throw catalogError(`invalid model at index ${index}`);
    const item = entry as Record<string, unknown>;
    return { id: item.id as string, ...(typeof item.name === "string" ? { name: item.name } : {}), capabilities: capabilitiesFromCatalog(item) };
  });
}

export function capabilitiesFromCatalog(model: Record<string, unknown>): ProviderCapabilities {
  const reasoning = model.reasoning;
  if (!reasoning || typeof reasoning !== "object" || Array.isArray(reasoning)) return { reasoning: ALL_REASONING_LEVELS, tools: true, streaming: true, vision: true };
  const efforts = (reasoning as { supported_efforts?: unknown }).supported_efforts;
  if (!Array.isArray(efforts)) return { reasoning: ALL_REASONING_LEVELS, tools: true, streaming: true, vision: true };
  const normalized = efforts.filter((effort): effort is ReasoningLevel => typeof effort === "string" && ALL_REASONING_LEVELS.includes(effort as ReasoningLevel));
  return { reasoning: normalized, tools: true, streaming: true, vision: true };
}

export async function discoverModels(options: { readonly baseUrl?: string; readonly fetch?: typeof fetch; readonly signal?: AbortSignal } = {}): Promise<readonly ProviderModel[]> {
  const response = await (options.fetch ?? fetch)(`${(options.baseUrl ?? "https://openrouter.ai/api/v1").replace(/\/$/, "")}/models`, { signal: options.signal });
  if (!response.ok) throw catalogError(`model catalog request failed (${response.status})`);
  let value: JsonValue;
  try { value = await response.json() as JsonValue; } catch { throw catalogError("model catalog returned invalid JSON"); }
  return parseModelCatalog(value);
}

function catalogError(message: string): Error & { readonly kind: "provider" } { return Object.assign(new Error(message), { kind: "provider" as const }); }
