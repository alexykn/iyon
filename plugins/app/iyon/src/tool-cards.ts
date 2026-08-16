import type { JsonValue } from "@iyon/sdk";
import type { LiveTool, ToolDraftKey, ToolUpdatePresentation } from "./contracts.ts";
import { draftIdFor } from "./state.ts";

export class ToolCardStore {
  private readonly cards = new Map<string, LiveTool>();
  private readonly ids = new Map<string, string>();

  preparing(key: ToolDraftKey, toolCallId?: string, toolName?: string): LiveTool {
    const id = draftIdFor(key);
    const existing = this.cards.get(id);
    if (existing !== undefined) {
      const updated = { ...existing, toolCallId: toolCallId ?? existing.toolCallId, toolName: toolName ?? existing.toolName };
      this.cards.set(id, updated); if (toolCallId) this.ids.set(toolCallId, id); return updated;
    }
    const card: LiveTool = { draftKey: key, toolCallId, toolName, status: "preparing", text: "", isError: false, frozen: false };
    this.cards.set(id, card); if (toolCallId) this.ids.set(toolCallId, id); return card;
  }

  arguments(key: ToolDraftKey, delta: string, toolCallId?: string, toolName?: string): LiveTool {
    const card = this.preparing(key, toolCallId, toolName);
    const updated = { ...card, text: card.text + delta, toolCallId: toolCallId ?? card.toolCallId, toolName: toolName ?? card.toolName };
    const id = draftIdFor(key); this.cards.set(id, updated); if (updated.toolCallId) this.ids.set(updated.toolCallId, id); return updated;
  }

  prepared(key: ToolDraftKey, toolCallId: string, toolName: string, argumentsValue: JsonValue): LiveTool {
    const card = this.preparing(key, toolCallId, toolName);
    const id = draftIdFor(key); const updated = { ...card, toolCallId, toolName, arguments: argumentsValue, status: "prepared" as const };
    this.cards.set(id, updated); this.ids.set(toolCallId, id); return updated;
  }

  started(toolCallId: string, toolName: string, argumentsValue: JsonValue): LiveTool {
    const existingId = this.ids.get(toolCallId);
    if (existingId !== undefined) {
      const card = this.cards.get(existingId)!; const updated = { ...card, toolCallId, toolName, arguments: argumentsValue, status: "running" as const };
      this.cards.set(existingId, updated); return updated;
    }
    const card: LiveTool = { toolCallId, toolName, arguments: argumentsValue, status: "running", text: "", isError: false, frozen: false };
    this.cards.set(toolCallId, card); this.ids.set(toolCallId, toolCallId); return card;
  }

  update(toolCallId: string, update: ToolUpdatePresentation): LiveTool | undefined {
    return this.map(toolCallId, (card) => update.type === "text" ? { ...card, text: card.text + update.text } : update.type === "progress" ? { ...card, progress: update } : { ...card, details: update.details });
  }
  approval(toolCallId: string): LiveTool | undefined { return this.map(toolCallId, (card) => ({ ...card, status: "pendingApproval" as const })); }
  resolveApproval(toolCallId: string, approved: boolean): LiveTool | undefined { return this.map(toolCallId, (card) => ({ ...card, status: approved ? "running" as const : "cancelled" as const, frozen: !approved })); }
  cancel(toolCallId: string): LiveTool | undefined { return this.map(toolCallId, (card) => ({ ...card, status: "cancelled" as const, frozen: true, isError: true })); }
  finish(toolCallId: string, isError: boolean): LiveTool | undefined { return this.map(toolCallId, (card) => ({ ...card, status: isError ? "failed" as const : "finished" as const, isError, frozen: true })); }
  result(toolCallId: string, toolName: string, text: string, details: JsonValue, isError: boolean): LiveTool | undefined { return this.map(toolCallId, (card) => ({ ...card, toolName, text, details, status: isError ? "failed" as const : "finished" as const, isError, frozen: true })); }
  get(toolCallId: string): LiveTool | undefined { const id = this.ids.get(toolCallId); return id === undefined ? undefined : this.cards.get(id); }
  keyFor(toolCallId: string): string | undefined { return this.ids.get(toolCallId); }
  keyForDraft(key: ToolDraftKey): string { return draftIdFor(key); }
  getByKey(key: string): LiveTool | undefined { return this.cards.get(key); }
  values(): readonly LiveTool[] { return [...this.cards.values()]; }
  clear(): void { this.cards.clear(); this.ids.clear(); }

  private map(toolCallId: string, update: (card: LiveTool) => LiveTool): LiveTool | undefined {
    const id = this.ids.get(toolCallId); if (id === undefined) return undefined;
    const card = this.cards.get(id); if (card === undefined) return undefined;
    const updated = update(card); this.cards.set(id, updated); return updated;
  }
}
