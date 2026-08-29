//! `console`: the remote-control plane (docs/exec-plans/active/remote-control.md).
//! Serves A2A over HTTP for every rig in the registry. Credentials for providers never
//! pass through here; plans are handed to each rig's own planner command.
#![forbid(unsafe_code)]
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::indexing_slicing,
        clippy::panic
    )
)]
#![allow(
    clippy::disallowed_methods,
    reason = "tokio::main expands to expect; the binary is the wiring site"
)]

mod adapters;
mod auth;
mod config;
mod rpc;
mod server;
#[cfg(test)]
mod server_tests;

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
    },
    /// Print the sha256 of a token read from stdin, for the token file.
    HashToken,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    match Cli::parse().command {
        Command::HashToken => {
            let mut token = String::new();
            std::io::stdin().read_line(&mut token)?;
            println!("{}", auth::hash(token.trim()));
            Ok(())
        }
        Command::Serve {
            registry,
            tokens,
            listen,
            public_url,
        } => {
            let rigs = config::load_registry(&registry)?;
            let auth = auth::TokenAuth::new(config::load_tokens(&tokens)?);
            let registry = adapters::FileRegistry::build(&rigs)?;
            let state = server::AppState {
                auth: Arc::new(auth),
                registry: Arc::new(registry),
                clock: Arc::new(infra::SystemClock),
                public_url,
                poll: std::time::Duration::from_secs(1),
            };
            let listener = tokio::net::TcpListener::bind(&listen).await?;
            tracing::info!(%listen, rigs = rigs.len(), "console listening");
            axum::serve(listener, server::router(state))
                .with_graceful_shutdown(async {
                    let _ = tokio::signal::ctrl_c().await;
                })
                .await?;
            Ok(())
        }
    }
}
