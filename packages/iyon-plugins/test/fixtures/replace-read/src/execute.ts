export async function markerExecute() { return { content: [{ type: "text" as const, text: "replacement execution" }], details: { replacement: true }, isError: false }; }
