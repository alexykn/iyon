import type { SourceMetadata } from "./contributions.ts";
import type { App, AppContext, AppRegistry } from "./app-registry.ts";
import type { EventHub } from "./events.ts";

export interface AppSelectionInput {
  readonly id?: string;
  readonly defaultId?: string;
  readonly context?: AppContext;
}

export interface SelectedApp {
  readonly id: string;
  readonly app: App;
  readonly source: SourceMetadata;
}

export async function selectApp(registry: AppRegistry, input: AppSelectionInput = {}, events?: EventHub): Promise<SelectedApp> {
  const id = input.id ?? input.defaultId ?? registry.list()[0]?.id;
  if (!id) throw new Error("no app is registered for selection");
  const registration = registry.lookup(id);
  if (!registration) throw new Error(`selected app is not registered: ${id}`);
  const app = await registration.value.create(input.context ?? {}) as App;
  events?.emit("app-selection", { appId: id, source: registration.source });
  return { id, app, source: registration.source };
}
