Yes. After reconstructing the actual PERF-7 implementation from commit `a280d814105874fa415dac2ba0e7658471dae074`, I think **PERF-7 should be reopened**.

There is one nuance: the old Candidate B was not literally “uncached.” It had a `known: Set<NodeId>` in JS, emitted a `REF` opcode for known nodes, and had a native weak cache. But it **did not implement production-equivalent retained caching**, and several shortcuts strongly biased the warm-cache comparison against the packed design. The original handoff itself explicitly required Candidate B to reference already-known NodeIds rather than serialize stable subtrees. 

The prior conclusion therefore should be treated as **inconclusive**, not “Candidate A won.”

# PERF-7v2 — identity-preserving packed View transport shootout

## 0. Why PERF-7 must be redone

The previous experiment demonstrated one genuinely interesting fact: packing structural information into a `Uint32Array` could be dramatically faster on large cold/rebuilt trees. The recorded median improvements were +48–54% at 2k nodes and +64–72% at 10k nodes in COLD/REBUILT cases. 

That is enough evidence to make a proper second experiment worthwhile.

It did **not** establish that packed transport is intrinsically worse under retained identity.

### 0.1 The JS “known” cache was not a real weak-cache protocol

Historical Candidate B had:

```ts
type PackedState = {
  known: Set<number>;
  words: number[];
  strings: string[];
};
```

and:

```ts
if (state.known.has(id)) {
  state.words.push(0, id);
  return;
}
state.known.add(id);
```



Native, however, stored:

```text
NodeId → WeakView
```

The direct decoder handles an expired weak entry correctly:

```text
NodeId lookup
→ WeakView upgrade succeeds:
      return cached View immediately

→ WeakView upgrade fails:
      remove stale cache entry
      decode the supplied node payload
      repopulate cache
```

That behavior remains in the current decoder. 

Historical Candidate B instead did:

```text
JS known Set says ID is known
→ send REF only
→ native WeakView expired
→ error "reference points to unknown node id"
```

There was:

- no miss recovery;
- no cold retry;
- no cache-generation handshake;
- no invalidation of the JS `known` set;
- no synchronization with native weak-cache pruning.

So the implementation could only safely exercise short benchmark histories where its assumptions happened to remain true.

That is **not equivalent caching**.

---

## 0.2 Candidate B used a separate host-local native cache

Candidate A's actual cache is environment-local and registered against N-API environment lifetime.

Candidate B added this instead:

```rust
struct NativeTuiHost {
    ...
    packed_cache: Arc<Mutex<ViewBridgeCache>>,
}
```



So the experiment compared:

```text
A:
environment-local retained semantic cache

B:
special benchmark-only per-host cache
```

Those are different lifetime/reuse domains.

7v2 must use **the exact same `ViewBridgeCache` implementation** for both transports.

---

## 0.3 Packed NodeId was incorrectly only 32 bits

This is a major correctness limitation.

The real decoder accepts a NodeId up to JavaScript's maximum safe integer:

```text
9,007,199,254,740,991 = 2^53 - 1
```



Historical Candidate B did:

```rust
let id = u64::from(self.word("node id")?);
```

where `word()` returned one `u32`. 

And JS wrote:

```ts
state.words.push(..., id);
new Uint32Array(state.words)
```



Therefore Candidate B could not preserve the real NodeId domain.

A production packed transport needs exact 53-bit identity.

---

## 0.4 Candidate B implemented only Text and Column

The entire packed native decoder supported:

```text
REF
TEXT
COLUMN
```



And the text encoding was:

```ts
const text = node.spans.map((span) => span.text).join("");
```

which discarded:

```text
span boundaries
span styles
wrap mode
alignment
```

The real bridge schema includes text styling, Diff, Spacer, Row, Column, Hanging, Grid, Container, Clamp, ContentMax, Component, Decorated nodes, layout child variants, arbitrary style states, borders, dimensions, overflow indicators, etc. 

The prior correctness oracle only proved:

```text
Column
├── plain Text
└── plain Text
```

So it was a benchmark of a **tiny cheaper subset**, not a production transport.

Ironically this makes the strong cold result even more interesting, but it cannot settle the architecture either way.

---

## 0.5 The warm packed path had needless allocation overhead

Every packed commit did:

```ts
state.words = [];
state.strings = [];

...

return {
  words: new Uint32Array(state.words),
  strings: state.strings,
};
```



Even an `IDENTICAL_IDENTITY` update where the complete semantic operation was logically:

```text
REF(root NodeId)
```

still created:

- a new JS array;
- a new string array;
- a new `Uint32Array`;
- a copy from `number[]` into that typed array.

Candidate A on its best warm path does:

```text
pass existing object
→ native reads NodeId
→ native WeakView hit
→ return
```

No tree walk and no transaction allocation.

That is not an intrinsic property of packed transport. A serious implementation can cache/reuse the tiny root-reference packet and use a reusable typed-array scratch arena for changed paths.

---

## 0.6 The benchmark result says “post-warmup”; the committed benchmark did not warm up

The result document states:

> “20 post-warmup samples”



But the committed harness creates a fresh host and immediately starts recording:

```ts
const host = createHost();

...

for (let index = 0; index < count; index += 1) {
  const started = Bun.nanoseconds();
  ...
  samples.push(...);
}
```

There is no warm-up loop. 

Therefore the first recorded IDENTICAL/SHARED sample is the cache-populating sample.

Twenty samples also makes tail metrics extremely weak.

So the old results are not reproducible as described.

---

# 1. Goal of PERF-7v2

Answer one question:

> **If both transports preserve the exact same semantic identity, weak-cache lifetime, subtree cutoff, full View schema and operation semantics, does an optimized packed structural transaction beat direct N-API object traversal enough to justify its complexity?**

Candidate A remains:

