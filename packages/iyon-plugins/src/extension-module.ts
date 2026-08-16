import { ActivationError } from "./errors.ts";
import type { Disposable } from "./disposable.ts";
import type { ExtensionAPI } from "./extension-api.ts";

export interface ExtensionModule {
  readonly activate: (api: ExtensionAPI) => void | Disposable | Promise<void | Disposable>;
}

export async function importExtension(entrypoint: string, details: { readonly packageId: string; readonly extensionId: string }): Promise<ExtensionModule> {
  let imported: unknown;
  try {
    imported = await import(entrypoint);
  } catch (error) {
    throw new ActivationError(`failed to import ${details.packageId}/${details.extensionId} at ${entrypoint}`, { ...details, entrypoint }, error);
  }
  if (!imported || typeof imported !== "object" || typeof (imported as { activate?: unknown }).activate !== "function") {
    throw new ActivationError(`extension ${details.packageId}/${details.extensionId} at ${entrypoint} must export exactly one activate(api) function`, { ...details, entrypoint });
  }
  return imported as ExtensionModule;
}
