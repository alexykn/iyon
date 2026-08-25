# CORE-1: Collapse architecture to 3 packages + plugins

## Motivation

The current architecture has too many layers of indirection:

```
Rust:           crates/iyon-api  →  crates/iyon-core  →  crates/iyon  (binary)
                                                          crates/iyon-core-native  (napi bridge)
                                    ↓
TypeScript:     packages/iyon-sdk  (types only, re-exports everything)
                                    ↓
                packages/iyon-runtime  (wraps native addon, orchestrates)
                                    ↓
                packages/iyon-plugins  (extension system)
                                    ↓
                packages/iyon-cli  (thin CLI entry)
                                    ↓
                plugins/agents/iyon  (agent plugin)
                plugins/app/iyon       (app plugin)
                plugins/providers/* (providers)
                plugins/tools/*     (tools)
```

Problems:
- `iyon-sdk` is pure types with no behavior — a pass-through package
- `iyon-runtime` is a thin wrapper around the native addon, plus some orchestration
- `iyon-cli` is ~50 lines of bootstrap
- The actual kernel logic (agent loop, model turn, tool lifecycle, session management) is split between Rust (`crates/iyon-core`) and TS (`packages/iyon-runtime` + `plugins/agents/iyon`)
- The native addon (`crates/iyon-core-native`) is the only reason Rust is in the critical path for core logic
- The app is a plugin but the boundary is fuzzy — the app plugin imports from `@iyon/runtime` and `@iyon/sdk` and `@iyon/plugins`, creating a complex dependency graph

## Target Architecture

```
packages/iyon-api        protocol types, ModelApi, provider auth, credential store
packages/iyon-core       KernelSession, ModelTurn, ToolExecution, agent loop,
                         runtime event loop, CLI entry, SDK (defineTool, etc.)
packages/iyon-plugins    extension system, registries, loader, discovery, scene

plugins/app/iyon         the app itself (scenes, state, actions, theme, views)
plugins/agents/iyon      agent logic (already a plugin!)
plugins/providers/*      provider implementations
plugins/tools/*          tool implementations
```

**3 packages. Everything else is a plugin. The app is a plugin.**

## What Changes

### 1. `packages/iyon-api` — absorbs `@iyon/sdk` types + provider auth

**Current contents** (very thin):
- `src/api.ts` — protocol types, `ModelApi` interface, `ModelStreamEvent`, etc.
- `src/providers.ts` — `ProviderDefinition`, `CredentialStore`, auth context types
- `src/core.ts` — kernel event types, branded IDs, session entry types
- `src/tool.ts` — `defineTool()`, `Tool` interface, `ToolContext`, `WorkspaceHandle`
- `src/index.ts` — re-exports everything

**Target**: absorb the types from `iyon-sdk` and become the single protocol/API package.

**What goes in**:
- `ModelApi`, `ModelStreamEvent`, `ContentBlock`, `ModelMessage`, `ModelRequest`, `ModelToolSpec`, `StopReason`, `Usage`, `ReasoningLevel`, `ModelError` — already here
- `ProviderDefinition`, `CredentialStore`, `ProviderAuthContext`, `ProviderAuthHooks`, `ProviderCapabilities`, `ProviderModel` — already here
- `CoreEvent`, `MessageDelta`, `MessageRole`, `ToolCallDelta`, `ToolUpdateEvent`, `SessionSnapshot`, `SessionEntry`, `TranscriptMessage`, `ModelTurnResult`, `AssembledToolCall`, `ToolExecutionRequest`, `ToolResult`, `ApprovalState`, `ApprovalRequirement`, `ApprovalStatus`, `ToolLifecycleEvent`, `ToolLifecycleState`, `ModelTurnState`, `ModelTurnOptions` — move from `@iyon/sdk/src/core.ts`
- `defineTool()`, `Tool`, `ToolCall`, `ToolContext`, `ToolDefinition`, `ToolMetadata`, `ToolResult`, `ToolUpdateSink`, `ToolExecutionMode`, `ToolApprovalPolicy`, `WorkspaceHandle` — move from `@iyon/sdk/src/tool.ts`
- Provider selection logic — move from `@iyon/runtime/src/providers/selection.ts`
- Credential store implementation — move from `@iyon/runtime/src/credentials.ts`

**What goes out**:
- Nothing, this package is being enriched

### 2. `packages/iyon-core` — absorbs `@iyon/runtime` + `@iyon/cli` + Rust kernel

**Current contents** (scattered across packages + Rust crates):

