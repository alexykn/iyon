# iyon public API

This inventory describes the library crate in `crates/iyon/src/lib.rs` and the
binary startup path in `crates/iyon/src/main.rs`. The library root exposes only
`iyon::tui`; the `backend`, `components`, `controller`, `state`, `theme`,
`tools`, and `transcript` modules beneath it are implementation modules, not
public module paths. Items documented below use their externally reachable
paths, including public re-exports.

## Library API

### Module

| Path | Kind | Signature | Purpose |
| --- | --- | --- | --- |
| `iyon::tui` | public module | `pub mod tui` | Provides the terminal UI application entry points, UI state type, actions, and backend/frontend event types. |

### Public functions

| Path | Kind | Signature | Purpose |
| --- | --- | --- | --- |
| `iyon::tui::build_app` | function | `pub fn build_app(command_tx: iyon_core::CoreCommandSender, selection: iyon_core::ModelSelection) -> iyon_tui::App<...>` | Builds the configured TUI application, wiring initialization, action handling, view rendering, history, and theme. The concrete `App` return type contains the initialization, update, and view closures shown by the source signature. |
| `iyon::tui::run_with_core` | async function | `pub async fn run_with_core(core: iyon_core::IyonCore, selection: iyon_core::ModelSelection) -> anyhow::Result<()>` | Splits a running core into command/event channels, runs the TUI with a core-event bridge, and shuts the bridge down when the app exits. |

There are no public traits or public inherent methods in `iyon`. In
particular, `IyonState` and `ApprovalDecision` have no public fields or public
constructors; their fields and implementation methods are `pub(crate)` or
private. The derived standard traits on public types are not additional
crate-declared methods.

### Public structs

#### `iyon::tui::ToolDraftKey`

