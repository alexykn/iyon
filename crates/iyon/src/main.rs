mod auth;

use std::sync::Arc;

use iyon::tui;

use anyhow::Result;
use clap::{Parser, Subcommand};
use iyon_api::OpenAICodexModelApi;
use iyon_core::{ModelSelection, tools::ToolHookSet};

const DEFAULT_OPENROUTER_MODEL: &str = "nvidia/nemotron-3.5-lightning:free";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProviderKind {
    OpenRouter,
    Codex,
    Mock,
}

#[derive(Debug, Parser)]
#[command(name = "iyon")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    Auth {
        #[command(subcommand)]
        command: AuthCommand,
    },
    Run,
}

#[derive(Debug, Subcommand)]
enum AuthCommand {
    Login,
    Logout,
    Status,
}

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("{error:#}");
        std::process::exit(1);
    }
}

fn build_tool_hooks() -> ToolHookSet {
    // Runtime bridge point for extension-backed tool_call/tool_result hooks.
    // Intentionally no-op until extension runner integration lands.
    ToolHookSet::default()
}

async fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.command.unwrap_or(Command::Run) {
        Command::Auth { command } => match command {
            AuthCommand::Login => auth::login().await,
            AuthCommand::Logout => auth::clear_credentials(),
            AuthCommand::Status => auth::print_status(),
        },
        Command::Run => run_interactive().await,
    }
}

/// Starts the TUI with the best available provider. Login is deliberately soft: if no
/// provider or credentials are available it falls back to the mock provider so the app
/// can still start (useful for testing and for in-app `/login` flows later).
async fn run_interactive() -> Result<()> {
    let (model, selection): (Arc<dyn iyon_api::ModelApi>, ModelSelection) = match detect_provider()
    {
        ProviderKind::OpenRouter => {
            let Some(key) = auth::openrouter_api_key() else {
                eprintln!(
                    "warning: no OpenRouter API key (set OPENROUTER_API_KEY); starting unconfigured"
                );
                return run_with_model(Arc::new(iyon_api::MockModelApi), mock_selection()).await;
            };
            let model_id = std::env::var("IYON_MODEL")
                .unwrap_or_else(|_| DEFAULT_OPENROUTER_MODEL.to_string());
            (
                Arc::new(iyon_api::OpenRouterModelApi::new(key, model_id.clone())?),
                ModelSelection {
                    provider: "openrouter".to_string(),
                    model_id,
                },
            )
        }
        ProviderKind::Codex => match auth::get_valid_credentials().await {
            Ok(Some(creds)) => (
                Arc::new(OpenAICodexModelApi::new(creds.access, creds.account_id)?),
                ModelSelection {
                    provider: "openai-codex".to_string(),
                    model_id: "gpt-5.3-codex".to_string(),
                },
            ),
            Ok(None) => {
                eprintln!("warning: not logged in to OpenAI Codex; starting unconfigured");
                (Arc::new(iyon_api::MockModelApi), mock_selection())
            }
            Err(error) => {
                eprintln!(
                    "warning: OpenAI Codex login unavailable ({error:#}); starting unconfigured"
                );
                (Arc::new(iyon_api::MockModelApi), mock_selection())
            }
        },
        ProviderKind::Mock => (Arc::new(iyon_api::MockModelApi), mock_selection()),
    };
    run_with_model(model, selection).await
}

async fn run_with_model(
    model: Arc<dyn iyon_api::ModelApi>,
    selection: ModelSelection,
) -> Result<()> {
    let tool_hooks = build_tool_hooks();
    let core = iyon_core::IyonCore::spawn_on_current_runtime_with_selection_and_hooks(
        model,
        selection.clone(),
        tool_hooks,
    );
    tui::run_with_core(core, selection).await
}

fn mock_selection() -> ModelSelection {
    ModelSelection {
        provider: "mock".to_string(),
        model_id: "mock".to_string(),
    }
}

/// Provider auto-detection. Explicit `IYON_PROVIDER` wins; otherwise prefer an
/// available OpenRouter key, then OpenAI Codex credentials, then fall back to mock.
fn detect_provider() -> ProviderKind {
    if let Ok(value) = std::env::var("IYON_PROVIDER") {
        return match value.trim().to_ascii_lowercase().as_str() {
            "openrouter" => ProviderKind::OpenRouter,
            "codex" | "openai" | "openai-codex" => ProviderKind::Codex,
            "mock" => ProviderKind::Mock,
            _ => ProviderKind::OpenRouter,
        };
    }
    if auth::openrouter_api_key().is_some() {
        ProviderKind::OpenRouter
    } else if auth::has_codex_credentials() {
        ProviderKind::Codex
    } else {
        ProviderKind::Mock
    }
}
