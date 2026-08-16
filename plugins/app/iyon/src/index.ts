import type { ExtensionAPI } from "iyon:plugins";
import { createIyonApp } from "./app.ts";
import type { IyonAppDependencies } from "./app.ts";

export const IYON_APP_ID = "iyon" as const;

export function activate(api: ExtensionAPI): void {
  api.apps.register({
    id: IYON_APP_ID,
    create(context) {
      return createIyonApp(context as IyonAppDependencies);
    },
  });
}

export { createIyonApp } from "./app.ts";
export type { IyonApp, IyonAppDependencies } from "./app.ts";
export type {
  FrontendEvent,
  InfoState,
  IyonAction,
  IyonAgent,
  IyonCoreCommands,
  IyonModelMetadata,
  IyonState,
  LiveTool,
  PendingApproval,
  ToolDraftKey,
  ToolUpdatePresentation,
} from "./contracts.ts";
