export type PluginErrorCode =
  | "manifest"
  | "source"
  | "compatibility"
  | "registration"
  | "activation"
  | "load";

export interface PluginDiagnosticDetails {
  readonly packageId?: string;
  readonly extensionId?: string;
  readonly source?: string;
  readonly entrypoint?: string;
}

export class PluginError extends Error {
  readonly code: PluginErrorCode;
  readonly details: PluginDiagnosticDetails;

  constructor(code: PluginErrorCode, message: string, details: PluginDiagnosticDetails = {}, cause?: unknown) {
    super(message, { cause });
    this.name = "PluginError";
    this.code = code;
    this.details = details;
  }
}

export class ManifestError extends PluginError {
  constructor(message: string, details: PluginDiagnosticDetails = {}, cause?: unknown) {
    super("manifest", message, details, cause);
    this.name = "ManifestError";
  }
}

export class SourceError extends PluginError {
  constructor(message: string, details: PluginDiagnosticDetails = {}, cause?: unknown) {
    super("source", message, details, cause);
    this.name = "SourceError";
  }
}

export class CompatibilityError extends PluginError {
  constructor(message: string, details: PluginDiagnosticDetails = {}, cause?: unknown) {
    super("compatibility", message, details, cause);
    this.name = "CompatibilityError";
  }
}

export class RegistrationError extends PluginError {
  constructor(message: string, details: PluginDiagnosticDetails = {}, cause?: unknown) {
    super("registration", message, details, cause);
    this.name = "RegistrationError";
  }
}

export class ActivationError extends PluginError {
  constructor(message: string, details: PluginDiagnosticDetails = {}, cause?: unknown) {
    super("activation", message, details, cause);
    this.name = "ActivationError";
  }
}

export class LoadError extends PluginError {
  constructor(message: string, details: PluginDiagnosticDetails = {}, cause?: unknown) {
    super("load", message, details, cause);
    this.name = "LoadError";
  }
}

export function diagnosticMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
