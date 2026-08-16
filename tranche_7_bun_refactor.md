# Tranche 7 — Migrate bundled providers to Bun

## Mission

Move the bundled Mock, OpenRouter, and OpenAI Codex providers out of the product's Rust execution path and into ordinary Bun + TypeScript extensions. Each provider must implement the existing `iyon:api` model request and `ModelStreamEvent` contract, use Bun's native `fetch`, streams, timers, filesystem, browser, and process APIs where needed, and register through the same `iyon.providers.register` mechanism as a third-party extension. After parity is proven, the T1/T6 TypeScript runtime selects only registered TypeScript providers; the Rust provider implementations remain deprecated compatibility types in `iyon-api`, but are not constructed by that product path.

## Prerequisites

- T1 has produced a Bun-owned runtime/CLI path, `iyon-native` N-API loading, the `iyon:*` virtual modules, and a buildable native addon. T7 must not create a Rust-owned process, embed Bun in Rust, or replace the T1 native boundary.
- T2 has recorded the reachable Rust API dispositions, and T4 has exposed the request, message, content, error, reasoning, usage, stop-reason, and stream-event shapes through `iyon:api` / `@iyon/sdk`.
- T3 has made the native core provider-agnostic. T7 consumes the core's model boundary and selection/config events; it does not move agent, tool, or session behavior into provider packages.
- T5 has supplied the TypeScript-facing runtime/native handles needed by the existing product bootstrap. T7 does not port the TUI or alter scrollback/rendering behavior.
- T6 has supplied transactional package activation and the general contribution registry. Bundled providers must use that loader and registration API exactly like external packages; there is no second built-in loader or `is_builtin` path.
- The current Rust behavior in `crates/iyon-api/src/model.rs`, `stream.rs`, `providers/{mock,openrouter,openai_codex}.rs`, `crates/iyon/src/auth.rs`, and `crates/iyon/src/main.rs` is the migration oracle. The public inventories and `iyon.md` remain behavioral references.
- T7 assumes the package/workspace paths established by T1–T6 exist. If an earlier tranche chose a different file split, preserve its public contracts and apply the commit scopes below to the corresponding existing path; do not introduce a second runtime abstraction to accommodate the plan.

T7 must not assume the T8 TypeScript tools, T9 TypeScript agent, or T10 TypeScript app/CLI migration. It must provide a provider-capable runtime/bootstrap seam that those tranches can consume, but it may not make their registrations or UI decisions.

## Invariants (do not violate)

- Bun owns provider control flow and networking. Provider code uses `fetch`, `ReadableStream`, `TextDecoder`, timers, and other normal Bun APIs. Do not add a shared Iyon HTTP client, a Rust HTTP layer, WASM, Rhai, WIT, an embedded JS engine, or a Rust-owned process.
- `iyon:api` remains the canonical protocol. Provider implementations normalize provider wire events into the existing `ModelStreamEvent` sequence, including content indexes, tool-call deltas/ends, usage, stop reasons, and typed error categories. Do not invent a provider-specific event protocol for the agent.
- Preserve request serialization semantics from the Rust implementations: system prompts, text/images, assistant tool calls, tool results, tool schemas, temperature/max-token parameters, reasoning effort, cache metadata, session IDs, and user metadata.
- Preserve the current provider defaults and fallback semantics: `deepseek/deepseek-v4-flash:latest` for OpenRouter, `gpt-5.3-codex` for Codex, `mock`/`mock` for Mock; trimmed case-insensitive `IYON_PROVIDER` aliases; unknown explicit values map to OpenRouter; absent selection prefers OpenRouter credentials, then Codex credentials, then Mock; selected-but-unavailable providers fall back to Mock with a visible warning.
- Preserve fragmented SSE handling. A frame may be split at any byte boundary, contain comments, multiple `data:` lines, blank lines, `[DONE]`, or a JSON object split across chunks. Provider parsers must retain incomplete data and must not decode one network chunk as one event.
- Retry only the existing transient classes and preserve bounded backoff. Do not retry invalid requests, authentication failures, provider protocol failures, or stream parse failures. Never log API keys, access tokens, refresh tokens, or credential files.
- Provider-specific authentication orchestration, model defaults, model discovery, and capability tables live in the provider packages. `iyon-native` may expose only generic credential-store operations; it must not know OpenRouter, OpenAI, Codex, OAuth claims, endpoints, or account semantics.
- Native long-lived operations retain explicit cancellation/ownership. A dropped TypeScript promise is not treated as cancellation unless the native/runtime contract explicitly bridges an `AbortSignal`.
- Rust concrete providers remain available as deprecated compatibility types in `iyon-api` for downstream Rust consumers. T7 does not delete them or remove networking dependencies from the Rust API crate. The TypeScript product path must not construct them.
- Deleting all three bundled TS registration modules must leave the runtime with no provider and an explicit startup error. There is no hidden native or default provider registration.

