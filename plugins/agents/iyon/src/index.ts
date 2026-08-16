import type { ExtensionAPI } from "iyon:plugins";
import { IyonAgent, type AgentContext } from "./agent.ts";

export function activate(api: ExtensionAPI): void {
  api.agents.register({
    id: "iyon",
    create(context) {
      return new IyonAgent(context as AgentContext);
    },
  });
}

export { IyonAgent } from "./agent.ts";
export type { Agent, AgentContext } from "./agent.ts";