```text
BridgeViewNode object
→ N-API
→ read NodeId
→ environment WeakView lookup
→ stop on hit
→ property-walk/decode misses
→ final Arc-backed View
```

Candidate B must become:

```text
BridgeViewNode immutable DAG
→ identity-aware packed encoder
→ REF for already-known stable subtree
→ DEF only for genuinely new semantic path
→ borrowed Uint32Array + changed strings
→ same environment WeakView cache
→ REF cutoff
→ decode DEF misses
→ final Arc-backed View
```

The comparison is invalid unless both have the same asymptotic shape:

```text
IDENTICAL:
    O(1)

SHARED_PATH:
    O(changed path)

COLD / REBUILT:
    O(total tree)
```

---

# 2. Prerequisites

**Do not begin 7v2 until these are done:**

```text
REMED-0 benchmark oracle repaired
REMED-1 nested TS NodeId correctness repaired
REMED-2 normal native mutations are dirty-and-return
```

The NodeId fix is especially mandatory.

A cache comparison is meaningless while semantically changed decorated children can accidentally retain their old ID.

The scheduling fix matters because transport benchmarking should measure:

```text
semantic construction
+
transport
+
native View reconstruction/cache
+
retained-state commit
```

without burying that cost beneath synchronous layout/paint.

---

# 3. Do not modify Candidate A for the shootout

Candidate A is the fixed baseline.

The current direct cache behavior is correct in shape:

```text
read NodeId first

hit:
    return cached Rust View
    do not read schema
    do not read kind
    do not read children

miss:
    remove expired WeakView if present
    validate schema
    decode payload
    cache WeakView
```



Do not “help” Candidate A or Candidate B by weakening one side.

---

# 4. Candidate B must use the SAME native cache

Delete the historical concept of:

```rust
NativeTuiHost {
    packed_cache: ...
}
```

Candidate B must use:

```rust
ViewBridgeCache {
    nodes: HashMap<u64, WeakView>,
}
```

owned by the same N-API environment cache infrastructure used by Candidate A.

Refactor:

```rust
fn view_bridge_cache(value: &Object<'_>) -> Result<...>
```

into something approximately like:

```rust
fn view_bridge_cache_for_env(
    env: napi::Env,
) -> Result<Arc<Mutex<ViewBridgeCache>>>;

fn view_bridge_cache(
    value: &Object<'_>,
) -> Result<Arc<Mutex<ViewBridgeCache>>> {
    view_bridge_cache_for_raw_env(value.value().env)
}
```

The packed N-API probe should accept/inject `Env` and call the same helper.

There must be only **one cache implementation**, including the same:

```text
WeakView behavior
expired-entry removal
pruning
environment cleanup hook
```

Candidate B is testing a transport, not a different retention architecture.

---

# 5. Candidate B JS cache must be weak and self-healing

Do **not** recreate:

```ts
known: Set<number>
```

That retains every all-time NodeId and cannot know when native `WeakView` expires.

Use semantic object identity:

```ts
class PackedViewEncoder {
  private knownNodes = new WeakSet<object>();
}
```

The key must be the actual frozen `BridgeViewNode` object.

Why this works:

```text
same semantic node
→ same private BridgeViewNode object
→ same NodeId
→ WeakSet hit

new semantic node
→ new BridgeViewNode
→ WeakSet miss
```

And because it is a `WeakSet`, the transport does not keep discarded semantic Views alive.

---

# 6. Native weak-cache expiry must be recoverable

A JS `WeakSet` only means:

> “this node was successfully transmitted to native before.”

It does **not** mean:

> “the Rust WeakView still upgrades now.”

That distinction is fundamental.

Candidate B therefore needs an optimistic REF protocol with cold recovery.

## Normal path

```text
knownNodes.has(node)
    ↓
emit REF(NodeId)
    ↓
native WeakView upgrades
    ↓
return cached View
```

## Desynchronized path

```text
knownNodes.has(node)
    ↓
emit REF(NodeId)
    ↓
native WeakView no longer upgrades
    ↓
native returns/throws PACKED_CACHE_MISS
    ↓
JS discards transport knowledge:
    knownNodes = new WeakSet()
    ↓
re-encode CURRENT ROOT as complete definitions
    ↓
retry once
    ↓
success
```

Do not strong-cache Rust Views just to avoid this recovery path.

The native cache must stay weak.

---

# 7. Cache miss must not partially mutate the host

Decode the complete packed root first.

Only after successful decoding:

```rust
self.host.render(view)?
```

Therefore:

```text
packed cache miss
→ no body replacement
→ no History mutation
→ no ViewSlot mutation
→ no terminal state change
```

Some newly decoded child `WeakView` entries may have been inserted before a later REF misses. That is harmless because NodeIds are immutable.

The host mutation itself must remain atomic.

---

# 8. Retry exactly once

Pseudo-code:

```ts
function commitPacked(
  root: View,
  invoke: (words: Uint32Array, strings: string[]) => void,
): void {
  try {
    const tx = encoder.encode(root);
    invoke(tx.words, tx.strings);
    encoder.commitSuccessfulDefinitions();
    return;
  } catch (error) {
    if (!isPackedCacheMiss(error)) throw error;
  }

  encoder.resetKnownNativeState();

  const cold = encoder.encodeCold(root);
  invoke(cold.words, cold.strings);

  encoder.commitSuccessfulDefinitions();
}
```

If the cold retry reports another missing persistent reference:

```text
FAIL HARD
```

because cold encoding is only allowed to use transaction-local backward references.

A second cache miss indicates a protocol/encoder bug.

---

# 9. Distinguish persistent references from transaction-local DAG references

A DAG may intentionally reuse one exact child twice:

```ts
const shared = View.text("x");

View.vertical([
  shared,
  shared,
]);
```