`pub struct ToolDraftKey` identifies an in-progress tool-call content item in a
model message. It derives `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, and
`Hash`.

Public fields:

| Path | Kind | Signature | Purpose |
| --- | --- | --- | --- |
| `iyon::tui::ToolDraftKey::message_id` | field | `pub message_id: u64` | Identifies the model message containing the draft. |
| `iyon::tui::ToolDraftKey::content_index` | field | `pub content_index: usize` | Identifies the tool-call content position within that message. |

#### `iyon::tui::ApprovalDecision`

`pub struct ApprovalDecision` carries the result of a user decision about a
tool-call approval. It derives `Debug` and `Clone`. It has no public fields;
`approval_id`, `tool_call_id`, and `approved` are crate-visible only.

#### `iyon::tui::IyonState`

`pub struct IyonState` is the root state owned by the TUI application. Its
backend, input, steering, conversation, approval, and display-state fields are
crate-visible only, and it has no public constructor or public methods.

### Public enums and variants

#### `iyon::tui::FrontendEvent`

`pub enum FrontendEvent` is the UI-facing event stream produced from core
events. It derives `Debug`.

| Path | Kind | Variant signature | Purpose |
| --- | --- | --- | --- |
| `iyon::tui::FrontendEvent::TurnStarted` | variant | `TurnStarted` | Signals that a model turn has started. |
| `iyon::tui::FrontendEvent::SteerQueued` | variant | `SteerQueued { text: String }` | Reports a user steering message queued for the active turn. |
| `iyon::tui::FrontendEvent::UserMessage` | variant | `UserMessage { text: String }` | Delivers user-message text to the UI transcript. |
| `iyon::tui::FrontendEvent::AssistantDelta` | variant | `AssistantDelta { text: String }` | Delivers an assistant text-stream delta. |
| `iyon::tui::FrontendEvent::ThinkingDelta` | variant | `ThinkingDelta { text: String }` | Delivers an assistant reasoning/thinking-stream delta. |
| `iyon::tui::FrontendEvent::ToolCallPreparing` | variant | `ToolCallPreparing { key: ToolDraftKey, tool_call_id: Option<String>, tool_name: Option<String> }` | Starts rendering a tool-call draft while its identity may still be incomplete. |
| `iyon::tui::FrontendEvent::ToolCallArguments` | variant | `ToolCallArguments { key: ToolDraftKey, tool_call_id: Option<String>, tool_name: Option<String>, delta: String }` | Appends an argument delta to a tool-call draft. |
| `iyon::tui::FrontendEvent::ToolCallPrepared` | variant | `ToolCallPrepared { key: ToolDraftKey, tool_call_id: String, tool_name: String, arguments: serde_json::Value }` | Reports a completed tool-call draft with parsed arguments. |
| `iyon::tui::FrontendEvent::TurnFinished` | variant | `TurnFinished` | Signals successful completion of a model turn. |
| `iyon::tui::FrontendEvent::TurnFailed` | variant | `TurnFailed { message: String }` | Reports a failed model turn and its message. |
| `iyon::tui::FrontendEvent::TurnCancelled` | variant | `TurnCancelled` | Signals cancellation of the active turn. |
| `iyon::tui::FrontendEvent::ToolCallStarted` | variant | `ToolCallStarted { tool_call_id: String, tool_name: String, arguments: serde_json::Value }` | Signals that a tool call has begun executing. |
| `iyon::tui::FrontendEvent::ToolCallUpdated` | variant | `ToolCallUpdated { tool_call_id: String, update: ToolUpdatePresentation }` | Reports a progress, text, or detail update for a running tool call. |
| `iyon::tui::FrontendEvent::ToolCallFinished` | variant | `ToolCallFinished { tool_call_id: String, is_error: bool }` | Signals completion of tool-call execution and whether it failed. |
| `iyon::tui::FrontendEvent::ToolApprovalRequested` | variant | `ToolApprovalRequested { approval_id: u64, tool_call_id: String, tool_name: String, arguments: serde_json::Value }` | Requests UI approval before executing a tool call. |
| `iyon::tui::FrontendEvent::ToolApprovalResolved` | variant | `ToolApprovalResolved { approval_id: u64, tool_call_id: String, approved: bool, reason: Option<String> }` | Reports the user’s approval decision and optional reason. |
| `iyon::tui::FrontendEvent::ToolResult` | variant | `ToolResult { tool_call_id: String, tool_name: String, text: String, details: serde_json::Value, is_error: bool }` | Delivers the final rendered tool result and error status. |
| `iyon::tui::FrontendEvent::ConfigChanged` | variant | `ConfigChanged { provider: String, model_id: String, reasoning_effort: iyon_core::ReasoningLevel }` | Reports a provider, model, or reasoning-effort configuration change. |

#### `iyon::tui::ToolUpdatePresentation`

`pub enum ToolUpdatePresentation` represents an update to a running tool call.
It derives `Debug` and `Clone`.

| Path | Kind | Variant signature | Purpose |
| --- | --- | --- | --- |
| `iyon::tui::ToolUpdatePresentation::Text` | variant | `Text(String)` | Supplies a textual tool progress update. |
| `iyon::tui::ToolUpdatePresentation::Progress` | variant | `Progress { label: String, current: Option<u64>, total: Option<u64> }` | Supplies labeled progress counts, optionally with a total. |
| `iyon::tui::ToolUpdatePresentation::Details` | variant | `Details(serde_json::Value)` | Supplies structured tool-update details. |

#### `iyon::tui::IyonAction`

`pub enum IyonAction` is the action type consumed by the TUI update loop. It
derives `Debug`.

| Path | Kind | Variant signature | Purpose |
| --- | --- | --- | --- |
| `iyon::tui::IyonAction::SubmitTurn` | variant | `SubmitTurn(String)` | Submits the composer text as a model turn. |
| `iyon::tui::IyonAction::ToolApproval` | variant | `ToolApproval(ApprovalDecision)` | Carries a user’s tool-approval decision. |
| `iyon::tui::IyonAction::CtrlC` | variant | `CtrlC` | Handles Ctrl-C according to current composer/turn state. |
| `iyon::tui::IyonAction::Escape` | variant | `Escape` | Handles Escape, including cancellation of an active turn. |
| `iyon::tui::IyonAction::RequestExit` | variant | `RequestExit` | Requests a clean application exit. |
| `iyon::tui::IyonAction::InterruptActiveTurn` | variant | `InterruptActiveTurn` | Requests interruption of the active model turn. |
| `iyon::tui::IyonAction::CycleReasoningEffort` | variant | `CycleReasoningEffort` | Cycles the selected reasoning-effort level. |
| `iyon::tui::IyonAction::ComposerPaste` | variant | `ComposerPaste(String)` | Delivers pasted text to the composer. |
| `iyon::tui::IyonAction::Backend` | variant | `Backend(FrontendEvent)` | Delivers a backend event to the UI update loop. |

## Binary entry and startup

`crates/iyon/src/main.rs` is the `iyon` binary entry point, not part of the
library API. `main` runs the private async `run` function and prints an error
then exits with status 1 if startup or runtime returns an error.

The CLI is defined with Clap:

- No subcommand defaults to `run`, which starts the interactive TUI.
- `iyon run` also starts the interactive TUI.
- `iyon auth login`, `iyon auth logout`, and `iyon auth status` invoke the
  corresponding private authentication helpers.

### Provider selection and construction

`detect_provider` and `ProviderKind` are private binary items, not public crate
API. Selection works as follows:

1. If `IYON_PROVIDER` is set, its trimmed, case-insensitive value selects
   `openrouter`, `codex`/`openai`/`openai-codex`, or `mock`. An unrecognized value
   is treated as OpenRouter.
2. Without `IYON_PROVIDER`, the binary prefers OpenRouter when an API key is
   available, then OpenAI Codex when stored credentials are available, and
   otherwise selects Mock. The OpenRouter lookup checks the
   `OPENROUTER_API_KEY` environment variable first and then the OS keyring.

The selected provider is constructed into the common model abstraction
`Arc<dyn iyon_api::ModelApi>`:

- **OpenRouter:** uses `IYON_MODEL`, or the compiled default
  `deepseek/deepseek-v4-flash:latest`, to construct
  `iyon_api::OpenRouterModelApi`. If the provider was selected but no key is
  available, startup falls back to `iyon_api::MockModelApi`.
- **Codex:** refreshes/loads stored credentials and constructs
  `iyon_api::OpenAICodexModelApi` with the access token and account ID. Its
  `ModelSelection` is provider `openai-codex` and model `gpt-5.3-codex`. Missing
  or unavailable credentials fall back to Mock.
- **Mock:** constructs `iyon_api::MockModelApi` directly, with a `ModelSelection`
  of provider `mock` and model `mock`.

The resulting model and `ModelSelection` are passed to
`IyonCore::spawn_on_current_runtime_with_selection_and_hooks` with the default
`ToolHookSet`, then to the public library function
`iyon::tui::run_with_core`.
