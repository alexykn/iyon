import type {
  ModelApi,
  ModelError,
  ModelRequest,
  ModelStreamEvent,
  ReasoningLevel,
} from "./api.ts";

export interface CredentialStore {
  get(service: string, account: string): Promise<string | undefined>;
  set(service: string, account: string, secret: string): Promise<void>;
  delete(service: string, account: string): Promise<void>;
  has(service: string, account: string): Promise<boolean>;
}

export interface ProviderLogger {
  debug?(message: string, details?: Readonly<Record<string, unknown>>): void;
  info?(message: string, details?: Readonly<Record<string, unknown>>): void;
  warn?(message: string, details?: Readonly<Record<string, unknown>>): void;
  error?(message: string, details?: Readonly<Record<string, unknown>>): void;
}

export interface ProviderAuthContext {
  readonly credentials: CredentialStore;
  readonly signal?: AbortSignal;
  readonly logger?: ProviderLogger;
  readonly prompt?: (question: string) => Promise<string>;
  readonly openBrowser?: (url: string) => Promise<void>;
  readonly callbackServer?: ProviderCallbackServer;
  readonly fetch?: typeof fetch;
  readonly now?: () => number;
  readonly randomBytes?: (length: number) => Uint8Array;
}

export interface ProviderCallbackServer {
  listen(path: string, signal?: AbortSignal): Promise<{ readonly code: string; readonly state?: string }>;
  close(): Promise<void>;
}

export interface ProviderAuthStatus {
  readonly provider: string;
  readonly authenticated: boolean;
  readonly accountId?: string;
  readonly expiresAt?: number;
  readonly sources?: readonly string[];
}

export interface ProviderAuthHooks {
  login?(context: ProviderAuthContext): Promise<ProviderAuthStatus>;
  logout?(context: ProviderAuthContext): Promise<void>;
  status?(context: ProviderAuthContext): Promise<ProviderAuthStatus>;
}

export interface ProviderCapabilities {
  readonly reasoning: readonly ReasoningLevel[];
  readonly vision?: boolean;
  readonly tools?: boolean;
  readonly streaming?: boolean;
}

export interface ProviderModel {
  readonly id: string;
  readonly name?: string;
  readonly capabilities?: ProviderCapabilities;
}

export interface ProviderDefinition {
  readonly [key: string]: unknown;
  readonly id: string;
  readonly defaultModel: string;
  readonly create: (config?: unknown) => ModelApi | Promise<ModelApi>;
  readonly capabilities: (model: string) => ProviderCapabilities;
  readonly models?: (config?: unknown) => Promise<readonly ProviderModel[]>;
  readonly auth?: ProviderAuthHooks;
}

export type ProviderConfig = Readonly<Record<string, unknown>>;
export type ProviderStream = AsyncIterable<ModelStreamEvent>;
export type ProviderError = ModelError;
export type ProviderRequest = ModelRequest;

export function isModelStreamEvent(value: unknown): value is ModelStreamEvent {
  return !!value && typeof value === "object" && typeof (value as { type?: unknown }).type === "string";
}