Cold encoding should not define it twice.

Maintain:

```ts
seenThisTransaction: Map<NodeId, BridgeViewNode>
```

Algorithm:

```ts
function encodeNode(node: BridgeViewNode): void {
  const seen = seenThisTransaction.get(node.id);

  if (seen !== undefined) {
    if (seen !== node) {
      throw new Error("same NodeId belongs to different bridge objects");
    }

    emitRef(node.id); // backward reference in this transaction
    return;
  }

  seenThisTransaction.set(node.id, node);

  if (!forceCold && knownNodes.has(node)) {
    emitRef(node.id);
    return;
  }

  emitDefinition(node);
  definedThisTransaction.push(node);
}
```

On successful native operation:

```ts
for (const node of definedThisTransaction) {
  knownNodes.add(node);
}
```

Do not add them before success.

---

# 10. NodeId must remain a full safe integer

Never serialize NodeId in one `u32`.

Encode it as two words:

```text
low 32 bits
high 21 bits
```

JS must not use bitwise operators on the original number.

Use arithmetic:

```ts
const U32 = 0x1_0000_0000;

function splitSafeU64(value: number): readonly [number, number] {
  if (
    !Number.isSafeInteger(value) ||
    value < 0
  ) {
    throw new RangeError("expected safe integer");
  }

  const low = value % U32;
  const high = Math.floor(value / U32);

  return [low, high];
}
```

Native:

```rust
fn safe_u64(low: u32, high: u32) -> Result<u64> {
    let value = (u64::from(high) << 32) | u64::from(low);

    if value > 9_007_199_254_740_991 {
        return Err(...);
    }

    Ok(value)
}
```

NodeId additionally requires `value != 0`.

Use the same encoding for any other bridge number whose current semantic domain is a JS safe integer, including component handles or Diff coordinates where applicable.

---

# 11. Wire format

Do not improvise this during implementation.

Use a versioned transaction.

## Header

```text
word  meaning
----  ------------------------------------
0     magic = PACKED_VIEW_MAGIC
1     packed protocol version
2     VIEW_BRIDGE_SCHEMA_VERSION
3     used word count
4     root count
5...  root records
```

`used word count` exists because the JS encoder will reuse a larger scratch `Uint32Array`.

Native must ignore capacity after `used_words`.

Validate:

```text
magic exact
protocol version exact
bridge schema exact
used_words >= header size
used_words <= actual typed-array length
root_count expected by operation
```

---

# 12. Node records

Use:

```text
REF:
    opcode = REF
    record_words = 4
    id_low
    id_high

DEF:
    opcode = DEF
    record_words
    id_low
    id_high
    bridge_kind
    payload...
```

`record_words` includes the complete recursive record.

This gives native exact structural boundaries and makes malformed input fail locally rather than consuming a sibling record.

Do **not** omit NodeId from definitions.

Identity is the point of the experiment.

---

# 13. REF native semantics

Pseudo-code:

```rust
fn decode_ref(&mut self, id: u64) -> Result<View, PackedError> {
    if self.active.contains(&id) {
        return Err(PackedError::Cycle(id));
    }

    match self.cache.upgrade(id)? {
        Some(view) => {
            perf::inc(PackedRefHits);
            Ok(view)
        }

        None => {
            perf::inc(PackedRefMisses);
            Err(PackedError::CacheMiss)
        }
    }
}
```

A stale persistent REF becomes `CacheMiss`.

An active-parent REF is a malformed cycle and must **not** be treated as recoverable weak-cache expiry.

---

# 14. DEF native semantics

Definitions are authoritative payloads.

Do not use a cache hit to avoid parsing a DEF initially.

Reason: after transport resynchronization the JS encoder deliberately sends complete definitions so that successful completion establishes that every `definedThisTransaction` node really has an individual native cache entry.

Pseudo-code:

```rust
fn decode_definition(
    &mut self,
    id: u64,
    record_end: usize,
) -> Result<View, PackedError> {
    if !self.active.insert(id) {
        return Err(PackedError::Cycle(id));
    }

    let kind = self.word()?;
    let candidate = self.decode_kind(kind)?;

    self.active.remove(&id);

    if self.cursor != record_end {
        return Err(PackedError::BadRecordLength);
    }

    // Optional but recommended invariant check.
    if let Some(existing) = self.cache.upgrade(id)? {
        if existing != candidate {
            return Err(PackedError::NodeIdentityChanged(id));
        }

        return Ok(existing);
    }

    self.cache.insert(id, candidate.downgrade())?;
    Ok(candidate)
}
```

That semantic-equality check occurs only when a full definition is unexpectedly resent for a still-live NodeId, not on normal warm REF hits.

It catches exactly the class of stale-identity error discovered in the current TS layer.

---

# 15. Full bridge schema is mandatory

Candidate B is not benchmark-eligible until it supports **every current `BridgeViewNode` kind**.

The current private schema contains the complete family shown in `ir.ts`. 

Required node payload grammar:

```text
TEXT
    wrap
    horizontal align
    span_count
    repeated:
        text_string_index
        optional Style

DIFF
    hunk_count
    repeated hunk:
        old range
        new range
        line_count
        repeated line:
            kind
            text_string_index
            termination
            optional/required oldLine
            optional/required newLine

SPACER
    rows

ROW
    gap
    child_count
    repeated LayoutChild

COLUMN
    gap
    child_count
    repeated LayoutChild

HANGING
    Node prefix
    Node continuation
    Node body

GRID
    column_count
    repeated GridTrack

    row_count
    repeated:
        row GridTrack
        cell_count
        repeated:
            columnSpan
            rowSpan
            horizontalAlign
            verticalAlign
            Node

    columnGap
    rowGap

CONTAINER
    Node child

CLAMP
    maxRows
    OverflowIndicator
    Node child

CONTENT_MAX
    maxRows
    Node child

COMPONENT
    full safe-integer handle

DECORATED
    Decoration
    Node child
```