## Out of scope

- Porting built-in tools or their renderers (T8), the default agent/subagent loop (T9), or the default app, TUI, CLI command surface, and final Bun standalone executable (T10).
- Rewriting `iyon-core`, `iyon-tui`, or the native bridge except for the generic credential primitive required by this tranche.
- Removing `crates/iyon-api/src/providers/**`, changing the public Rust protocol, deleting `reqwest`, or removing `crates/iyon/src/main.rs`. Until T10, the remaining Rust binary is a compatibility path; the new runtime/bootstrap path is the product path and must use TS providers.
- Building a provider-independent HTTP abstraction, provider allowlists, provider-only privileged APIs, a second bundled-extension loader, or a bespoke keychain abstraction that embeds provider names in Rust.
- Adding provider UI, `/login` commands, model-picker rendering, agent policy, tool execution, or app behavior. Auth hooks may expose provider operations to the already-existing runtime auth surface, but T7 does not own that surface.
- Speculative Codex model-network discovery. The Codex contribution exports its known default model/capabilities; OpenRouter may query its documented catalog endpoint because the current protocol already anticipates model-specific reasoning data.

## Adjacent tranche contracts

### Consumes from earlier tranches

- The `iyon:api` TypeScript protocol and error/value types from T4.
- The T6 contribution registration, transactional activation, duplicate-ID handling, package loader, and runtime context/auth-hook contracts.
- The T1/T5 native runtime and TUI/app bootstrap entrypoint, including the explicit model-selection handoff into the native core.
- Generic logging, cancellation, and test-runtime facilities already provided by `iyon-runtime`; provider packages do not add hidden runtime globals.

### Exports for later tranches

- Three ordinary workspace extension packages at `plugins/providers/mock`, `plugins/providers/openrouter`, and `plugins/providers/openai-codex`, each with a default registration module and a directly usable `ModelApi` implementation.
- A stable provider registry/startup contract in which model selection resolves a registered definition, calls `create(config)`, and passes the returned model plus selection to the existing runtime/core boundary.
- Provider capability/model metadata and auth hooks consumable by T10's app/CLI and by third-party extensions through the same public API.
- Captured/synthetic wire fixtures and parity assertions that T8–T10 can reuse without live provider credentials.
- A generic credential-store N-API/runtime primitive, if the T1 bridge does not already provide one, with no provider-specific behavior.

### Must not steal from neighbors

- T7 must not register tools, agents, or the app, modify tool selection/execution, or add provider-specific branches to the native core (T8/T9/T10 boundaries).
- T7 must not take ownership of CLI parsing, terminal rendering, login command presentation, or final executable packaging. Auth hooks are provider operations only; the later CLI decides how to invoke them.
- T7 must not add a native fallback “for compatibility” to the TS provider resolver. The compatibility Rust binary may remain until T10, but the TS runtime has no implicit native provider.

## Commits

### Commit 1 — Define the provider contribution seam and parity fixtures

**Why:** The provider packages need one typed contract for construction, capabilities, model metadata, and auth without duplicating the T6 registry or coupling providers to the app. The existing Rust implementations also need an explicit event-sequence oracle before networking code is moved.

**Files:**

