import type { ModelStreamEvent } from "iyon:api";

export interface InvalidToolCall {
  readonly contentIndex: number;
  readonly id?: string;
  readonly name?: string;
  readonly argumentsText: string;
  readonly reason: string;
}

interface PendingToolCall {
  readonly contentIndex: number;
  id?: string;
  name?: string;
  argumentsText: string;
  ended: boolean;
}

export class StreamAssembler {
  private readonly calls = new Map<number, PendingToolCall>();

  observe(event: ModelStreamEvent): void {
    if (event.type === "toolCallStart") {
      this.calls.set(event.contentIndex, { contentIndex: event.contentIndex, id: event.id, name: event.name, argumentsText: "", ended: false });
      return;
    }
    if (event.type === "toolCallDelta") {
      const call = this.calls.get(event.contentIndex) ?? { contentIndex: event.contentIndex, argumentsText: "", ended: false };
      call.id ??= event.id;
      call.name ??= event.name;
      call.argumentsText += event.argumentsDelta;
      this.calls.set(event.contentIndex, call);
      return;
    }
    if (event.type === "toolCallEnd") {
      const call = this.calls.get(event.contentIndex) ?? { contentIndex: event.contentIndex, argumentsText: "", ended: false };
      call.id ??= event.id;
      call.name ??= event.name;
      call.argumentsText = JSON.stringify(event.arguments);
      call.ended = true;
      this.calls.set(event.contentIndex, call);
    }
  }

  invalidCalls(): InvalidToolCall[] {
    return [...this.calls.values()]
      .filter((call) => !call.ended || call.id === undefined || call.name === undefined)
      .map((call) => ({
        contentIndex: call.contentIndex,
        ...(call.id === undefined ? {} : { id: call.id }),
        ...(call.name === undefined ? {} : { name: call.name }),
        argumentsText: call.argumentsText,
        reason: !call.ended ? "tool call stream ended before toolCallEnd" : "tool call is missing an id or name",
      }));
  }
}