Do not join text spans.

Do not normalize away semantically observable distinctions.

---

# 16. LayoutChild encoding

Use a fixed-size prefix:

```text
kind
size
maxRows
Node
```

Rules:

```text
normal:
    size=0
    maxRows=0

fixed:
    size=<value>
    maxRows=0

flex:
    size=0
    maxRows=0

flexMax:
    size=0
    maxRows=<value>

contentMax:
    size=0
    maxRows=<value>
```

Native applies the same horizontal/vertical validity constraints as the current direct decoder.

Do not create alternate semantics for packed transport.

---

# 17. GridTrack encoding

```text
kind
value
```

Interpretation:

```text
content       value=0
contentMax    value=max
fixed         value=size
flex          value=0
flexMax       value=max
```

---

# 18. Style encoding must preserve “unset” versus false

This matters.

Do not encode text attributes as only:

```text
bold=true bit
italic=true bit
...
```

because:

```text
attribute absent
```

and:

```text
attribute explicitly false
```

can have different cascade meaning.

Use two masks:

```text
attribute_present_mask
attribute_true_mask
```

For:

```text
bold
dim
italic
underline
reversed
strikethrough
```

Then:

```text
present=0                 → unset
present=1,true=0          → explicitly false
present=1,true=1          → explicitly true
```

---

# 19. Style grammar

Use:

```text
flags

if THEME:
    theme string index

if FOREGROUND:
    Color

if BACKGROUND:
    Color

attribute_present_mask
attribute_true_mask
```

If current bridge semantics allow additional arbitrary attribute names, preserve them explicitly rather than silently dropping them.

The packed decoder must accept exactly the semantic domain accepted by the direct decoder.

---

# 20. Color grammar

```text
NONE
STRING
    string_index

ANSI
    value
```

Do not convert named/theme colors in JS.

Native performs the same semantic conversion as Candidate A.

---

# 21. Decoration grammar

Use a presence bitmap:

```text
PADDING
BACKGROUND
FOREGROUND
BORDER
STYLE_STATES
WIDTH_RULE
HEIGHT_RULE
MIN_WIDTH
MAX_WIDTH
MIN_HEIGHT
MAX_HEIGHT
```

`style` itself is always encoded because current `DecorationNode` always contains it.

Then emit only fields selected by the bitmap.

Padding:

```text
top
right
bottom
left
```

Width/height rule:

```text
FIT
FILL
```

with absence represented by its presence bit.

---

# 22. Style states

These are arbitrary semantic strings.

Encode:

```text
state_count
repeated:
    key_string_index
    value_string_index
```

Do not hash them.

Do not sort unless the direct semantic path normalizes ordering identically.

---

# 23. Border

Encode:

```text
flags

optional glyph map:
    glyph_count
    repeated:
        key_string_index
        value_string_index

border-style tag:
    absent
    plain
    rounded
    double

edge tag:
    absent
    all
    topBottom

optional Color
```

Arbitrary glyph text stays in the string table.

---

# 24. Overflow indicator

```text
NONE

ELLIPSIS
    Style

FOOTER
    prefix string index
    Style
```

---

# 25. Shared canonical protocol constants

Do not put unrelated magic integers into TS and Rust manually.

Extend the existing canonical bridge-schema mechanism with:

```text
packed protocol version
packed magic
REF/DEF opcodes
color tags
border tags
presence-bit assignments
```

The bridge schema already provides canonical numeric discriminants for the direct bridge. PERF-7v2 should extend that principle rather than create another unsynchronized schema. The original handoff explicitly required checked/shared numeric mappings. 

The `build.rs` JSON-parser cleanup from the remediation handoff should happen before this, so Rust can deserialize the schema properly rather than extending the current textual scanner.

---

# 26. Write directly into reusable `Uint32Array`

Do not recreate the historical:

```text
number[]
→ new Uint32Array(number[])
```

path.

Use:

```ts
class WordWriter {
  private buffer = new Uint32Array(INITIAL_CAPACITY);
  private cursor = 0;

  reset(): void {
    this.cursor = HEADER_WORDS;
  }

  push(value: number): void {
    this.ensure(1);
    this.buffer[this.cursor++] = value;
  }

  ensure(additional: number): void {
    ...
  }
}
```

Grow geometrically.

Copy only when capacity actually grows.

For ordinary commits, reuse the same backing buffer.

Because native only borrows it synchronously, reuse after the N-API call is safe.

The transaction header's `used_words` prevents native from reading stale unused capacity.

---

# 27. Reuse the string-array object too

Keep:

```ts
private readonly strings: string[] = [];
```

and:

```ts
this.strings.length = 0;
```

for each transaction.

Do not allocate another `[]` every call.

A later UTF-8 arena experiment is separate.

---

# 28. Add an actual IDENTICAL fast path

This is probably the most important warm-path omission from the first experiment.

For a single known root:

```text
do not reset/walk scratch encoder
do not visit descendants
do not allocate new Uint32Array
do not allocate a new strings array
```

Keep:

```ts
private refPackets =
  new WeakMap<BridgeViewNode, Uint32Array>();
```

A packet contains:

```text
transaction header
root_count=1
REF
root NodeId
```

Then:

```ts
if (knownNodes.has(root)) {
  let packet = refPackets.get(root);

  if (packet === undefined) {
    packet = createSingleRootRefPacket(root.id);
    refPackets.set(root, packet);
  }

  invokeNative(packet, EMPTY_STRINGS);
  return;
}
```

`WeakMap` does not keep discarded semantic nodes alive.

If native reports `PACKED_CACHE_MISS`, discard the `knownNodes` knowledge and cold-retry.