- `packages/iyon-sdk/src/providers.ts`
- `packages/iyon-sdk/src/index.ts`
- `packages/iyon-runtime/src/providers/types.ts`
- `packages/iyon-runtime/test/providers/fixtures.ts`
- `plugins/providers/README.md`

**Work:**

- Reuse the T4 `ModelApi`, `ModelRequest`, `ModelStreamEvent`, `ModelError`, `ReasoningLevel`, `Usage`, and `StopReason` types; add only the missing provider contribution types: provider ID/config, `create(config)`, model-specific `capabilities(model)`, optional model catalog/discovery, and `auth.login/logout/status` contexts.
- Keep configs provider-owned. The generic contract must not require OpenRouter or Codex fields, and the registry must not inspect provider config. Auth context should expose the existing runtime prompt/browser/logger/cancellation facilities and the generic credential store, not a provider switch.
- Add fixture helpers that consume an `AsyncIterable<ModelStreamEvent>` and compare the complete ordered sequence, including exact `content_index`, tool argument fragments, usage, and final stop reason. Include redaction helpers so fixture failures cannot print credentials.
- Record the Rust oracle cases as named fixture metadata: Mock text stream; OpenRouter text/reasoning/tool/usage/error cases; Codex message/reasoning/function-call/completion/error cases. The actual wire payloads are added alongside each provider in later commits.
- Document that provider registration is a normal T6 contribution and that removing a registration is expected to make startup fail, not to select a native fallback.

**Tests / verification:**

- `bun test packages/iyon-runtime/test/providers/fixtures.ts` (or the T1/T6 equivalent Bun test command) proves ordered comparison distinguishes missing, reordered, and extra events.
- Type-check `packages/iyon-sdk` and `packages/iyon-runtime` with the repository's configured Bun/TypeScript command.
- Assert the contract has no `reqwest`, native provider type, provider-name enum, or built-in-only registration import.

**Must not:** Modify Rust provider behavior, add a second registry, add provider implementations, or make the fixture helper an HTTP client.

### Commit 2 — Expose only a generic native credential store

**Why:** OpenRouter key storage and Codex credential persistence currently use Rust keyring code. The provider-specific policy must move to TypeScript while retaining an optional native secure-store primitive.

**Files:**

- `crates/iyon-native/src/credentials.rs`
- `crates/iyon-native/src/lib.rs`
- `packages/iyon-runtime/src/credentials.ts`
- `packages/iyon-runtime/test/credentials.test.ts`
- `crates/iyon-native/tests/credentials.rs`

**Work:**

- Add generic `get(service, account)`, `set(service, account, secret)`, `delete(service, account)`, and presence/status operations to the existing N-API addon. Cross the boundary with owned strings and typed errors; never hold N-API values across an async suspension.
- Keep service/account strings opaque to Rust. The TypeScript façade owns names such as `iyon`/`openrouter` and `iyon`/`openai-codex`, and decides when to fall back to environment variables or a user credential file.
- Ensure delete is idempotent for missing entries, set does not log the secret, and a failed native operation is returned rather than swallowed. Connect `AbortSignal` only if the existing N-API operation is long-lived; keychain calls themselves must not spawn an unowned Rust task.
- Test the runtime façade against an in-memory fake and the native addon boundary against a disposable service/account. Do not require a developer keychain in CI.

**Tests / verification:**

- `cargo test -p iyon-native credentials` and the native addon smoke command from T1.
- `bun test packages/iyon-runtime/test/credentials.test.ts` asserts round-trip, overwrite, delete, missing status, typed failure, and secret redaction.
- `cargo check --workspace` keeps all existing Rust crates buildable.

**Must not:** Mention OpenRouter/OpenAI/Codex in `iyon-native`, move OAuth or API-key policy into Rust, or replace the generic store with an Iyon HTTP/auth service.

### Commit 3 — Port the Mock provider and registration

**Why:** Mock is deterministic and establishes the complete bundled-provider package shape and registration path before real network protocols are introduced.

**Files:**

