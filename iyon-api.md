# iyon-api public API

This inventory follows the public surface of `crates/iyon-api/src/lib.rs` and only the items reachable from that crate root. The crate root re-exports public definitions from private implementation modules. There are no public free functions, public statics, or public modules at the crate root.

The source modules `client`, `error`, `model`, `providers`, and `stream` are declared with private `mod` items in `crates/iyon-api/src/lib.rs`. Although `providers` declares `pub mod mock`, `pub mod openai_codex`, and `pub mod openrouter`, their private parent makes those module paths inaccessible outside the crate; only the three provider types re-exported below are public API. Source-declared derives such as `Debug`, `Clone`, `Copy`, `Default`, `PartialEq`, and `Eq` are not expanded into separate generated methods here.

## Crate-root re-exports and client module

Definitions are in `crates/iyon-api/src/client.rs`; all three items are re-exported by `crates/iyon-api/src/lib.rs`.

| Path | Kind | Signature | Purpose |
|---|---|---|---|
| `iyon_api::ModelStream` | type alias — re-export | `Pin<Box<dyn Stream<Item = Result<ModelStreamEvent, ModelError>> + Send>>` | Asynchronously yielded model events and model errors. |
| `iyon_api::ModelStreamFuture<'a>` | type alias — re-export | `Pin<Box<dyn Future<Output = Result<ModelStream, ModelError>> + Send + 'a>>` | Future that resolves to a model event stream or a `ModelError`. |
| `iyon_api::ModelApi` | trait — re-export | `pub trait ModelApi: Send + Sync` | Provider surface used by core to start a model response stream. |
| `iyon_api::ModelApi::stream` | required trait method | `fn stream(&self, request: ModelRequest) -> ModelStreamFuture<'_>` | Starts streaming a response for a request. Implemented by `MockModelApi`, `OpenAICodexModelApi`, and `OpenRouterModelApi`. |

## Error module

Definitions are in `crates/iyon-api/src/error.rs`; the types are re-exported by `crates/iyon-api/src/lib.rs`.

### `ModelError`

| Path | Kind | Signature | Purpose |
|---|---|---|---|
| `iyon_api::ModelError` | struct — re-export | `pub struct ModelError` | Represents a model-provider failure with a category and message. |
| `iyon_api::ModelError::kind` | public field | `ModelErrorKind` | Classifies the failure. |
| `iyon_api::ModelError::message` | public field | `String` | Human-readable failure message. |
| `iyon_api::ModelError::new` | inherent method | `pub fn new(kind: ModelErrorKind, message: impl Into<String>) -> Self` | Constructs an error with the supplied kind and message. |
| `iyon_api::ModelError::unknown` | inherent method | `pub fn unknown(message: impl Into<String>) -> Self` | Constructs an error with `ModelErrorKind::Unknown`. |

### `ModelErrorKind`

| Path | Kind | Signature | Purpose |
|---|---|---|---|
| `iyon_api::ModelErrorKind` | enum — re-export | `pub enum ModelErrorKind` | Categories used to normalize provider failures. |
| `iyon_api::ModelErrorKind::InvalidRequest` | unit variant | — | The request is invalid. |
| `iyon_api::ModelErrorKind::Authentication` | unit variant | — | Authentication or authorization failed. |
| `iyon_api::ModelErrorKind::RateLimited` | unit variant | — | The provider rate-limited the request. |
| `iyon_api::ModelErrorKind::Provider` | unit variant | — | The provider returned an application-level error. |
| `iyon_api::ModelErrorKind::Transport` | unit variant | — | Network or transport processing failed. |
| `iyon_api::ModelErrorKind::Cancelled` | unit variant | — | The operation was cancelled. |
| `iyon_api::ModelErrorKind::Unknown` | unit variant | — | The failure has no more specific category. |

## Model module

Definitions are in `crates/iyon-api/src/model.rs`; the types are re-exported by `crates/iyon-api/src/lib.rs`.