The cached packet itself must only be used while:

```text
knownNodes.has(root)
```

is true.

---

# 29. SHARED_PATH must stop at stable child identity in JS

The encoder must inspect identity before child payload.

For:

```text
new root R2
├── huge old shared subtree S
└── new changed leaf X2
```

the packed work should be:

```text
DEF R2
    REF S
    DEF X2
```

No descendant of `S` may be visited by the encoder.

This must be counter-tested.

For a 10,000-node `S`, encoded structural work must be approximately the same as for a 20-node `S`.

---

# 30. Add a deep shared-path workload

The historical shared test was an unusually easy shallow shape:

```text
root
├── huge shared subtree
└── changed leaf
```

Keep it, but add:

```text
root changed
└── level 1 changed
    ├── stable sibling
    └── level 2 changed
        ├── stable sibling
        └── ...
            └── changed leaf
```

Use depths:

```text
4
16
64
```

with large stable sibling subtrees.

Expected packed encoder cost:

```text
O(depth)
```

not:

```text
O(total retained tree)
```

This proves structural retention under realistic builder chains.

---

# 31. Full-schema correctness fixtures come before benchmarks

Do not run performance measurements until Candidate B passes direct-versus-packed differential tests for all node families.

Minimum fixture matrix:

```text
Text
    plain
    multi-span
    Unicode
    every wrap
    every horizontal alignment
    styled spans
    explicit false attributes

Diff
    context/addition/deletion
    terminated/unterminated

Spacer

Row
    normal/fixed/flex

Column
    normal/fixed/flex/flexMax/contentMax

Hanging

Grid
    every track
    spanning cells
    every alignment
    row/column gaps

Container

Clamp
    none
    ellipsis
    footer

ContentMax

Component

Decorated
    padding
    foreground
    background
    borders
    custom glyphs
    style ref/theme
    style states
    width/height
    all min/max dimensions
```

Compare more than `screenRows()`.

Compare:

```text
plain text cells
styles
alignment
cell positions
history output where relevant
component behavior
clipping/overflow
```

---

# 32. Add randomized differential trees

Use a deterministic PRNG.

Generate valid trees with:

```text
depth <= 6
1-8 children
random node kinds
random Unicode strings
random decoration combinations
random grids
random wrapping
random style states
shared DAG children
```

For each tree render:

```text
Candidate A fresh
Candidate B fresh
```

at widths/heights:

```text
1×1
2×4
7×5
20×10
80×24
121×37
```

Compare physical output.

Use fixed seeds in CI.

For example:

```text
0x00000001
0x12345678
0xdeadbeef
0xcafebabe
```

When a failure occurs, log the seed and generated semantic tree.

---

# 33. Explicit weak-cache-expiry test

This test prevents the old shortcut from returning.

Add a `perf-packed-benchmark`-only native cache reset helper:

```text
tuiPerfResetViewBridgeCache()
```

Test:

```text
1. packed-render root A
2. Candidate-B JS WeakSet now believes A is known
3. clear native ViewBridgeCache only
4. packed-render exact A again
```

Expected:

```text
first attempt:
    REF A
    native cache miss

JS:
    records one cache resync
    resets known WeakSet

second attempt:
    full definition A

native:
    succeeds

physical output:
    identical
```

Assert:

```text
exactly 1 recovery
not infinite retries
host mutates exactly once
```

---

# 34. Test native cache expiry without an artificial reset too

Construct transient roots until normal weak-cache pruning occurs and old Rust roots can die.

Then resurrect an older JS `View` which Candidate B still has as an object.

Expected behavior is the same:

```text
optimistic REF
→ possible cache miss
→ one cold resync
→ correct output
```

The transport must remain correct regardless of whether the weak cache has forgotten the node.

---

# 35. NodeId-width tests

Mandatory cases:

```text
1
2^32 - 1
2^32
2^32 + 1
2^53 - 1
```

Test:

```text
TS split
→ packed words
→ Rust reconstruct
→ exact equality
```

Also reject:

```text
0 for NodeId
2^53
negative
fractional
NaN
Infinity
```

The historical u32 truncation must be impossible to reintroduce.

---

# 36. Stable-root allocation test

After warming a single root, run 10,000 exact-identity commits.

Instrument JS.

Required:

```text
encoder_nodes_visited = 10,000
    // root only, one per operation

definition_records = 0
string_entries = 0
scratch_buffer_grows = 0
transaction_buffer_allocations = 0
```

The cached small REF packets must be reused.

If `new Uint32Array(...)` executes per identical commit, reject the tranche.

---

# 37. SHARED_PATH complexity test

Warm:

```text
S = 10,000-node subtree
R0 = root(S, leaf0)
```

Then perform:

```text
R1 = root(S, leaf1)
...
R1000
```

Counters per operation should approximate:

```text
nodes inspected        3
REF records            1
DEF records            2
strings                 changed leaf only
```

The exact count may differ according to wrapper shapes, but:

```text
20-node S
2,000-node S
10,000-node S
```

must produce the same asymptotic encoder work.

---

# 38. Candidate-B counters

Add explicit counters.

### JS encoder

```text
packed_encoder_nodes_visited
packed_encoder_ref_records
packed_encoder_def_records
packed_encoder_words_used
packed_encoder_strings
packed_encoder_string_bytes
packed_encoder_buffer_grows
packed_encoder_ref_packet_hits
packed_encoder_cache_resyncs
packed_encoder_cold_retries
```

### Native

```text
napi_packed_nodes_seen
napi_packed_ref_hits
napi_packed_ref_misses
napi_packed_defs_decoded
napi_packed_words_read
napi_packed_string_bytes_copied
```

Do not reuse direct counters in ambiguous ways.

The benchmark result must be able to explain *why* one candidate won.

---

