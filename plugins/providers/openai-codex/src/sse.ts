export async function* parseSse(body: ReadableStream<Uint8Array>): AsyncIterable<string> {
  const reader = body.getReader();
  const decoder = new TextDecoder();
  let buffer = "";
  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    buffer += decoder.decode(value, { stream: true });
    while (true) {
      const match = /\r\n\r\n|\n\n|\r\r/.exec(buffer);
      if (!match || match.index === undefined) break;
      const frame = buffer.slice(0, match.index);
      buffer = buffer.slice(match.index + match[0].length);
      const data = frameData(frame);
      if (data !== undefined) yield data;
    }
  }
  buffer += decoder.decode();
  const data = frameData(buffer);
  if (data !== undefined) yield data;
}

function frameData(frame: string): string | undefined {
  const lines = frame.split(/\r\n|\n|\r/).filter((line) => !line.trimStart().startsWith(":"));
  const data = lines.filter((line) => line.startsWith("data:")).map((line) => line.slice(5).startsWith(" ") ? line.slice(6) : line.slice(5));
  return data.length > 0 ? data.join("\n") : undefined;
}
