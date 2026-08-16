export async function* eventsFromNextEvent<T>(
  nextEvent: () => Promise<T | null>,
): AsyncIterable<T> {
  for (;;) {
    const event = await nextEvent();
    if (event === null) {
      return;
    }
    yield event;
  }
}