# 39. Benchmark modes

Retain the four original modes, but define them correctly.

## COLD

Meaning:

```text
process/JIT warm
encoder scratch warm
semantic/native identity cache cold
fresh semantic NodeIds
```

Candidate A:

```text
fresh NodeIds
native bridge cache contains none of them
```

Candidate B:

```text
fresh NodeIds
knownNodes contains none of them
native bridge cache contains none of them
```

This isolates cold transport cost rather than first-allocation cost.

---

## FIRST_USE

Add a separate startup benchmark:

```text
fresh encoder
fresh scratch
fresh host
fresh cache
fresh tree
```

This intentionally includes first buffer growth/setup.

Do not conflate it with COLD.

---

## IDENTICAL_IDENTITY

Setup outside measurement:

```text
one semantic root
one host
warm it repeatedly
```

Recorded sample:

```text
same exact BridgeViewNode root every time
```

Candidate A expectation:

```text
1 NodeId lookup
1 WeakView hit
0 descendant reads
```

Candidate B:

```text
1 cached REF packet
1 native REF lookup
0 definitions
0 strings
0 scratch allocation
```

---

## SHARED_PATH

Warm the stable subtree before measurement.

Every measured update creates:

```text
new changed path
same stable subtree identity
```

Both transports must stop at exactly the same identity boundary.

---

## REBUILT_EQUIVALENT

Persistent warm process/host.

Each sample builds semantically equal but **entirely new identities**.

Candidate A:

```text
cache misses for new IDs
```

Candidate B:

```text
no WeakSet hits for new objects
full definitions
```

This is intentionally different from COLD because the host/process/cache infrastructure remains warm.

---

# 40. Explicit warm-up phases

Every benchmark function must visibly contain:

```ts
for (let index = 0; index < WARMUP; index++) {
    runUntimedSample(...);
}

resetPerfCounters();
resetRecordedSamples();

for (let index = 0; index < MEASURED; index++) {
    runTimedSample(...);
}
```

No prose-only warmup.

Authoritative:

```text
warm-up >= 20
measured >= 200
```

For p99:

```text
>= 1000 samples
```

Otherwise mark p99 informational.

Do not repeat the 20-sample p95 decision.

---

# 41. Benchmark actual transport commit separately from rendering

Primary metric:

```text
View construction required by mode
+
Candidate B JS encoding where applicable
+
N-API crossing
+
Rust cache/decode
+
host retained View commit / dirtying
+
return
```

Call this:

```text
commit_ns
```

Do **not** synchronously render the frame as part of the primary transport decision.

Candidate selection concerns the boundary.

Then separately measure:

```text
commit
+
explicit advance(0)
+
layout
+
paint
```

as:

```text
forced_frame_ns
```

This answers whether the transport improvement is significant at the user-visible frame level.

---

# 42. Benchmark component timings too

For Candidate B record:

```text
construction_ns
encoding_ns
napi_and_native_ns
total_commit_ns
```

For Candidate A:

```text
construction_ns
napi_and_native_ns
total_commit_ns
```

The decision uses **total commit**, not the isolated encoder/native numbers.

Component timings are diagnostic only.

---

# 43. Workload matrix must represent the full schema

The historical benchmark only used plain Text + Column. That must not happen again. 

Run at least:

```text
plain_text_column
styled_span_heavy
row_heavy
column_track_heavy
grid_heavy
decoration_heavy
diff_heavy
component_heavy
mixed_realistic
```

At:

```text
~20 nodes
~200 nodes
~2,000 nodes
~10,000 nodes
```

Where a particular semantic structure doesn't naturally map to exact node counts, report the exact actual node count.

---

# 44. Add two SHARED workloads

```text
SHARED_WIDE
SHARED_DEEP
```

The first proves large-subtree cutoff.

The second proves changed-path scaling.

These are more important to real retained UI usage than REBUILT_EQUIVALENT.

---

# 45. Add a realistic update trace

A production UI does not perform one mode forever.

Add:

```text
1 cold initial tree

then 1,000 operations approximately:
    many SHARED_PATH updates
    some IDENTICAL_IDENTITY submissions
    occasional rebuilt/equivalent sections
    occasional large replacement
```

Do not invent percentages from thin air if actual application instrumentation is available later.

For now use a declared synthetic mix, e.g.:

```text
70% SHARED_PATH
20% IDENTICAL_IDENTITY
8% REBUILT_EQUIVALENT smaller sections
2% large replacement
```

Label this explicitly as **synthetic**, not production telemetry.

Report total CPU and total elapsed commit time over the trace.

---

# 46. Alternate candidate order

Do not execute:

```text
all A
then all B
```

for an hour and compare.

Within a benchmark case, deterministically alternate:

```text
A B
B A
A B
B A
```

using independently warmed candidate state.

This reduces thermal/JIT/background bias.

Never let A populate B's semantic cache or vice versa.

Use explicit perf-only native cache resets/environment isolation.

---

# 47. Raw samples are mandatory

For every authoritative case retain:

```json
{
  "benchmark_version": "PERF-7v2",
  "candidate": "direct|packed",
  "mode": "...",
  "workload": "...",
  "node_count": 10000,
  "git_sha": "...",
  "warmup_iterations": 20,
  "measured_iterations": 200,
  "samples_ns": [],
  "median_ns": 0,
  "p95_ns": 0,
  "counters": {},
  "bun_version": "...",
  "rustc_version": "...",
  "target": "...",
  "profile": "release"
}
```

The result note must be reproducible directly from these raw samples.

---

# 48. Statistical result

For median and p95 also compute bootstrap confidence intervals.

The exact resampling implementation is less important than preserving raw observations.

Do not call:

```text
packed = -0.7%
```

a meaningful regression if measurement uncertainty is ±8%.

