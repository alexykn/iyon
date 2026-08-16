import { ContributionRegistry } from "./registries.ts";
import type { AppContribution } from "./contributions.ts";

export interface AppContext {
  readonly [key: string]: unknown;
}

export interface App {
  readonly [key: string]: unknown;
}

export interface AppRegistration {
  readonly id: string;
  readonly create: (context: AppContext) => App | Promise<App>;
}

export class AppRegistry extends ContributionRegistry<AppContribution & AppRegistration> {
  constructor(options: ConstructorParameters<typeof ContributionRegistry<AppContribution & AppRegistration>>[0] = {}) { super({ ...options, name: "app" }); }
}
