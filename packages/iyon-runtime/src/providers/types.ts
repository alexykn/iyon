import type { ModelApi } from "@iyon/sdk";
import type { ProviderDefinition } from "@iyon/sdk";

export type { ProviderDefinition } from "@iyon/sdk";

export interface ModelSelection {
  readonly provider: string;
  readonly model_id: string;
}

export interface ResolvedProvider {
  readonly definition: ProviderDefinition;
  readonly model: ModelApi;
  readonly selection: ModelSelection;
}

export interface ProviderSelectionWarning {
  readonly provider: string;
  readonly message: string;
}

export class ProviderSelectionError extends Error {
  readonly code = "NO_PROVIDER_REGISTERED";

  constructor(message: string) {
    super(message);
    this.name = "ProviderSelectionError";
  }
}