Likewise do not call +4% a win if it lies inside noise.

---

# 49. Memory measurement

Candidate B introduces retained JS encoder state.

Prove:

```text
WeakSet/WeakMap do not retain dead View DAGs
```

and:

```text
scratch capacity follows largest active transaction,
not total lifetime NodeIds
```

Track:

```text
JS heap after GC-capable test points
RSS
native ViewBridgeCache entries
scratch word capacity
```

A single large transaction leaving one reusable large scratch buffer is acceptable.

A cache growing one entry for every View ever seen is not.

---

# 50. Do not implement the UTF-8 arena yet

First compare:

```text
Uint32Array
+
string[]
```

correctly.

Only if Candidate B:

```text
wins
```

or:

```text
lands very close while profiling shows N-API string conversion is dominant
```

run the second experiment:

```text
Uint32Array structure
+
Uint8Array UTF-8 arena
```

The original handoff was correct about this sequencing. 

---

# 51. If string arena is tested

It must still preserve identity cutoff.

For a warm REF:

```text
0 bytes encoded into UTF-8 arena
```

For SHARED_PATH:

```text
only strings belonging to changed definitions
```

Do not re-UTF8-encode stable subtree strings.

And include JS `TextEncoder` CPU in total latency.

---

# 52. Production decision gate

Do not decide from COLD alone.

Do not decide from IDENTICAL alone.

Candidate B may enter production only after:

```text
all correctness gates pass

AND

IDENTICAL + SHARED:
    no material statistically credible regression

AND

full-schema 2k/10k:
    packed improvement is repeatable

AND

synthetic realistic trace:
    total commit latency improvement >= 5%

AND

memory/lifetime:
    no cumulative retention bug
```

Use the original complexity rule on the realistic/full-suite result:

```text
<5%:
    reject packed transport

5-15%:
    keep only if complexity is judged manageable

>=15%:
    strong production candidate
```

But add one additional rule:

> A ≥15% COLD win does not compensate for a significant regression on the normal retained SHARED_PATH unless trace-level results still clearly win.

Conversely:

> If IDENTICAL/SHARED are statistically neutral and large COLD/REBUILT/full-schema cases are consistently ≥15% faster, Candidate B should not be rejected merely because its warm advantage is near zero.

That is the decision the old experiment failed to answer fairly.

---

# 53. Productionization only happens after the benchmark decision

Keep Candidate B behind:

```text
perf-packed-benchmark
```

through the experiment.

If it loses:

```text
delete implementation
keep PERF-7v2 result + test rationale as appropriate
```

If it wins:

make **a separate production commit**.

Do not silently make benchmark code production code.

---

# 54. If B wins, it must replace every View-bearing N-API transport

Do not optimize only:

```text
Tui.render()
```

and call the transport migrated.

Inventory all private View crossings, including at minimum:

```text
Tui.render

History.push
History.freeze

ViewSlot initial value
ViewSlot.setView
ViewSlot.setAnimation
ViewSlot.stopAnimation

ScrollPane initial content
ScrollPane.setContent
```

and any additional `BridgeViewNode`/`nodeForBridge()` call sites present at implementation time.

Do a code search before editing.

---

# 55. Support a forest transaction

`ViewSlot.setAnimation()` supplies multiple Views.

Therefore make the wire header support:

```text
root_count >= 1
```

from the beginning.

Single-View operation:

```text
root_count = 1
```

Animation:

```text
root_count = frame count
```

All roots share:

```text
same word buffer
same string table
same identity table
```

If two animation frames share subtrees, Candidate B should preserve that sharing within the same transaction.

---

# 56. Still one N-API state mutation

If Candidate B wins, do **not** implement:

```text
pack
→ napi decodeView()
→ NativeTuiView handle
→ napi history.push(handle)
```

That restores the intermediate-object design PERF-6 intentionally removed.

Each semantic mutation remains one call:

```text
History.pushPacked(words, strings)
ViewSlot.setViewPacked(words, strings)
Tui.renderPacked(words, strings)
...
```

Internally all those thin N-API methods call the same:

```rust
decode_packed_roots(...)
```

implementation.

---

# 57. Do not keep two production transports forever

After production validation:

```text
packed wins:
    packed becomes normal private View bridge
    direct structured decoder remains test-only temporarily if needed for differential tests, then remove from production path

direct wins:
    delete packed production implementation
```

Do not add configuration saying:

```text
ION_TUI_TRANSPORT=direct|packed
```

as permanent architecture.

That would double the correctness surface.

---

# 58. Mandatory acceptance tests

An implementation agent is not allowed to mark 7v2 complete until all of these pass:

```text
[ ] full BridgeViewNode schema packed
[ ] every node/decorative semantic field differential-tested
[ ] NodeId > 2^32 round-trips
[ ] NodeId MAX_SAFE round-trips
[ ] native packed decoder uses same environment ViewBridgeCache
[ ] no NativeTuiHost.packed_cache exists
[ ] JS known state uses weak object identity
[ ] native WeakView expiry triggers one cold recovery
[ ] cold recovery does not partially mutate host
[ ] second cache miss after cold recovery is a hard error
[ ] DAG duplicates become backward REFs
[ ] cyclic/malformed transaction rejected
[ ] IDENTICAL emits only root REF
[ ] IDENTICAL allocates no new transaction buffer after warmup
[ ] SHARED_PATH does not visit stable descendants
[ ] 20/2k/10k shared subtree sizes produce constant encoder work
[ ] explicit benchmark warmup exists in code
[ ] >=200 recorded samples in authoritative mode
[ ] COLD and REBUILT have different cache semantics
[ ] raw benchmark samples retained
[ ] JS encode time included
[ ] CPU and memory included
[ ] complete full-schema workload matrix run
[ ] SHARED_WIDE and SHARED_DEEP run
[ ] realistic trace run
```

