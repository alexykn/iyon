mod auth;

use std::sync::Arc;

use anyhow::{Result, bail};
use clap::{Parser, Subcommand};
use iyon_api::OpenAICodexModelApi;
use iyon_core::tools::ToolHookSet;

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
        Command::Run => {
            let creds = auth::get_valid_credentials().await?;
            let Some(creds) = creds else {
                bail!("not logged in. Run: iyon auth login");
            };
            let model = Arc::new(OpenAICodexModelApi::new(creds.access, creds.account_id)?);
            let tool_hooks = build_tool_hooks();
            let core = iyon_core::IyonCore::spawn_on_current_runtime_and_hooks(model, tool_hooks);
            iyon_tui::run_with_core(core)
        }
    }
}