| Logic | Current Location |
|---|---|
| KernelSession (message storage, ID allocation, entries) | Rust `crates/iyon-core/src/kernel/session.rs` + TS wrapper `packages/iyon-runtime/src/modules/core.ts` |
| ModelTurn (stream assembly, event emission, cancellation) | Rust `crates/iyon-core/src/kernel/model_turn.rs` + TS wrapper `packages/iyon-runtime/src/modules/core.ts` |
| ToolExecution / ToolLifecycleHandle (state machine) | Rust `crates/iyon-core/src/kernel/tool.rs` + TS wrapper `packages/iyon-runtime/src/modules/core.ts` |
| Approval state machine | Rust `crates/iyon-core/src/kernel/approval.rs` |
| Queue system (prompt/steer/follow-up) | Rust `crates/iyon-core/src/kernel/queue.rs` |
| Agent loop (orchestration) | Rust `crates/iyon-core/src/agent/loop.rs` + TS `plugins/agents/iyon/src/agent.ts` |
| Runtime event loop (command processing) | Rust `crates/iyon-core/src/runtime.rs` |
| Virtual modules (iyon:api, iyon:core, iyon:tui, iyon:plugins) | `packages/iyon-runtime/src/virtual-modules.ts` |
| Bootstrap (provider discovery, registration) | `packages/iyon-runtime/src/bootstrap/` |
| Tool execution, contracts, policy, approval | `packages/iyon-runtime/src/tools/` |
| CLI entry (auth, run, selection) | `packages/iyon-cli/src/` + `crates/iyon/src/main.rs` |
| Credentials native addon | `crates/iyon-core-native/src/` |
| IyonCore handle, CoreCommand, CoreEvent | Rust `crates/iyon-core/src/handle.rs`, `command.rs`, `event.rs` |

**Target**: single package containing all core runtime logic, ported from Rust to TypeScript.

**What goes in**:
- Port of `KernelSession` from Rust `kernel/session.rs` to TS
- Port of `ModelTurn` from Rust `kernel/model_turn.rs` to TS
- Port of `ToolExecution` / `ToolLifecycleHandle` from Rust `kernel/tool.rs` + `kernel/approval.rs` to TS
- Port of `KernelQueues` from Rust `kernel/queue.rs` to TS
- Port of `Kernel` and `KernelConfig` from Rust `kernel/kernel.rs` to TS
- Port of agent loop from Rust `agent/loop.rs` + `agent/turn.rs` + `agent/tool_execution.rs` + `agent/request.rs` + `agent/control.rs` to TS
- Port of runtime event loop from Rust `runtime.rs` + `handle.rs` + `session/state.rs` to TS
- Virtual modules setup (from `packages/iyon-runtime/src/virtual-modules.ts`)
- Bootstrap (from `packages/iyon-runtime/src/bootstrap/`)
- Tool execution, contracts, policy, approval (from `packages/iyon-runtime/src/tools/`)
- CLI entry (from `packages/iyon-cli/src/` + `crates/iyon/src/main.rs`)
- `defineTool()` and SDK exports (from `packages/iyon-sdk/src/tool.ts`)
- Provider selection (from `packages/iyon-runtime/src/providers/`)
- Smoke tests and utilities (from `packages/iyon-runtime/src/smoke.ts`)

### 3. `packages/iyon-plugins` — stays as is

This package is already clean. It owns:
- Extension system (loading, activation, deactivation)
- Registries (tools, providers, agents, apps, commands, shortcuts, scene)
- Package discovery, manifest parsing, compatibility
- Scene extensions (compose/replace)
- Tool support utilities (output, process, mutations, diff, render)

**No changes** except import paths may need updating.

### 4. Plugin packages — minor import path updates

All plugins currently import from `@iyon/sdk`, `@iyon/runtime`, and `@iyon/plugins`. After the collapse:
- `@iyon/sdk` types → `@iyon/api` (or `@iyon/sdk` becomes a re-export)
- `@iyon/runtime` → `@iyon/core`

## Detailed Porting Plan

### Phase 1: Enrich `packages/iyon-api`

1. Move all types from `@iyon/sdk/src/core.ts` into `packages/iyon-api/src/core.ts`
2. Move `defineTool()` and Tool types from `@iyon/sdk/src/tool.ts` into `packages/iyon-api/src/tool.ts`
3. Move provider selection from `@iyon/runtime/src/providers/selection.ts` into `packages/iyon-api/src/providers/selection.ts`
4. Move credential store from `@iyon/runtime/src/credentials.ts` into `packages/iyon-api/src/credentials.ts`
5. Update `packages/iyon-api/src/index.ts` to export everything

### Phase 2: Port Rust kernel to `packages/iyon-core`

**File-by-file port from Rust to TypeScript:**

