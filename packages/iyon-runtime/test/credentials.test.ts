import { describe, expect, test } from "bun:test";
import { MemoryCredentialStore, credentialStoreFromNative } from "../src/credentials.ts";

describe("generic credential store", () => {
  test("round trips, overwrites, reports missing values, and deletes idempotently", async () => {
    const store = new MemoryCredentialStore();
    expect(await store.has("service", "account")).toBe(false);
    await store.set("service", "account", "secret");
    expect(await store.get("service", "account")).toBe("secret");
    await store.set("service", "account", "replacement");
    expect(await store.get("service", "account")).toBe("replacement");
    await store.delete("service", "account");
    await store.delete("service", "account");
    expect(await store.get("service", "account")).toBeUndefined();
  });

  test("adapts native operations without logging or transforming secrets", async () => {
    let value: string | undefined;
    const store = credentialStoreFromNative({
      credentialGet: () => value,
      credentialSet: (_service, _account, secret) => { value = secret; },
      credentialDelete: () => { value = undefined; },
      credentialHas: () => value !== undefined,
    });
    await store.set("opaque", "opaque", "do-not-log");
    expect(await store.get("opaque", "opaque")).toBe("do-not-log");
    await store.delete("opaque", "opaque");
    expect(await store.has("opaque", "opaque")).toBe(false);
  });
});
