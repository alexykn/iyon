import type { KernelSession, MessageId } from "@iyon/sdk";

export interface SteeringQueue {
  drain(): readonly string[];
}

export function drainSteering(session: KernelSession, queue?: SteeringQueue): string[] {
  const messages = [...(queue?.drain() ?? [])];
  const steer = session.dequeue("steer");
  if (steer !== null) messages.push(steer);
  return messages;
}

export function injectSteeredMessages(session: KernelSession, messages: readonly string[]): readonly MessageId[] {
  return messages.map((text) => session.appendMessage({ role: "user", content: [{ type: "text", text }] }));
}
