import type {
  CoreEvent,
  SessionSnapshot,
  ToolResult,
} from "iyon:core";
import type {
  ModelApi,
  ModelRequest,
  ModelStreamEvent,
} from "iyon:api";
import { KernelSession } from "iyon:core";

const fakeEvents: ModelStreamEvent[] = [
  { type: "started" },
  { type: "textStart", contentIndex: 0 },
  { type: "textDelta", contentIndex: 0, delta: "I will read it." },
  { type: "toolCallStart", contentIndex: 1, id: "call-1", name: "fakeRead" },
  {
    type: "toolCallDelta",
    contentIndex: 1,
    id: "call-1",
    name: "fakeRead",
    argumentsDelta: '{"path":"README"}',
  },
  {
    type: "toolCallEnd",
    contentIndex: 1,
    id: "call-1",
    name: "fakeRead",
    arguments: { path: "README" },
  },
  { type: "done", stopReason: "toolUse" },
];

export const fakeModel: ModelApi = {
  async *stream(_request: ModelRequest): AsyncIterable<ModelStreamEvent> {
    yield* fakeEvents;
  },
};

export interface SyntheticAgentResult {
  snapshot: SessionSnapshot;
  events: CoreEvent[];
}

export async function runSyntheticAgent(model: ModelApi = fakeModel): Promise<SyntheticAgentResult> {
  const session = new KernelSession({ id: 100 });
  session.appendMessage({
    role: "user",
    content: [{ type: "text", text: "Please read README" }],
  });
  const request: ModelRequest = {
    messages: [{ role: "user", content: [{ type: "text", text: "Please read README" }] }],
    tools: [{ name: "fakeRead", description: "fake", inputSchema: {} }],
    params: {},
    metadata: {},
  };
  const turn = session.beginModelTurn({ request });
  for await (const event of await model.stream(request)) {
    await turn.push(event);
  }
  const turnResult = await turn.finish();
  const tool = session.prepareToolExecution({
    turnId: turnResult.turnId,
    messageId: turnResult.assistantMessage.id,
    toolCallId: "call-1",
    toolName: "fakeRead",
    arguments: { path: "README" },
  });
  tool.prepared({ path: "README" });
  tool.start();
  tool.finish({
    content: [{ type: "text", text: "README contents" }],
    details: { synthetic: true },
    isError: false,
  } satisfies ToolResult);

  const snapshot = session.snapshot();
  session.close();
  const events: CoreEvent[] = [];
  for await (const event of session.events()) {
    events.push(event);
  }
  return { snapshot, events };
}