| Rust source | → | TS target |
|---|---|---|
| `crates/iyon-core/src/ids.rs` | → | `packages/iyon-core/src/kernel/ids.ts` |
| `crates/iyon-core/src/event.rs` | → | `packages/iyon-core/src/kernel/event.ts` |
| `crates/iyon-core/src/command.rs` | → | `packages/iyon-core/src/kernel/command.ts` |
| `crates/iyon-core/src/kernel/session.rs` | → | `packages/iyon-core/src/kernel/session.ts` |
| `crates/iyon-core/src/kernel/model_turn.rs` | → | `packages/iyon-core/src/kernel/model-turn.ts` |
| `crates/iyon-core/src/kernel/tool.rs` | → | `packages/iyon-core/src/kernel/tool.ts` |
| `crates/iyon-core/src/kernel/approval.rs` | → | `packages/iyon-core/src/kernel/approval.ts` |
| `crates/iyon-core/src/kernel/queue.rs` | → | `packages/iyon-core/src/kernel/queue.ts` |
| `crates/iyon-core/src/kernel/kernel.rs` | → | `packages/iyon-core/src/kernel/kernel.ts` |
| `crates/iyon-core/src/agent/loop.rs` | → | `packages/iyon-core/src/agent/loop.ts` |
| `crates/iyon-core/src/agent/turn.rs` | → | `packages/iyon-core/src/agent/turn.ts` |
| `crates/iyon-core/src/agent/tool_execution.rs` | → | `packages/iyon-core/src/agent/tool-execution.ts` |
| `crates/iyon-core/src/agent/request.rs` | → | `packages/iyon-core/src/agent/request.ts` |
| `crates/iyon-core/src/agent/control.rs` | → | `packages/iyon-core/src/agent/control.ts` |
| `crates/iyon-core/src/agent/transcript.rs` | → | `packages/iyon-core/src/agent/transcript.ts` |
| `crates/iyon-core/src/agent/tool_call.rs` | → | `packages/iyon-core/src/agent/tool-call.ts` |
| `crates/iyon-core/src/runtime.rs` | → | `packages/iyon-core/src/runtime.ts` |
| `crates/iyon-core/src/handle.rs` | → | `packages/iyon-core/src/handle.ts` |
| `crates/iyon-core/src/session/state.rs` | → | `packages/iyon-core/src/session/state.ts` |

### Phase 3: Merge runtime packages

1. Merge `packages/iyon-runtime/src/tools/` into `packages/iyon-core/src/tools/`
2. Merge `packages/iyon-runtime/src/bootstrap/` into `packages/iyon-core/src/bootstrap/`
3. Merge `packages/iyon-runtime/src/virtual-modules.ts` into `packages/iyon-core/src/virtual-modules.ts`
4. Merge `packages/iyon-runtime/src/smoke.ts` into `packages/iyon-core/src/smoke.ts`
5. Merge `packages/iyon-runtime/src/modules/` into `packages/iyon-core/src/kernel/`
6. Merge `packages/iyon-cli/src/` into `packages/iyon-core/src/cli/`

### Phase 4: Eliminate native addon

1. Replace credential operations (keyring) with a pure TS implementation (e.g., encrypted file in `~/.config/iyon/credentials.json`)
2. OR keep a minimal `iyon-credentials` native addon (just `credentialGet/Set/Delete/Has`)
3. Remove `crates/iyon-core-native/`
4. Remove `crates/iyon-api/` (all types now in TS)
5. Remove `crates/iyon-core/` (all logic now in TS)
6. Remove `crates/iyon/` (binary entry now in TS)

### Phase 5: Update import paths

After the collapse, the import map should be:

| Old import | New import |
|---|---|
| `@iyon/sdk` | `@iyon/api` (for types) or `@iyon/core` (for `defineTool`) |
| `@iyon/runtime` | `@iyon/core` |
| `@iyon/cli` | `@iyon/core` |
| `@iyon/plugins` | `@iyon/plugins` (unchanged) |
| `@iyon/tui` | `@iyon/tui` (unchanged, stays Rust) |

## Key Design Decisions

### 1. Agent loop is a plugin, not core

The agent loop (`plugins/agents/iyon/src/agent.ts`) is already a plugin — it registers itself via `api.agents.register()`. The core should be agnostic to which agent loop runs. The core provides:
- `KernelSession` — manages conversation state, message storage, event emission
- `ModelTurn` — processes a single model turn from a provider stream
- `ToolExecution` — manages tool lifecycle state machine
- Tool registry — `@iyon/plugins` owns this
- Provider selection — `@iyon/api` owns this

The agent loop plugin uses these primitives to implement the turn-taking policy. This is already the pattern in the TS agent plugin — it imports `KernelSession`, `ModelTurn`, `ToolExecution` from `@iyon/runtime` and orchestrates them.

### 2. `iyon-api` is the protocol boundary

