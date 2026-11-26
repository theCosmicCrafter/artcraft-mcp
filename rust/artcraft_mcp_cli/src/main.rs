//! artcraft-mcp 2.0 — dual-transport MCP server for ArtCraft.
//!
//! stdio mode: newline-delimited JSON-RPC over stdin/stdout (legacy + modern).
//! http mode:  stateless Streamable HTTP on /mcp (MCP 2026-07-28), with legacy
//!             session support for pre-2026 clients.
//!
//! Stdout is protocol-only in stdio mode: all logs go to stderr.

use clap::{Parser, ValueEnum};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod auth;
mod client;
mod coercion;
mod discovery;
mod registry;
mod server;
mod tasks_store;
mod telemetry;
mod tools;
mod types;

#[derive(Copy, Clone, Debug, ValueEnum, PartialEq, Eq)]
pub enum Transport {
    Stdio,
    Http,
}

#[derive(Copy, Clone, Debug, ValueEnum, PartialEq, Eq)]
pub enum ToolMode {
    /// ~10 hot tools + 3 meta-tools for 2026-07-28 clients; full list for legacy.
    Progressive,
    /// All tools exposed directly (legacy behavior).
    Full,
    /// Like progressive, but tuned for programmatic tool calling hosts.
    Codemode,
}

#[derive(Parser, Debug)]
#[command(name = "artcraft-mcp", version, about = "ArtCraft MCP server (2026-07-28 dual-transport)")]
pub struct Cli {
    /// Transport to serve.
    #[arg(long, value_enum, default_value_t = Transport::Stdio)]
    pub transport: Transport,

    /// Bind address for --transport http.
    #[arg(long, default_value = "127.0.0.1:8787")]
    pub bind: String,

    /// Tool exposure mode.
    #[arg(long, value_enum, default_value_t = ToolMode::Progressive)]
    pub tool_mode: ToolMode,

    /// Confirm before jobs estimated above this many credits (0 = confirm everything billable).
    #[arg(long, default_value_t = 50)]
    pub confirm_over_credits: u64,

    /// Auto-approve jobs estimated at or under this many credits.
    #[arg(long, default_value_t = 0)]
    pub auto_approve_under: u64,

    /// Never ask for cost confirmation (headless CI).
    #[arg(long)]
    pub no_confirm: bool,

    /// Required to bind --transport http to a non-loopback address.
    #[arg(long)]
    pub allow_remote: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();
    // All tracing goes to stderr; stdout carries JSON-RPC in stdio mode.
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stderr)
                .with_ansi(false),
        )
        .init();

    let cli = Cli::parse();
    tracing::info!(transport = ?cli.transport, "artcraft-mcp starting");

    match cli.transport {
        Transport::Stdio => server::serve_stdio(cli).await,
        Transport::Http => server::serve_http(cli).await,
    }
}
