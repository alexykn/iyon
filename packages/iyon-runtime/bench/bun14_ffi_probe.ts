import { CFunction, linkSymbols, type Pointer } from "bun:ffi";
import { native } from "../src/native.ts";

type ProbePointers = {
  readonly noop_ptr: Pointer;
  readonly u32_8_ptr: Pointer;
  readonly i32_4_ptr: Pointer;
  readonly buffer_ptr: Pointer;
  readonly cstring_ptr: Pointer;
};

const pointers = native.tuiPerfAbiProbe?.() as unknown as ProbePointers | undefined;
if (pointers === undefined) throw new Error("native addon does not expose tuiPerfAbiProbe");

const directNoop = CFunction({ ptr: pointers.noop_ptr, args: ["u32"], returns: "u32" });
const linked = linkSymbols({
  noop: { ptr: pointers.noop_ptr, args: ["u32"], returns: "u32" },
  u32Eight: { ptr: pointers.u32_8_ptr, args: ["u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32"], returns: "u32" },
  i32Four: { ptr: pointers.i32_4_ptr, args: ["i32", "i32", "i32", "i32"], returns: "i32" },
  buffer: { ptr: pointers.buffer_ptr, args: ["buffer", "buffer_length"], returns: "u32" },
  cstring: { ptr: pointers.cstring_ptr, args: ["cstring"], returns: "u32" },
}).symbols;

const expectedEight = (values: readonly number[]): number => {
  const weights = [3, 5, 7, 11, 13, 17, 19, 23];
  return values.reduce((result, value, index) => result + value * weights[index]!, 0) >>> 0;
};

const eightValues = [1, 2, 3, 4, 5, 6, 7, 8] as const;
const eightResult = linked.u32Eight(...eightValues);
if (eightResult !== expectedEight(eightValues)) {
  throw new Error(`ABI argument order/signature mismatch: ${eightResult}`);
}

const i32Values = [-7, 2, 11, -13] as const;
const expectedI32 = i32Values.reduce((result, value, index) => result + value * [3, 5, 7, 11][index]!, 0);
const i32Result = linked.i32Four(...i32Values);
if (i32Result !== expectedI32) {
  throw new Error(`i32 ABI argument order/signature mismatch: ${i32Result}`);
}

const bytes = new Uint8Array([0x7b, 0x01, 0x02, 0x03]);
const bufferResult = linked.buffer(bytes, bytes);
const expectedBuffer = bytes.byteLength * 257 + bytes[0]!;
if (bufferResult !== expectedBuffer) {
  throw new Error(`buffer_length mismatch: ${bufferResult} !== ${expectedBuffer}`);
}

const text = "Bun 1.4 — ABI ✓";
const encoded = new TextEncoder().encode(text);
let expectedHash = 2166136261;
for (const byte of encoded) expectedHash = Math.imul(expectedHash ^ 0, 16777619) + byte >>> 0;
const cstringResult = linked.cstring(text);
if (cstringResult !== expectedHash) {
  throw new Error(`cstring UTF-8 mismatch: ${cstringResult} !== ${expectedHash}`);
}

let value = 0;
for (let index = 0; index < 1_000_000; index += 1) value = directNoop(value);
const samples: number[] = [];
for (let sample = 0; sample < 5; sample += 1) {
  const started = Bun.nanoseconds();
  for (let index = 0; index < 1_000_000; index += 1) value = directNoop(value);
  samples.push(Bun.nanoseconds() - started);
}
const averageNs = samples.reduce((sum, sample) => sum + sample, 0) / samples.reduce((sum) => sum + 1_000_000, 0);
if (value === 0) throw new Error("no-op result was optimized away");
if (averageNs >= 5) throw new Error(`hot no-op FFI gate failed: ${averageNs.toFixed(3)} ns`);

console.log(JSON.stringify({
  bun: Bun.version,
  revision: Bun.revision,
  jit_enabled: averageNs < 5,
  pointers,
  u32_8: "pass",
  i32_4: "pass",
  buffer_length: "pass",
  cstring_utf8: "pass",
  noop_average_ns: averageNs,
  noop_sub_5ns: averageNs < 5,
}));