`iyon-api` owns:
- `ModelApi` interface (the contract between providers and the core)
- All protocol types (`ModelStreamEvent`, `ContentBlock`, `ModelMessage`, etc.)
- `ProviderDefinition` (the contract between provider plugins and the runtime)
- `CredentialStore` (the contract between auth flows and secure storage)
- Provider selection (how to pick a provider based on environment/config)

This is the "API" in the sense that it defines the protocol contracts. It has no runtime behavior — just types and pure functions.

### 3. `iyon-core` is the runtime

`iyon-core` owns:
- `KernelSession` — the conversation state machine
- `ModelTurn` — streaming model turn processor
- `ToolExecution` — tool lifecycle state machine
- Agent loop — the turn-taking policy (runs in the agent plugin, uses core primitives)
- Runtime event loop — the main event-driven orchestration
- CLI entry — `iyon` command with auth, run, etc.
- Virtual modules — `iyon:api`, `iyon:core`, `iyon:tui`, `iyon:plugins`
- `defineTool()` — SDK helper for tool authors
- Tool execution dispatch — running tools with hooks, approval, updates

### 4. The app is a plugin

`plugins/app/iyon` registers:
- Scenes (compose/replace via `api.scene`)
- Theme (`createIyonTheme`)
- State (`IyonState`, `reduceIyonState`)
- Actions (`handleIyonAction`)
- Backend event bridge (`CoreEventMapper`, `startCoreEventBridge`)
- Views (`IyonRootView`, `footerText`, `workingFrames`)

The app plugin loads like any other plugin. It declares its dependencies in `package.json` and registers its contributions via `activate(api)`. The core doesn't know about the app — it just provides the extension system and the runtime primitives.

## Benefits

1. **No native build for core logic** — eliminate `napi-rs`, `iyon-core-native.node`, Cargo build steps
2. **Single package to import** — `@iyon/core` gives you everything you need to run or extend Iyon
3. **Clean separation** — `iyon-api` = protocol, `iyon-core` = runtime, `iyon-plugins` = extension system
4. **App is a plugin** — consistent lifecycle, same loading, same registries, testable independently
5. **Simpler workspace** — 3 packages + plugins instead of 7 packages + 4 Rust crates + native addon
6. **Faster iteration** — no Rust compilation for core changes, full TypeScript debugging
7. **Easier contribution** — most developers know TS/JS, the Rust surface is just the TUI

## What Stays in Rust

- **The TUI framework** (`iyon-tui` + `@iyon/tui`) — the terminal UI rendering engine, view system, layout, themes, controls
- **Optional: credentials** — if we keep a small native addon for OS keyring access

Everything else moves to TypeScript.

## Files to Delete

After full migration:
- `crates/iyon-api/` (entire crate)
- `crates/iyon-core/` (entire crate)
- `crates/iyon-core-native/` (entire crate)
- `crates/iyon/` (entire crate)
- `packages/iyon-sdk/` (entire package)
- `packages/iyon-runtime/` (entire package)
- `packages/iyon-cli/` (entire package)

## Files to Create

- `packages/iyon-api/src/core.ts` — types from `@iyon/sdk/src/core.ts`
- `packages/iyon-api/src/tool.ts` — types from `@iyon/sdk/src/tool.ts`
- `packages/iyon-api/src/credentials.ts` — from `@iyon/runtime/src/credentials.ts`
- `packages/iyon-api/src/providers/selection.ts` — from `@iyon/runtime/src/providers/selection.ts`
- `packages/iyon-core/src/kernel/*.ts` — ports of Rust kernel
- `packages/iyon-core/src/agent/*.ts` — ports of Rust agent
- `packages/iyon-core/src/runtime.ts` — port of Rust runtime.rs
- `packages/iyon-core/src/handle.ts` — port of Rust handle.rs
- `packages/iyon-core/src/session/state.ts` — port of Rust session/state.rs
- `packages/iyon-core/src/cli/*.ts` — from `@iyon/cli` + Rust binary
- `packages/iyon-core/src/tools/*.ts` — from `@iyon/runtime/src/tools/`
- `packages/iyon-core/src/bootstrap/*.ts` — from `@iyon/runtime/src/bootstrap/`
- `packages/iyon-core/src/virtual-modules.ts` — from `@iyon/runtime`
- `packages/iyon-core/src/smoke.ts` — from `@iyon/runtime`

## Summary

```
Before:  7 packages + 4 Rust crates + native addon
After:   3 packages + plugins (app is a plugin)
```

The Rust surface shrinks to just the TUI. Everything else is TypeScript, organized into three clear packages: protocol (`iyon-api`), runtime (`iyon-core`), and extension system (`iyon-plugins`).

The app is a plugin. The agent loop is a plugin. Providers are plugins. Tools are plugins. The core is small, generic, and extensible.