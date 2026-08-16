import { chmod, mkdir, readFile, unlink, writeFile } from "node:fs/promises";
import { join } from "node:path";
import type { CredentialStore } from "@iyon/sdk";

export interface CodexCredentials { access: string; refresh: string; expires: number; accountId: string; }
export const CREDENTIAL_SERVICE = "iyon";
export const CREDENTIAL_ACCOUNTS = ["openai-codex", "openai_codex", "openai-codex-responses"] as const;

export async function loadCredentials(options: { readonly credentials?: CredentialStore; readonly filePath?: string } = {}): Promise<CodexCredentials | undefined> {
  if (options.credentials) {
    for (const account of CREDENTIAL_ACCOUNTS) {
      const raw = await options.credentials.get(CREDENTIAL_SERVICE, account);
      const parsed = parseCredentials(raw);
      if (parsed) return parsed;
      if (raw !== undefined) throw credentialsError("stored Codex credentials are malformed");
    }
  }
  const path = options.filePath ?? credentialsFilePath();
  try {
    const raw = await readFile(path, "utf8");
    const parsed = parseCredentials(raw);
    if (!parsed) throw credentialsError("local Codex credentials are malformed");
    return parsed;
  } catch (error) {
    if (isMissing(error)) return undefined;
    throw credentialsError(`failed reading Codex credentials file: ${error instanceof Error ? error.message : "unknown error"}`);
  }
}

export async function saveCredentials(credentials: CodexCredentials, options: { readonly credentials?: CredentialStore; readonly filePath?: string } = {}): Promise<void> {
  const serialized = JSON.stringify({ access: credentials.access, refresh: credentials.refresh, expires: credentials.expires, account_id: credentials.accountId });
  const parsed = parseCredentials(serialized);
  if (!parsed) throw credentialsError("refusing to persist malformed Codex credentials");
  if (options.credentials) {
    await options.credentials.set(CREDENTIAL_SERVICE, CREDENTIAL_ACCOUNTS[0], serialized);
    const verify = parseCredentials(await options.credentials.get(CREDENTIAL_SERVICE, CREDENTIAL_ACCOUNTS[0]));
    if (!verify) throw credentialsError("saved credentials failed verification");
  }
  const path = options.filePath ?? credentialsFilePath();
  await mkdir(join(path, ".."), { recursive: true });
  await writeFile(path, serialized, { mode: 0o600 });
  await chmod(path, 0o600);
  if (!parseCredentials(await readFile(path, "utf8"))) throw credentialsError("saved credentials file failed verification");
}

export async function deleteCredentials(options: { readonly credentials?: CredentialStore; readonly filePath?: string } = {}): Promise<void> {
  for (const account of CREDENTIAL_ACCOUNTS) await options.credentials?.delete(CREDENTIAL_SERVICE, account);
  try { await unlink(options.filePath ?? credentialsFilePath()); } catch (error) { if (!isMissing(error)) throw credentialsError(`failed removing Codex credentials file: ${error instanceof Error ? error.message : "unknown error"}`); }
}

export function credentialsFilePath(): string {
  return process.env.IYON_CODEX_CREDENTIALS_FILE ?? join(process.env.HOME ?? ".", ".config", "iyon", "credentials", "openai-codex.json");
}

export function parseStoredCredentials(value: string | undefined): CodexCredentials | undefined { return parseCredentials(value); }

function parseCredentials(value: string | undefined): CodexCredentials | undefined {
  if (!value) return undefined;
  try {
    const parsed = JSON.parse(value) as Partial<CodexCredentials>;
    const accountId = typeof parsed.accountId === "string" ? parsed.accountId : (parsed as Partial<CodexCredentials> & { account_id?: unknown }).account_id;
    if (typeof parsed.access !== "string" || typeof parsed.refresh !== "string" || typeof parsed.expires !== "number" || typeof accountId !== "string") return undefined;
    return { access: parsed.access, refresh: parsed.refresh, expires: parsed.expires, accountId };
  } catch { return undefined; }
}
function isMissing(error: unknown): boolean { return !!error && typeof error === "object" && (error as { code?: unknown }).code === "ENOENT"; }
function credentialsError(message: string): Error & { readonly kind: "authentication" } { return Object.assign(new Error(message), { kind: "authentication" as const }); }