### `ModelRequest`

| Path | Kind | Signature | Purpose |
|---|---|---|---|
| `iyon_api::ModelRequest` | struct — re-export | `pub struct ModelRequest` | Complete input passed to a model provider. |
| `iyon_api::ModelRequest::system_prompt` | public field | `Option<String>` | Optional system instruction. |
| `iyon_api::ModelRequest::messages` | public field | `Vec<ModelMessage>` | Conversation messages to send. |
| `iyon_api::ModelRequest::tools` | public field | `Vec<ModelToolSpec>` | Tools offered to the model. |
| `iyon_api::ModelRequest::params` | public field | `ModelParams` | Sampling, reasoning, and cache parameters. |
| `iyon_api::ModelRequest::metadata` | public field | `ModelMetadata` | Session and user metadata. |

### `ModelParams`

| Path | Kind | Signature | Purpose |
|---|---|---|---|
| `iyon_api::ModelParams` | struct — re-export | `pub struct ModelParams` | Optional generation controls. |
| `iyon_api::ModelParams::temperature` | public field | `Option<f32>` | Optional sampling temperature. |
| `iyon_api::ModelParams::max_tokens` | public field | `Option<u32>` | Optional output-token limit. |
| `iyon_api::ModelParams::reasoning` | public field | `Option<ReasoningLevel>` | Optional reasoning effort. |
| `iyon_api::ModelParams::cache_retention` | public field | `Option<CacheRetention>` | Optional prompt-cache retention preference. |

### `ModelMetadata`

| Path | Kind | Signature | Purpose |
|---|---|---|---|
| `iyon_api::ModelMetadata` | struct — re-export | `pub struct ModelMetadata` | Optional request metadata. |
| `iyon_api::ModelMetadata::session_id` | public field | `Option<String>` | Optional session identifier. |
| `iyon_api::ModelMetadata::user_id` | public field | `Option<String>` | Optional user identifier. |

### `ReasoningLevel`

| Path | Kind | Signature | Purpose |
|---|---|---|---|
| `iyon_api::ReasoningLevel` | enum — re-export | `pub enum ReasoningLevel` | Supported reasoning-effort levels. |
| `iyon_api::ReasoningLevel::None` | unit variant | — | Disable reasoning. |
| `iyon_api::ReasoningLevel::Minimal` | unit variant | — | Minimal reasoning effort. |
| `iyon_api::ReasoningLevel::Low` | unit variant | — | Low reasoning effort. |
| `iyon_api::ReasoningLevel::Medium` | unit variant | — | Medium reasoning effort; the default variant. |
| `iyon_api::ReasoningLevel::High` | unit variant | — | High reasoning effort. |
| `iyon_api::ReasoningLevel::XHigh` | unit variant | — | Extra-high reasoning effort. |
| `iyon_api::ReasoningLevel::Max` | unit variant | — | Maximum reasoning effort. |
| `iyon_api::ReasoningLevel::ALL` | associated constant | `pub const ALL: [ReasoningLevel; 7]` | All effort levels in ascending order. |
| `iyon_api::ReasoningLevel::code` | inherent method | `pub fn code(self) -> &'static str` | Returns the wire-level effort name. |
| `iyon_api::ReasoningLevel::candidates` | inherent method | `pub fn candidates(provider: &str) -> &'static [ReasoningLevel]` | Returns effort levels recognized for a provider name, or an empty slice for providers without reasoning support. |
| `iyon_api::ReasoningLevel::next_for` | inherent method | `pub fn next_for(current: ReasoningLevel, provider: &str) -> ReasoningLevel` | Advances to the next provider-supported effort, wrapping around; leaves the current value unchanged when there are no candidates. |

### `CacheRetention`

| Path | Kind | Signature | Purpose |
|---|---|---|---|
| `iyon_api::CacheRetention` | enum — re-export | `pub enum CacheRetention` | Prompt-cache retention preference. |
| `iyon_api::CacheRetention::None` | unit variant | — | Do not retain cached prompt content. |
| `iyon_api::CacheRetention::Short` | unit variant | — | Short cache retention. |
| `iyon_api::CacheRetention::Long` | unit variant | — | Long cache retention. |

