import type { Disposable } from "./disposable.ts";
import type { RegisteredContribution, SourceMetadata } from "./contributions.ts";

export interface ExtensionEvents {
  activation: { readonly packageId: string; readonly extensionId: string; readonly source: SourceMetadata };
  "activation-failure": { readonly packageId: string; readonly extensionId: string; readonly source: string; readonly error: unknown };
  registration: { readonly contribution: RegisteredContribution<any> };
  replacement: { readonly contribution: RegisteredContribution<any> };
  unload: { readonly contribution: RegisteredContribution<any> };
  "app-selection": { readonly appId: string; readonly source: SourceMetadata };
  "scene-selection": { readonly phase: "compose" | "replace"; readonly source: SourceMetadata };
}

export type ExtensionHandler<E extends keyof ExtensionEvents> = (event: ExtensionEvents[E]) => void;

export class EventHub {
  private readonly handlers = new Map<keyof ExtensionEvents, ExtensionHandler<any>[]>();
  private readonly observerErrors: unknown[] = [];

  on<E extends keyof ExtensionEvents>(event: E, handler: ExtensionHandler<E>): Disposable {
    const handlers = this.handlers.get(event) ?? [];
    handlers.push(handler);
    this.handlers.set(event, handlers);
    let active = true;
    return { dispose: () => { if (!active) return; active = false; const index = handlers.indexOf(handler); if (index >= 0) handlers.splice(index, 1); } };
  }

  emit<E extends keyof ExtensionEvents>(event: E, value: ExtensionEvents[E]): void {
    for (const handler of [...(this.handlers.get(event) ?? [])]) {
      try { handler(value); } catch (error) { this.observerErrors.push(error); }
    }
  }

  get errors(): readonly unknown[] { return this.observerErrors; }
}
