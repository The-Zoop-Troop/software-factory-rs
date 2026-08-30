//! `console`: the remote-control plane (docs/exec-plans/completed/remote-control.md).
//! Serves A2A over HTTP for every rig in the registry. Credentials for providers never
//! pass through here; plans are handed to each rig's own planner command.
#![forbid(unsafe_code)]
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::indexing_slicing,
        clippy::disallowed_types,
        clippy::panic
    )
)]
#![allow(
    clippy::disallowed_methods,
    reason = "tokio::main expands to expect; the binary is the wiring site"
)]

mod adapters;
mod alerts;
mod auth;
mod config;
#[cfg(feature = "fake")]
mod fake;
mod rpc;
mod server;
#[cfg(test)]
mod server_tests;
mod ui;
mod webapp;

use std::path::PathBuf;
use std::sync::Arc;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "console",
    version,
    about = "A2A control plane over factory rigs"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Serve the A2A API.
    Serve {
        /// Rig registry (TOML, see docs/DEPLOYMENT.md).
        #[arg(long, default_value = "console/rigs.toml")]
        registry: PathBuf,
        /// Token file (TOML; sha256 of each bearer token and its grants).
        #[arg(long, default_value = "console/tokens.toml")]
        tokens: PathBuf,
        /// Address to bind.
        #[arg(long, default_value = "127.0.0.1:7700")]
        listen: String,
        /// URL clients reach this console at (goes into the Agent Card).
        #[arg(long, default_value = "http://127.0.0.1:7700")]
        public_url: String,
        /// Webhook that receives `{"rig","text"}` when a task needs a human or finishes.
        #[arg(long, env = "CONSOLE_ALERT_URL")]
        alert_url: Option<String>,
        /// Seconds between alert sweeps.
        #[arg(long, default_value_t = 30)]
        alert_interval: u64,
        /// Serve an in-memory rig `toy` with token `fake` (UI development and e2e tests).
        #[cfg(feature = "fake")]
        #[arg(long)]
        fake: bool,
    },
    /// Print the sha256 of a token read from stdin, for the token file.
    HashToken,
    /// Print the Agent Card a rig would publish (used to generate the API reference).
    Card {
        #[arg(long, default_value = "toy")]
        rig: String,
        #[arg(long, default_value = "https://console.example")]
        public_url: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _telemetry = infra::telemetry::init("console", &infra::TelemetryConfig::from_env())?;
    match Cli::parse().command {
        Command::HashToken => {
            let mut token = String::new();
            std::io::stdin().read_line(&mut token)?;
            println!("{}", auth::hash(token.trim()));
            Ok(())
        }
        Command::Card { rig, public_url } => {
            let rig = domain::RigName::try_new(&rig)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&server::agent_card(&public_url, &rig))?
            );
            Ok(())
        }
        Command::Serve {
            registry,
            tokens,
            listen,
            public_url,
            alert_url,
            alert_interval,
            #[cfg(feature = "fake")]
            fake,
        } => {
            #[cfg(feature = "fake")]
            let (auth, registry): (
                Arc<dyn app::Authenticator>,
                Arc<dyn app::RigRegistry>,
            ) = if fake {
                let (a, r) = fake::world().await?;
                (Arc::new(a), Arc::new(r))
            } else {
                let rigs = config::load_registry(&registry)?;
                let auth = auth::TokenAuth::new(config::load_tokens(&tokens)?);
                (
                    Arc::new(auth),
                    Arc::new(adapters::FileRegistry::build(&rigs)?),
                )
            };
            #[cfg(not(feature = "fake"))]
            let (auth, registry): (
                Arc<dyn app::Authenticator>,
                Arc<dyn app::RigRegistry>,
            ) = {
                let rigs = config::load_registry(&registry)?;
                let auth = auth::TokenAuth::new(config::load_tokens(&tokens)?);
                (
                    Arc::new(auth),
                    Arc::new(adapters::FileRegistry::build(&rigs)?),
                )
            };
            let state = server::AppState {
                auth,
                registry,
                clock: Arc::new(infra::SystemClock),
                public_url,
                poll: std::time::Duration::from_secs(1),
            };
            tracing::info!(ui = webapp::built(), "web console embedded");
            if let Some(url) = alert_url {
                tokio::spawn(alerts::run(
                    state.registry.clone(),
                    state.clock.clone(),
                    Arc::new(alerts::Webhook::new(url)),
                    domain::Duration::from_seconds(alert_interval.max(5)),
                ));
            }
            let listener = tokio::net::TcpListener::bind(&listen).await?;
            tracing::info!(%listen, rigs = state.registry.names().len(), "console listening");
            axum::serve(listener, server::router(state))
                .with_graceful_shutdown(async {
                    let _ = tokio::signal::ctrl_c().await;
                })
                .await?;
            Ok(())
        }
    }
}
