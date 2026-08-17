import type { KernelSession, MessageId } from "@iyon/sdk";

export interface SteeringQueue {
  drain(): readonly string[];
}

export function drainSteering(session: KernelSession, queue?: SteeringQueue): string[] {
  const messages = [...(queue?.drain() ?? [])];
  drainQueue(session, "steer", messages);
  drainQueue(session, "followUp", messages);
  return messages;
}

export function drainPrompts(session: KernelSession, messages: string[] = []): string[] {
  drainQueue(session, "prompt", messages);
  return messages;
}

function drainQueue(session: KernelSession, kind: "prompt" | "steer" | "followUp", messages: string[]): void {
  const value = session.dequeue(kind);
  if (value !== null) messages.push(value);
}

export function injectSteeredMessages(session: KernelSession, messages: readonly string[]): readonly MessageId[] {
  return messages.map((text) => session.deliverUserMessage(text));
}