---

# 59. Banned shortcuts

Reject the tranche immediately if an agent writes any equivalent of:

```ts
const known = new Set<number>();
```

without a weak/lifetime strategy and native cache resynchronization.

Reject:

```rust
struct NativeTuiHost {
    packed_cache: ...
}
```

Candidate B must share Candidate A's environment cache.

Reject:

```ts
words.push(node.id);
```

where NodeId occupies one `u32`.

Reject:

```ts
node.spans.map(...).join("")
```

as the packed Text representation.

Reject:

```ts
const words: number[] = [];
return new Uint32Array(words);
```

on every steady commit.

Reject:

```text
packed supports Text and Column
→ benchmark it
→ decide production architecture
```

Reject:

```text
20 samples
→ p95 architecture decision
```

Reject a result document claiming “post-warmup” without a visible benchmark warm-up loop.

Reject:

```text
SHARED_PATH
→ JS recursively walks 10k stable subtree merely to discover every ID is known
```

Reject:

```text
cache miss
→ keep Rust Views strongly forever
```

The correct response is cold transport resynchronization.

---

# 60. Expected steady-state traces

These should be written directly into tests/comments.

## Exact identity

```text
TS:
same root object R

Packed encoder:
knownNodes.has(R)
→ reuse cached REF packet

Wire:
HEADER
REF R

Native:
WeakView[R] hit
→ View

Host:
commit body
→ dirty
→ return
```

No children.

No strings.

No typed-array allocation.

---

## Shared subtree

```text
R1
├── S
└── X1

after warmup

R2
├── S
└── X2
```

Packed:

```text
DEF R2
    REF S
    DEF X2
```

Native:

```text
decode R2
    cache-hit S
    construct X2

construct R2
cache R2
commit
```

The size of `S` must not affect transport work.

---

## Weak-cache resurrection

```text
JS still owns R
native WeakView[R] expired

encoder:
    REF R

native:
    miss

JS:
    clear known transport state

encoder:
    DEF entire current R DAG

native:
    reconstruct
    weak-cache all definitions
    commit once

JS:
    mark transmitted nodes known
```

Correctness restored automatically.

---

# 61. Recommended implementation sequence

Do not give one agent the whole thing and let them announce completion in one commit.

Use these sub-tranches:

```text
PERF-7v2.0
    restore historical experiment only as reference
    add corrected benchmark framework
    add packed counters
    no architectural decision

PERF-7v2.1
    versioned packed wire format
    53-bit IDs
    reusable typed-array writer
    REF + DEF
    same environment native cache
    weak JS transport knowledge
    cache-miss recovery

PERF-7v2.2
    complete View schema
    exhaustive direct-vs-packed fixtures
    randomized differential tests

PERF-7v2.3
    root REF packet memoization
    SHARED cutoff
    scratch-buffer reuse
    allocation/lifetime tests

PERF-7v2.4
    authoritative benchmark
    full workload matrix
    realistic trace
    write evidence-backed decision

PERF-7v2.5 — only if B wins
    migrate every production View boundary
    full suite + long-session test
    remove losing production transport
```

Suggested commits:

```text
bench(tui): reopen retained View transport evaluation
feat(native): add cache-equivalent packed View transaction
test(tui): prove packed View schema and identity parity
perf(runtime): retain packed View transport state without steady allocations
bench(tui): complete PERF-7v2 transport decision
```

And only if it wins:

```text
perf(tui): adopt packed retained View transport
```

---

# 62. What the final PERF-7v2 result document must say

Require the implementation agent to report:

```text
Exact candidate SHAs

Exact protocol version

Correctness:
    full schema parity status
    randomized seeds run
    weak-cache recovery status
    NodeId-width status

For every workload/mode:
    direct median/p95
    packed median/p95
    confidence intervals
    relative change

Packed structural counters:
    encoder nodes visited
    REF records
    DEF records
    words
    strings
    buffer growth

Native counters:
    direct cache hits/misses
    packed REF hits/misses
    packed definitions

Memory:
    heap
    RSS
    scratch high-water
    native bridge-cache size

Synthetic trace:
    total direct commit CPU/time
    total packed commit CPU/time

Final decision:
    A or B

Decision rule:
    exactly which 5%/15% condition was satisfied or failed
```

No sentence such as:

> “Packed was slower on cache hits.”

is acceptable without showing that its warmed transaction was actually:

```text
one reusable REF packet
+
one native weak-cache lookup
```

Likewise:

> “Packed wins because cold is 60% faster”

is unacceptable if shared-path production updates regress materially.

---

## Bottom line

The first PERF-7 **did uncover a potentially important signal**: structural packing was very fast when native had to consume thousands of new nodes. But the experiment then compared that prototype against a mature direct path under conditions where the direct path already had its complete retained identity architecture, while packed Candidate B still had benchmark-only identity bookkeeping, a different native cache lifetime, avoidable per-call buffer allocation, only 32-bit IDs, incomplete schema support, and no weak-cache recovery. 

So I would explicitly change the original handoff status from:

```text
PERF-7: complete; Candidate A won
```

to:

```text
PERF-7: superseded / evidence invalid for production decision

PERF-7v2 required:
compare direct structured N-API traversal against a FULLY
identity-retaining, weak-cache-correct, allocation-conscious,
full-schema packed transaction under the repaired benchmark oracle.
```

The most important 7v2 insight is that **Candidate B must not merely “have a REF opcode.”** It needs a complete cache architecture: weak JS knowledge, the same environment-native WeakView cache, O(1) exact-root transactions, O(changed-path) shared updates, safe recovery when native retention disappears, and no steady transaction allocation that Candidate A inherently avoids. Only after that implementation exists is the direct-vs-packed benchmark actually answering the intended PERF-7 question.