- `plugins/providers/mock/package.json`
- `plugins/providers/mock/tsconfig.json`
- `plugins/providers/mock/src/index.ts`
- `plugins/providers/mock/src/provider.ts`
- `plugins/providers/mock/test/provider.test.ts`

**Work:**

- Implement `MockProvider implements ModelApi` with the current Rust behavior: find the last user text, emit the initial delay/started event, text stream chunks, `TextEnd`, and `Done(Stop)` with the same timing-independent event order. Keep delay injectable or disabled in tests.
- Export the provider contribution with ID `mock`, model `mock`, no reasoning candidates, and no provider auth hook. `create(config)` returns a new provider and does not use a global singleton.
- Use `iyon:api` types and `iyon:plugins` registration only. The package must be loadable through the same T6 activation path as an external package.
- Test empty input, multiple user messages, deterministic chunking, delayed streaming, cancellation behavior supported by the existing `ModelApi` contract, and exact fixture equality with the Rust mock sequence.

**Tests / verification:**

- `bun test plugins/providers/mock/test/provider.test.ts`.
- `bunx tsc --noEmit -p plugins/providers/mock/tsconfig.json` (or the workspace type-check command).
- Activate only the mock package in a minimal T6 runtime and assert selection resolves `provider: "mock", model_id: "mock"`.

**Must not:** Add tools/agents/app registrations, call native Rust for mock behavior, or make Mock a hidden fallback in the registry.

### Commit 4 — Port OpenRouter request serialization and streaming

**Why:** OpenRouter is the first real provider and contains the most important protocol risks: OpenAI-compatible message/tool serialization, fragmented SSE, retries, usage, reasoning, and late tool-call completion.

**Files:**

- `plugins/providers/openrouter/package.json`
- `plugins/providers/openrouter/tsconfig.json`
- `plugins/providers/openrouter/src/provider.ts`
- `plugins/providers/openrouter/src/serialize.ts`
- `plugins/providers/openrouter/src/sse.ts`
- `plugins/providers/openrouter/src/normalize.ts`
- `plugins/providers/openrouter/test/fixtures/*.sse`
- `plugins/providers/openrouter/test/provider.test.ts`
- `plugins/providers/openrouter/test/serialize.test.ts`

**Work:**

- Implement `OpenRouterProvider` with injectable `fetch`/sleep only for tests; production uses global Bun `fetch`. Construct `${baseUrl}/chat/completions` with the existing defaults and headers, including bearer auth, SSE accept, content type, and optional `OPENROUTER_TITLE` behavior.
- Port `build_request_body`, `convert_user_content`, `convert_assistant`, and `convert_tool` semantics exactly: system message omission when blank, data-URL images, assistant tool-call JSON strings, tool-result role/id, `reasoning.effort`, `temperature`, `max_tokens`, and `stream: true`/`tool_choice: auto`.
- Implement a byte-safe SSE decoder using `TextDecoder` with streaming mode and a retained buffer. Parse blank-line-delimited frames, comments, multiple `data:` lines, `[DONE]`, JSON errors, and a final unterminated frame if the response closes cleanly. Do not assume `ReadableStream` chunks align with frames or JSON values.
- Port the Rust event mapping: `Started`; text and reasoning deltas; indexed tool-call buffers; `ToolCallStart` only once the ID/name are known; every argument delta; usage with cached prompt tokens subtracted from input; provider finish-reason mapping; flush valid/invalid tool JSON as the Rust implementation does; infer `ToolUse` when a tool call exists but finish reason is absent; then `Done`.
- Port the two-retry policy and 300ms exponential base delay. Normalize status 400/401/403/429/5xx and transport errors to the existing `ModelErrorKind` values without exposing response secrets.
- Make test responses synthetic `Response` objects whose body is chunked at every boundary around SSE delimiters, UTF-8 code points, JSON punctuation, tool arguments, and `[DONE]`. Capture request URL, headers, and parsed JSON to assert serialization without a live endpoint.

**Tests / verification:**

