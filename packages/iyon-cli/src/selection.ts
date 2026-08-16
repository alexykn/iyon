import type { AgentRegistry, AppRegistry, ProviderRegistry } from "@iyon/plugins";
import { selectApp, type SelectedApp } from "@iyon/plugins";
import { selectProvider, type ResolvedProvider } from "@iyon/runtime";

export async function selectIyonProvider(registry: ProviderRegistry, options: { readonly env?: NodeJS.ProcessEnv; readonly config?: unknown; readonly warn?: (warning: { readonly provider: string; readonly message: string }) => void } = {}): Promise<ResolvedProvider> {
  return selectProvider({ registry, ...options });
}

export interface SelectedAgent { readonly id: string; readonly agent: unknown; }

export function selectIyonAgent(registry: AgentRegistry, context: unknown, id = "iyon"): SelectedAgent {
  const registration = registry.lookup(id);
  if (!registration) throw new Error(`selected agent is not registered: ${id}`);
  if (typeof registration.value.create !== "function") throw new Error(`agent contribution cannot create an agent: ${id}`);
  return { id, agent: registration.value.create(context) };
}

export async function selectIyonApp(registry: AppRegistry, context: Record<string, unknown>, id = "iyon"): Promise<SelectedApp> {
  return selectApp(registry, { id, context });
}