### `ModelMessage`

| Path | Kind | Signature | Purpose |
|---|---|---|---|
| `iyon_api::ModelMessage` | enum — re-export | `pub enum ModelMessage` | A user, assistant, or tool-result conversation message. |
| `iyon_api::ModelMessage::User` | variant with public field | `{ content: Vec<ContentBlock> }` | User-authored content. |
| `iyon_api::ModelMessage::User::content` | public variant field | `Vec<ContentBlock>` | User content blocks. |
| `iyon_api::ModelMessage::Assistant` | variant with public field | `{ content: Vec<ContentBlock> }` | Assistant-authored content. |
| `iyon_api::ModelMessage::Assistant::content` | public variant field | `Vec<ContentBlock>` | Assistant content blocks. |
| `iyon_api::ModelMessage::ToolResult` | variant with public fields | `{ tool_call_id: String, tool_name: String, content: Vec<ContentBlock>, is_error: bool }` | Result returned from a tool invocation. |
| `iyon_api::ModelMessage::ToolResult::tool_call_id` | public variant field | `String` | Identifier of the related tool call. |
| `iyon_api::ModelMessage::ToolResult::tool_name` | public variant field | `String` | Name of the tool that produced the result. |
| `iyon_api::ModelMessage::ToolResult::content` | public variant field | `Vec<ContentBlock>` | Tool result content blocks. |
| `iyon_api::ModelMessage::ToolResult::is_error` | public variant field | `bool` | Whether the tool result represents an error. |

### `ContentBlock`

| Path | Kind | Signature | Purpose |
|---|---|---|---|
| `iyon_api::ContentBlock` | enum — re-export | `pub enum ContentBlock` | One content unit in a model message. |
| `iyon_api::ContentBlock::Text` | variant with public field | `{ text: String }` | Plain text content. |
| `iyon_api::ContentBlock::Text::text` | public variant field | `String` | Text value. |
| `iyon_api::ContentBlock::Image` | variant with public fields | `{ data: Vec<u8>, mime_type: String }` | Binary image content. |
| `iyon_api::ContentBlock::Image::data` | public variant field | `Vec<u8>` | Image bytes. |
| `iyon_api::ContentBlock::Image::mime_type` | public variant field | `String` | Image MIME type. |
| `iyon_api::ContentBlock::Thinking` | variant with public field | `{ text: String }` | Model reasoning/thinking content. |
| `iyon_api::ContentBlock::Thinking::text` | public variant field | `String` | Thinking text. |
| `iyon_api::ContentBlock::ToolCall` | variant with public fields | `{ id: String, name: String, arguments: Value }` | A model-requested tool call. |
| `iyon_api::ContentBlock::ToolCall::id` | public variant field | `String` | Tool-call identifier. |
| `iyon_api::ContentBlock::ToolCall::name` | public variant field | `String` | Requested tool name. |
| `iyon_api::ContentBlock::ToolCall::arguments` | public variant field | `serde_json::Value` | JSON tool arguments. |

### `ModelToolSpec`

| Path | Kind | Signature | Purpose |
|---|---|---|---|
| `iyon_api::ModelToolSpec` | struct — re-export | `pub struct ModelToolSpec` | Description of a tool exposed to a model. |
| `iyon_api::ModelToolSpec::name` | public field | `String` | Tool name. |
| `iyon_api::ModelToolSpec::description` | public field | `String` | Human-readable tool description. |
| `iyon_api::ModelToolSpec::input_schema` | public field | `serde_json::Value` | JSON schema for tool input. |

## Stream module

Definitions are in `crates/iyon-api/src/stream.rs`; the types are re-exported by `crates/iyon-api/src/lib.rs`.

### `ModelStreamEvent`

