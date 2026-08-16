import type { ProviderCapabilities, ProviderModel } from "@iyon/sdk";
import type { ReasoningLevel } from "iyon:api";

export const CODEX_REASONING_LEVELS: readonly ReasoningLevel[] = ["none", "minimal", "low", "medium", "high", "xhigh", "max"];
export const CODEX_MODELS: readonly ProviderModel[] = [{ id: "gpt-5.3-codex", name: "GPT-5.3 Codex", capabilities: { reasoning: CODEX_REASONING_LEVELS, tools: true, streaming: true, vision: true } }];
export const CODEX_CAPABILITIES: ProviderCapabilities = CODEX_MODELS[0].capabilities!;