- `bun test plugins/providers/openrouter/test` with named cases `serializes_full_model_request`, `parses_fragmented_text_reasoning_and_usage`, `flushes_fragmented_tool_calls`, `handles_multiline_sse_and_comments`, `normalizes_http_errors`, `retries_transient_statuses`, and `does_not_retry_auth_or_parse_errors`.
- Compare every produced event array with the OpenRouter Rust oracle fixture, including tool-call indexes and cached-token accounting.
- `cargo check --workspace` and the workspace TypeScript check remain green; no Rust provider file is changed in this commit.

**Must not:** Create a shared Iyon HTTP client, add live credentials to tests, silently discard malformed provider frames, or move model discovery/auth into `iyon-native`.

### Commit 5 — Add OpenRouter capabilities, model discovery, and auth hooks

**Why:** OpenRouter-specific defaults, reasoning support, model catalog behavior, and API-key lookup belong in the provider contribution rather than in the runtime or native bridge.

**Files:**

- `plugins/providers/openrouter/src/index.ts`
- `plugins/providers/openrouter/src/models.ts`
- `plugins/providers/openrouter/src/auth.ts`
- `plugins/providers/openrouter/src/provider.ts`
- `plugins/providers/openrouter/test/models.test.ts`
- `plugins/providers/openrouter/test/auth.test.ts`

**Work:**

- Register ID `openrouter`, expose `deepseek/deepseek-v4-flash:latest` as the default model, and implement `capabilities(model)` from the provider's reasoning levels. When the catalog supplies `reasoning.supported_efforts`, normalize only those levels; before discovery, preserve the current full OpenAI-style candidate set.
- Add optional model discovery using normal `fetch` to `${baseUrl}/models`, parse the catalog into the generic model metadata, preserve model IDs and names, and convert malformed/failed catalogs into typed provider errors. Discovery must not be required for a normal configured chat stream.
- Port current auth lookup order: non-empty `OPENROUTER_API_KEY` first, then the generic credential store. `login` and `logout` use the existing T6 auth context and generic secret prompt/store; `status` reports presence without returning or logging the key. Empty environment values do not shadow a stored credential.
- Keep provider config injectable for tests (base URL, key, model, fetch), while runtime config continues to honor `IYON_MODEL` and the documented environment semantics.
- Test capability narrowing, default model resolution, catalog parsing/failure, environment-over-store precedence, login/logout/status with a fake store, and no-secret logging.

**Tests / verification:**

- `bun test plugins/providers/openrouter/test`.
- Assert deleting the OpenRouter registration removes it from provider definitions; no native provider appears in the registry.
- Type-check the package and run `cargo check --workspace`.

**Must not:** Add OpenRouter branches to runtime/native code, require catalog network access during startup, or implement an interactive CLI command owned by T10.

### Commit 6 — Port OpenAI Codex request serialization and Responses streaming

**Why:** Codex uses a different provider protocol and must be independently reviewable. Its Responses event vocabulary, reasoning summaries, tool argument reconciliation, session headers, and completion usage cannot be hidden behind OpenRouter assumptions.

**Files:**

- `plugins/providers/openai-codex/package.json`
- `plugins/providers/openai-codex/tsconfig.json`
- `plugins/providers/openai-codex/src/provider.ts`
- `plugins/providers/openai-codex/src/serialize.ts`
- `plugins/providers/openai-codex/src/sse.ts`
- `plugins/providers/openai-codex/src/normalize.ts`
- `plugins/providers/openai-codex/test/fixtures/*.sse`
- `plugins/providers/openai-codex/test/provider.test.ts`
- `plugins/providers/openai-codex/test/serialize.test.ts`

**Work:**