| Path | Kind | Signature | Purpose |
|---|---|---|---|
| `iyon_api::ModelStreamEvent` | enum — re-export | `pub enum ModelStreamEvent` | Incremental lifecycle, content, usage, completion, and error events from a provider stream. |
| `iyon_api::ModelStreamEvent::Started` | unit variant | — | The provider stream has started. |
| `iyon_api::ModelStreamEvent::TextStart` | variant with public field | `{ content_index: usize }` | Begins a text content block. |
| `iyon_api::ModelStreamEvent::TextStart::content_index` | public variant field | `usize` | Index of the text block. |
| `iyon_api::ModelStreamEvent::TextDelta` | variant with public fields | `{ content_index: usize, delta: String }` | Adds text to a text block. |
| `iyon_api::ModelStreamEvent::TextDelta::content_index` | public variant field | `usize` | Index of the text block. |
| `iyon_api::ModelStreamEvent::TextDelta::delta` | public variant field | `String` | Newly streamed text. |
| `iyon_api::ModelStreamEvent::TextEnd` | variant with public fields | `{ content_index: usize, text: String }` | Completes a text block with its full text. |
| `iyon_api::ModelStreamEvent::TextEnd::content_index` | public variant field | `usize` | Index of the text block. |
| `iyon_api::ModelStreamEvent::TextEnd::text` | public variant field | `String` | Complete text value. |
| `iyon_api::ModelStreamEvent::ThinkingStart` | variant with public field | `{ content_index: usize }` | Begins a thinking block. |
| `iyon_api::ModelStreamEvent::ThinkingStart::content_index` | public variant field | `usize` | Index of the thinking block. |
| `iyon_api::ModelStreamEvent::ThinkingDelta` | variant with public fields | `{ content_index: usize, delta: String }` | Adds text to a thinking block. |
| `iyon_api::ModelStreamEvent::ThinkingDelta::content_index` | public variant field | `usize` | Index of the thinking block. |
| `iyon_api::ModelStreamEvent::ThinkingDelta::delta` | public variant field | `String` | Newly streamed thinking text. |
| `iyon_api::ModelStreamEvent::ThinkingEnd` | variant with public fields | `{ content_index: usize, text: String }` | Completes a thinking block with its full text. |
| `iyon_api::ModelStreamEvent::ThinkingEnd::content_index` | public variant field | `usize` | Index of the thinking block. |
| `iyon_api::ModelStreamEvent::ThinkingEnd::text` | public variant field | `String` | Complete thinking text. |
| `iyon_api::ModelStreamEvent::ToolCallStart` | variant with public fields | `{ content_index: usize, id: Option<String>, name: Option<String> }` | Begins a streamed tool call. |
| `iyon_api::ModelStreamEvent::ToolCallStart::content_index` | public variant field | `usize` | Index of the tool-call block. |
| `iyon_api::ModelStreamEvent::ToolCallStart::id` | public variant field | `Option<String>` | Tool-call identifier, when available. |
| `iyon_api::ModelStreamEvent::ToolCallStart::name` | public variant field | `Option<String>` | Tool name, when available. |
| `iyon_api::ModelStreamEvent::ToolCallDelta` | variant with public fields | `{ content_index: usize, id: Option<String>, name: Option<String>, arguments_delta: String }` | Adds streamed tool-call arguments. |
| `iyon_api::ModelStreamEvent::ToolCallDelta::content_index` | public variant field | `usize` | Index of the tool-call block. |
| `iyon_api::ModelStreamEvent::ToolCallDelta::id` | public variant field | `Option<String>` | Tool-call identifier, when available. |
| `iyon_api::ModelStreamEvent::ToolCallDelta::name` | public variant field | `Option<String>` | Tool name, when available. |
| `iyon_api::ModelStreamEvent::ToolCallDelta::arguments_delta` | public variant field | `String` | Newly streamed JSON-argument text. |
| `iyon_api::ModelStreamEvent::ToolCallEnd` | variant with public fields | `{ content_index: usize, id: String, name: String, arguments: Value }` | Completes a tool call with parsed arguments. |
| `iyon_api::ModelStreamEvent::ToolCallEnd::content_index` | public variant field | `usize` | Index of the tool-call block. |
| `iyon_api::ModelStreamEvent::ToolCallEnd::id` | public variant field | `String` | Complete tool-call identifier. |
| `iyon_api::ModelStreamEvent::ToolCallEnd::name` | public variant field | `String` | Complete tool name. |
| `iyon_api::ModelStreamEvent::ToolCallEnd::arguments` | public variant field | `serde_json::Value` | Parsed JSON arguments. |
| `iyon_api::ModelStreamEvent::Usage` | variant with public field | `{ usage: Usage }` | Reports token usage. |
| `iyon_api::ModelStreamEvent::Usage::usage` | public variant field | `Usage` | Token-usage counters. |
| `iyon_api::ModelStreamEvent::Done` | variant with public field | `{ stop_reason: StopReason }` | Marks normal stream completion. |
| `iyon_api::ModelStreamEvent::Done::stop_reason` | public variant field | `StopReason` | Reason generation stopped. |
| `iyon_api::ModelStreamEvent::Error` | variant with public field | `{ message: String }` | Reports an error event in the stream. |
| `iyon_api::ModelStreamEvent::Error::message` | public variant field | `String` | Error message. |

