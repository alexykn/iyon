export async function* parseSse(body: ReadableStream<Uint8Array>): AsyncIterable<string> {
  const reader = body.getReader();
  const decoder = new TextDecoder();
  let buffer = "";

  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    buffer += decoder.decode(value, { stream: true });
    yield* takeFrames(buffer, (remaining) => { buffer = remaining; });
  }

  buffer += decoder.decode();
  if (buffer.trim().length > 0) {
    const data = frameData(buffer);
    if (data !== undefined) yield data;
  }
}

function* takeFrames(input: string, update: (remaining: string) => void): Generator<string> {
  let buffer = input;
  while (true) {
    const match = /\r\n\r\n|\n\n|\r\r/.exec(buffer);
    if (!match || match.index === undefined) break;
    const frame = buffer.slice(0, match.index);
    buffer = buffer.slice(match.index + match[0].length);
    const data = frameData(frame);
    if (data !== undefined) yield data;
  }
  update(buffer);
}

function frameData(frame: string): string | undefined {
  const lines: string[] = [];
  for (const line of frame.split(/\r\n|\n|\r/)) {
    if (line.trimStart().startsWith(":")) continue;
    if (!line.startsWith("data:")) continue;
    lines.push(line.slice(5).startsWith(" ") ? line.slice(6) : line.slice(5));
  }
  if (lines.length === 0) return undefined;
  return lines.join("\n");
}