- Implement the Responses endpoint construction and existing headers: bearer access token, `chatgpt-account-id`, `originator`, `session_id`, `x-client-request-id`, and `OpenAI-Beta`. Use request metadata session ID or generate a local ID with the same fallback purpose as Rust.
- Port `build_request_body`, message/content conversion, function tools, `store: false`, `parallel_tool_calls`, low text verbosity, encrypted reasoning include, prompt-cache key, reasoning effort, temperature, and max-output-token behavior. Preserve the current default model.
- Implement the same retained-buffer SSE framing rules as OpenRouter, but map Codex event kinds precisely: output-item creation, text/refusal deltas, reasoning summary deltas and separators, function-call argument deltas/done reconciliation, output-item completion, response usage/status completion, failed/error events, and final inferred `ToolUse`.
- Port three retries with 400ms exponential base delay and the existing HTTP error message normalization. Keep response-body parsing bounded and do not include tokens or full untrusted bodies in logs.
- Use synthetic fragmented fixtures for message-only, reasoning, multiple function calls, incomplete/failed/cancelled responses, usage/cache tokens, and malformed argument JSON. Assert exact sequences against the Rust Codex oracle.

**Tests / verification:**

- `bun test plugins/providers/openai-codex/test` with `serializes_codex_request`, `parses_fragmented_responses_stream`, `reconciles_argument_done_without_duplicate_delta`, `emits_reasoning_boundaries`, `maps_completion_status`, and retry/error cases.
- Type-check the package and run `cargo check --workspace`.

**Must not:** Implement OAuth in the stream class, call `reqwest`/Rust, reuse OpenRouter wire parsing beyond a small framing helper if T6 provides one, or add agent/tool behavior.

### Commit 7 — Add Codex capabilities and provider-owned OAuth persistence

**Why:** Codex authentication currently lives in `crates/iyon/src/auth.rs`; moving its orchestration into the provider is required for the TS path to be complete while keeping native storage generic.

**Files:**

- `plugins/providers/openai-codex/src/index.ts`
- `plugins/providers/openai-codex/src/models.ts`
- `plugins/providers/openai-codex/src/auth.ts`
- `plugins/providers/openai-codex/src/credentials.ts`
- `plugins/providers/openai-codex/test/auth.test.ts`
- `plugins/providers/openai-codex/test/models.test.ts`

**Work:**

- Register ID `openai-codex`, the `gpt-5.3-codex` default model, and its reasoning capability table. Keep model discovery static and explicit until a supported Codex catalog exists; do not invent an endpoint.
- Port the provider-specific OAuth constants, PKCE verifier/challenge, random state, authorization URL, local callback route/port, state validation, token exchange, refresh-token handling, JWT `chatgpt_account_id` extraction, expiry calculation, and refresh skew. Use Bun browser/process/server APIs through the existing auth context; no Rust listener or Rust HTTP request is introduced.
- Port keychain-first/fallback-file semantics into TypeScript: use the generic credential store under provider-chosen opaque keys, read/write the documented local credential file as a provider concern, enforce restrictive file permissions, verify persisted JSON, and make logout remove all legacy account aliases plus the file. Keep compatibility with the current account aliases and credential shape so existing users are not silently logged out.
- Implement `login`, `logout`, and `status` hooks with typed failures and redacted output. `status` reports presence/provider/account/expiry metadata as the current command does, but never the access or refresh token.
- Test PKCE deterministically with injected randomness, callback path/state/code validation, refresh-before-expiry and refresh-token preservation, keychain/file precedence, legacy aliases, delete behavior, malformed credentials, and permission/error propagation. Use fake browser, callback server, store, clock, and fetch contexts.

**Tests / verification:**

- `bun test plugins/providers/openai-codex/test`.
- Assert no Codex string, OAuth URL, JWT claim, or callback port exists under `crates/iyon-native` or `packages/iyon-runtime`.
- Type-check the package and run `cargo check --workspace`.

**Must not:** Put OAuth knowledge into native Rust, print credentials, make T10's CLI decisions, or make authentication a prerequisite for loading all provider registrations.

### Commit 8 — Register all bundled providers through the runtime bootstrap

**Why:** The individual packages are not a product migration until the T1/T6 startup path loads them through the normal extension mechanism and resolves selection without any native default.

**Files:**

- `packages/iyon-runtime/src/bootstrap/providers.ts`
- `packages/iyon-runtime/src/bootstrap/index.ts`
- `packages/iyon-runtime/src/providers/selection.ts`
- `packages/iyon-runtime/test/providers/startup.test.ts`
- `packages/iyon-runtime/test/providers/selection.test.ts`
- `package.json`
- `bun.lock`

