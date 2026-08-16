import { createHash, randomBytes as nodeRandomBytes } from "node:crypto";
import type { ProviderAuthContext, ProviderAuthStatus } from "@iyon/sdk";
import { deleteCredentials, loadCredentials, saveCredentials, type CodexCredentials } from "./credentials.ts";

export const CLIENT_ID = "app_EMoamEEZ73f0CkXaXp7hrann";
export const AUTHORIZE_URL = "https://auth.openai.com/oauth/authorize";
export const TOKEN_URL = "https://auth.openai.com/oauth/token";
export const REDIRECT_URI = "http://localhost:1455/auth/callback";
export const SCOPE = "openid profile email offline_access";

export function pkceChallenge(verifier: string): string { return base64Url(createHash("sha256").update(verifier).digest()); }
export function randomUrlSafe(length = 32, random: (length: number) => Uint8Array = (size) => nodeRandomBytes(size)): string { return base64Url(random(length)); }
export function parseCallback(path: string, expectedState: string): { readonly code: string } {
  const url = new URL(path, "http://localhost");
  if (url.pathname !== "/auth/callback") throw authError("invalid callback path");
  if (url.searchParams.get("state") !== expectedState) throw authError("oauth state mismatch");
  const code = url.searchParams.get("code");
  if (!code) throw authError("missing authorization code");
  return { code };
}

export function accountIdFromAccessToken(token: string): string | undefined {
  try {
    const payload = token.split(".")[1];
    const value = JSON.parse(new TextDecoder().decode(base64Bytes(payload))) as Record<string, unknown>;
    const auth = value["https://api.openai.com/auth"];
    return auth && typeof auth === "object" && typeof (auth as { chatgpt_account_id?: unknown }).chatgpt_account_id === "string" ? (auth as { chatgpt_account_id: string }).chatgpt_account_id : undefined;
  } catch { return undefined; }
}

export async function login(context: ProviderAuthContext): Promise<ProviderAuthStatus> {
  if (!context.callbackServer || !context.openBrowser) throw authError("Codex login requires browser and callback support");
  const verifier = randomUrlSafe(32, (length) => context.randomBytes?.(length) ?? nodeRandomBytes(length));
  const state = randomUrlSafe(32, (length) => context.randomBytes?.(length) ?? nodeRandomBytes(length));
  const url = new URL(AUTHORIZE_URL);
  url.search = new URLSearchParams({ response_type: "code", client_id: CLIENT_ID, redirect_uri: REDIRECT_URI, scope: SCOPE, code_challenge: pkceChallenge(verifier), code_challenge_method: "S256", state }).toString();
  await context.openBrowser(url.toString());
  const callback = await context.callbackServer.listen("/auth/callback", context.signal);
  if (callback.state !== undefined && callback.state !== state) throw authError("oauth state mismatch");
  const response = await (context.fetch ?? fetch)(TOKEN_URL, { method: "POST", headers: { "content-type": "application/x-www-form-urlencoded" }, body: new URLSearchParams({ grant_type: "authorization_code", client_id: CLIENT_ID, code: callback.code, code_verifier: verifier, redirect_uri: REDIRECT_URI }) });
  if (!response.ok) throw authError(`Codex token exchange failed (${response.status})`);
  const token = await response.json() as { access_token?: string; refresh_token?: string; expires_in?: number };
  if (!token.access_token || !token.refresh_token || typeof token.expires_in !== "number") throw authError("Codex token exchange returned incomplete credentials");
  const credentials: CodexCredentials = { access: token.access_token, refresh: token.refresh_token, expires: (context.now?.() ?? Date.now()) + token.expires_in * 1000, accountId: accountIdFromAccessToken(token.access_token) ?? "" };
  if (!credentials.accountId) throw authError("Codex access token did not contain an account id");
  await saveCredentials(credentials, { credentials: context.credentials });
  return { provider: "openai-codex", authenticated: true, accountId: credentials.accountId, expiresAt: credentials.expires, sources: ["credential-store", "file"] };
}

export async function logout(context: ProviderAuthContext): Promise<void> { await deleteCredentials({ credentials: context.credentials }); }

export async function loadValidCredentials(options: { readonly credentials?: import("@iyon/sdk").CredentialStore; readonly fetch?: typeof fetch; readonly now?: () => number; readonly filePath?: string } = {}): Promise<CodexCredentials | undefined> {
  const credentials = await loadCredentials(options);
  if (!credentials || credentials.expires > (options.now?.() ?? Date.now()) + 60_000) return credentials;
  const response = await (options.fetch ?? fetch)(TOKEN_URL, { method: "POST", headers: { "content-type": "application/x-www-form-urlencoded" }, body: new URLSearchParams({ grant_type: "refresh_token", refresh_token: credentials.refresh, client_id: CLIENT_ID }) });
  if (!response.ok) throw authError(`Codex token refresh failed (${response.status})`);
  const token = await response.json() as { access_token?: string; refresh_token?: string; expires_in?: number };
  if (!token.access_token || typeof token.expires_in !== "number") throw authError("Codex token refresh returned incomplete credentials");
  const refreshed: CodexCredentials = { access: token.access_token, refresh: token.refresh_token ?? credentials.refresh, expires: (options.now?.() ?? Date.now()) + token.expires_in * 1000, accountId: accountIdFromAccessToken(token.access_token) ?? credentials.accountId };
  await saveCredentials(refreshed, options);
  return refreshed;
}

export async function status(context: ProviderAuthContext): Promise<ProviderAuthStatus> {
  const credentials = await loadCredentials({ credentials: context.credentials });
  return credentials ? { provider: "openai-codex", authenticated: true, accountId: credentials.accountId, expiresAt: credentials.expires, sources: ["credential-store"] } : { provider: "openai-codex", authenticated: false };
}

function base64Url(value: Uint8Array): string { return Buffer.from(value).toString("base64url"); }
function base64Bytes(value: string): Uint8Array { return Buffer.from(value.replaceAll("-", "+").replaceAll("_", "/"), "base64"); }
function authError(message: string): Error & { readonly kind: "authentication" } { return Object.assign(new Error(message), { kind: "authentication" as const }); }
