//! `annee` — Family Year Planner server and CLI.

use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

use annee_server::config::Env;
use annee_server::{app, bootstrap};

#[derive(Parser)]
#[command(name = "annee", version, about = "Family Year Planner")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Run the web server (default).
    Serve,
    /// Run the server with the in-memory demo store (same as APP_MODE=demo).
    Demo,
    /// Validate configuration, secrets and Google Calendar access.
    CheckConfig,
    /// Exit 0 if the local server answers /healthz (used by the container healthcheck).
    Healthcheck,
    /// Refresh school and public holidays in the Repères calendar.
    Import {
        /// Print what would change without writing anything.
        #[arg(long)]
        dry_run: bool,
    },
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();
    let cli = Cli::parse();
    let result = match cli.command.unwrap_or(Command::Serve) {
        Command::Serve => serve(false).await,
        Command::Demo => serve(true).await,
        Command::Healthcheck => annee_server::cli::healthcheck().await,
        Command::CheckConfig => annee_server::cli::check_config().await,
        Command::Import { dry_run } => annee_server::cli::import(dry_run, false).await,
    };
    if let Err(e) = result {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

async fn serve(force_demo: bool) -> anyhow::Result<()> {
    let env = Env::from_process(force_demo).map_err(|e| anyhow::anyhow!(e))?;
    let config = bootstrap::load_config(&env)?;
    let state = bootstrap::build_state(&env, config).await?;
    let _scheduler = bootstrap::start_background(&state).await?;
    let listener = tokio::net::TcpListener::bind(&env.bind).await?;
    tracing::info!(bind = %env.bind, mode = env.mode.as_str(), "listening");
    axum::serve(
        listener,
        app::router(state).into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let term = async {
        if let Ok(mut s) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();
    tokio::select! { _ = ctrl_c => {}, _ = term => {} }
}
