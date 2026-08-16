import type { CredentialStore } from "@iyon/sdk";

export type { CredentialStore } from "@iyon/sdk";

export class CredentialStoreError extends Error {
  constructor(message: string, options?: { readonly cause?: unknown }) {
    super(message, options);
    this.name = "CredentialStoreError";
  }
}

export class MemoryCredentialStore implements CredentialStore {
  private readonly values = new Map<string, string>();

  async get(service: string, account: string): Promise<string | undefined> {
    return this.values.get(key(service, account));
  }

  async set(service: string, account: string, secret: string): Promise<void> {
    this.values.set(key(service, account), secret);
  }

  async delete(service: string, account: string): Promise<void> {
    this.values.delete(key(service, account));
  }

  async has(service: string, account: string): Promise<boolean> {
    return this.values.has(key(service, account));
  }
}

export interface NativeCredentialStore {
  credentialGet(service: string, account: string): string | undefined | Promise<string | undefined>;
  credentialSet(service: string, account: string, secret: string): void | Promise<void>;
  credentialDelete(service: string, account: string): void | Promise<void>;
  credentialHas(service: string, account: string): boolean | Promise<boolean>;
}

export function credentialStoreFromNative(native: NativeCredentialStore): CredentialStore {
  return {
    get: async (service, account) => await native.credentialGet(service, account),
    set: async (service, account, secret) => await native.credentialSet(service, account, secret),
    delete: async (service, account) => await native.credentialDelete(service, account),
    has: async (service, account) => await native.credentialHas(service, account),
  };
}

function key(service: string, account: string): string {
  return `${service}\0${account}`;
}