**Work:**

- Add the three bundled provider workspace packages to the existing workspace manifest using the repository's established workspace syntax, with no new third-party dependency. Register them by importing their ordinary extension entrypoints through the T6 loader; do not call constructors directly from bootstrap.
- Implement generic selection over registered definitions. Preserve `IYON_PROVIDER` aliases and unknown-value behavior, check OpenRouter auth status before Codex status when no explicit provider is set, and return Mock only when its registration is present. Explicit provider absence or unavailable credentials produces the same visible warning/fallback behavior as `iyon.md`.
- Resolve the selected definition's model/default config, call async `create`, obtain capabilities, and pass the resulting `ModelApi` plus `ModelSelection` through the existing TS runtime/native-core handoff. Keep provider selection out of `iyon-native` and out of `iyon-core`.
- Add a startup test that activates all registrations and covers each fallback branch, then removes each provider registration (and all registrations) and asserts the resolver fails with “no provider registered” rather than silently constructing a Rust model. Add an import/graph guard that the runtime bootstrap contains no `MockModelApi`, `OpenRouterModelApi`, `OpenAICodexModelApi`, or Rust provider import.
- Make the runtime startup path the product path used by the post-T6 Bun runtime. If the repository still builds `crates/iyon` during this tranche, leave that binary as a documented compatibility binary until T10; do not route the new runtime through its Rust `main`.

**Tests / verification:**

- `bun test packages/iyon-runtime/test/providers` with controlled environment and fake credential statuses.
- Run the T1 standalone Bun smoke executable using the runtime bootstrap and assert its initial config event names the selected TS provider/model.
- `cargo check --workspace` plus the existing Rust tests; workspace type-check and package build must pass.

**Must not:** Modify app/CLI UX, port tools or agent startup, add a native provider fallback, or create a separate bundled extension loader.

### Commit 9 — Deprecate Rust concrete-provider use and close the parity gate

**Why:** Once TS parity and runtime cutover pass, the compatibility boundary should be explicit and mechanically checked so future work cannot accidentally reintroduce a Rust concrete provider into the product path.

**Files:**

- `crates/iyon-api/src/lib.rs`
- `crates/iyon-api/src/providers/mock.rs`
- `crates/iyon-api/src/providers/openrouter.rs`
- `crates/iyon-api/src/providers/openai_codex.rs`
- `crates/iyon-api/tests/provider_compatibility.rs`
- `packages/iyon-runtime/test/providers/cutover.test.ts`
- `.github/workflows/api-surface.yml`

**Work:**

- Mark the concrete Rust provider exports and constructors deprecated with migration guidance to the matching TS packages, while retaining their signatures and implementation for downstream Rust compatibility. Do not remove `ModelApi`, normalized protocol types, or provider-related dependencies in this tranche.
- Add compatibility tests proving the deprecated Rust types still compile and retain the existing Rust oracle behavior, while the TS cutover test proves the runtime's provider instances originate from registered TS contributions.
- Add CI assertions for the exit criteria: all three bundled provider registrations load through the common loader; deleting them leaves no provider; runtime source contains no native concrete-provider construction; fragmented fixture suites pass; Rust workspace remains green. Keep the check focused on provider cutover rather than requiring live network credentials.
- Update only the provider-related CI command/configuration needed to run Bun provider tests and the existing Rust checks. Do not turn the workflow into T10 packaging or standalone-executable work.

**Tests / verification:**

- `test -f tranche_7_bun_refactor.md`.
- `cargo test --workspace` (or the repository's existing Rust CI command), including the compatibility test.
- `bun test plugins/providers packages/iyon-runtime/test/providers` and the configured workspace type-check/build.
- Assert the exit criteria directly: no provider registration means startup failure; all normal startup selections are TS provider instances; no secret appears in test output; every provider fixture passes exact event-sequence comparison.

**Must not:** Delete Rust providers, change public protocol shapes, remove the Rust binary, package the final executable, or claim T10's app/CLI cutover is complete.
