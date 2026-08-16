export const MAX_COMPOSER_ROWS = 13;
export const LARGE_PASTE_CHAR_THRESHOLD = 1000;
export const LARGE_PASTE_LINE_THRESHOLD = 10;

export function normalizePaste(value: string): string {
  return value.replaceAll("\r\n", "\n").replaceAll("\r", "\n").replaceAll("\t", "    ");
}

export function isLargePaste(value: string): boolean {
  return value.length > LARGE_PASTE_CHAR_THRESHOLD || value.split("\n").length > LARGE_PASTE_LINE_THRESHOLD;
}

export class ComposerPasteStore {
  private readonly entries: Array<readonly [string, string]> = [];
  private nextId = 0;

  displayText(current: string, input: string): string {
    const normalized = normalizePaste(input);
    if (!isLargePaste(normalized)) return normalized;
    const count = normalized.length;
    const base = `[Pasted Content ${count} chars]`;
    const marker = this.markerFor(current, base);
    this.entries.push([marker, normalized]);
    return marker;
  }

  expand(text: string): string {
    let expanded = text;
    for (const [marker, original] of this.entries) expanded = expanded.replaceAll(marker, original);
    this.clear();
    return expanded;
  }

  clear(): void { this.entries.length = 0; }
  get size(): number { return this.entries.length; }

  private markerFor(current: string, base: string): string {
    if (!current.includes(base) && !this.entries.some(([marker]) => marker === base)) return base;
    do {
      this.nextId = (this.nextId + 1) || 1;
    } while (current.includes(`${base} #${this.nextId}`) || this.entries.some(([marker]) => marker === `${base} #${this.nextId}`));
    return `${base} #${this.nextId}`;
  }
}
