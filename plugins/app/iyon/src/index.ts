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
export { ComposerPasteStore, isLargePaste, normalizePaste, MAX_COMPOSER_ROWS } from "./composer.ts";
export { createInitialState, cycleReasoningEffort, draftIdFor, reduceIyonState, updateInfo } from "./state.ts";
export { createIyonTheme } from "./theme.ts";
export { createIyonView, footerText } from "./view.ts";
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