### `Usage`

| Path | Kind | Signature | Purpose |
|---|---|---|---|
| `iyon_api::Usage` | struct — re-export | `pub struct Usage` | Token-usage counters emitted by a provider. |
| `iyon_api::Usage::input_tokens` | public field | `u64` | Non-cached input-token count. |
| `iyon_api::Usage::output_tokens` | public field | `u64` | Generated output-token count. |
| `iyon_api::Usage::cache_read_tokens` | public field | `u64` | Input tokens read from cache. |
| `iyon_api::Usage::cache_write_tokens` | public field | `u64` | Tokens written to cache. |

### `StopReason`

| Path | Kind | Signature | Purpose |
|---|---|---|---|
| `iyon_api::StopReason` | enum — re-export | `pub enum StopReason` | Normalized reason a provider stopped generation. |
| `iyon_api::StopReason::Stop` | unit variant | — | Provider completed normally. |
| `iyon_api::StopReason::Length` | unit variant | — | Output reached a length limit. |
| `iyon_api::StopReason::ToolUse` | unit variant | — | Generation stopped to request tool use. |
| `iyon_api::StopReason::Error` | unit variant | — | Generation stopped because of an error. |
| `iyon_api::StopReason::Aborted` | unit variant | — | Generation was aborted. |

## Provider types

The definitions are in the private `crates/iyon-api/src/providers/` module tree and are re-exported at the crate root by `crates/iyon-api/src/lib.rs`. The providers all implement `iyon_api::ModelApi` and therefore expose the required `stream` method described above.

### `MockModelApi`

| Path | Kind | Signature | Purpose |
|---|---|---|---|
| `iyon_api::MockModelApi` | unit struct — re-export | `pub struct MockModelApi` | In-process provider that returns a delayed mock response. It has no public fields or inherent methods. |
| `iyon_api::MockModelApi::stream` | `ModelApi` implementation method | `fn stream(&self, request: ModelRequest) -> ModelStreamFuture<'_>` | Streams a mock response based on the last user text. Defined in `crates/iyon-api/src/providers/mock.rs`, `impl ModelApi for MockModelApi`. |

### `OpenAICodexModelApi`

