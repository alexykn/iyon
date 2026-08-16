export const DEFAULT_SYSTEM_PROMPT = "";

export function buildSystemPrompt(prompt = DEFAULT_SYSTEM_PROMPT): string | undefined {
  const normalized = prompt.trim();
  return normalized.length === 0 ? undefined : normalized;
}