| Path | Kind | Signature | Purpose |
|---|---|---|---|
| `iyon_api::OpenAICodexModelApi` | struct — re-export | `pub struct OpenAICodexModelApi` | OpenAI Codex provider client. Its fields are private. |
| `iyon_api::OpenAICodexModelApi::new` | inherent method | `pub fn new(access_token: impl Into<String>, account_id: impl Into<String>) -> Result<Self, ModelError>` | Builds a Codex client using an access token and account ID. |
| `iyon_api::OpenAICodexModelApi::stream` | `ModelApi` implementation method | `fn stream(&self, request: ModelRequest) -> ModelStreamFuture<'_>` | Sends a Codex request and translates its SSE response into `ModelStreamEvent` values. Defined in `crates/iyon-api/src/providers/openai_codex.rs`, `impl ModelApi for OpenAICodexModelApi`. |

### `OpenRouterModelApi`

| Path | Kind | Signature | Purpose |
|---|---|---|---|
| `iyon_api::OpenRouterModelApi` | struct — re-export | `pub struct OpenRouterModelApi` | OpenRouter chat-completions provider client. Its fields are private. |
| `iyon_api::OpenRouterModelApi::new` | inherent method | `pub fn new(api_key: impl Into<String>, model_id: impl Into<String>) -> Result<Self, ModelError>` | Builds an OpenRouter client using the default OpenRouter API base URL. |
| `iyon_api::OpenRouterModelApi::with_base_url` | inherent method | `pub fn with_base_url(api_key: impl Into<String>, model_id: impl Into<String>, base_url: impl Into<String>) -> Result<Self, ModelError>` | Builds an OpenRouter client with an explicitly supplied API base URL. |
| `iyon_api::OpenRouterModelApi::stream` | `ModelApi` implementation method | `fn stream(&self, request: ModelRequest) -> ModelStreamFuture<'_>` | Sends a chat-completions request and translates its SSE response into `ModelStreamEvent` values. Defined in `crates/iyon-api/src/providers/openrouter.rs`, `impl ModelApi for OpenRouterModelApi`. |

## How providers are currently registered

`ModelApi` is the provider surface: its sole required method is `ModelApi::stream` (`crates/iyon-api/src/client.rs`). Concrete providers live under `crates/iyon-api/src/providers/` and are re-exported at the crate root in `crates/iyon-api/src/lib.rs` as `MockModelApi`, `OpenAICodexModelApi`, and `OpenRouterModelApi`.

There is no central provider registry in `iyon-api`. Provider construction and selection happen in the `iyon` binary, `crates/iyon/src/main.rs`:

1. `detect_provider()` checks `IYON_PROVIDER` first. Its accepted values are `openrouter`, `codex`, `openai`, `openai-codex`, and `mock`; an unrecognized explicit value selects OpenRouter. When `IYON_PROVIDER` is unset, it selects OpenRouter if `auth::openrouter_api_key()` finds a key, then Codex if `auth::has_codex_credentials()` finds credentials, and otherwise Mock.
2. `run_interactive()` constructs the selected provider. OpenRouter uses `auth::openrouter_api_key()` and `IYON_MODEL`, defaulting to `deepseek/deepseek-v4-flash:latest`; if the key is unavailable it warns about `OPENROUTER_API_KEY` and immediately falls back to `MockModelApi`. Codex uses `auth::get_valid_credentials()` and constructs `OpenAICodexModelApi`; missing or unavailable credentials also fall back to Mock. Mock directly constructs `MockModelApi`.
3. `run_interactive()` stores the selected concrete provider behind `Arc<dyn iyon_api::ModelApi>`. It passes that trait object, together with `ModelSelection`, to `run_with_model()`.
4. `run_with_model()` creates hooks and passes the `Arc<dyn ModelApi>` into `iyon_core::IyonCore::spawn_on_current_runtime_with_selection_and_hooks`, then starts the TUI with `tui::run_with_core`.

The relevant binary functions are `detect_provider()`, `run_interactive()`, `run_with_model()`, and `mock_selection()` in `crates/iyon/src/main.rs`. `OPENROUTER_TITLE` is additionally read by `OpenRouterModelApi::with_base_url` in `crates/iyon-api/src/providers/openrouter.rs` for optional request attribution; it does not select or register a provider.
