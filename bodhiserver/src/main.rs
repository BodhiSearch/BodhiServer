use bodhiserver::{
  build_server, port_from_env_vars, server::ServerHandle, shutdown_signal, DEFAULT_HOST,
  DEFAULT_PORT_STR,
};
use clap::{Parser, Subcommand};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

static ENV_BODHISERVER_PORT: &str = "BODHISERVER_PORT";

#[derive(Parser)]
#[command(version)]
#[command(about = "Run GenerativeAI LLMs locally and serve them via OpenAI compatible API")]
struct Cli {
  #[command(subcommand)]
  command: Command,
}

#[derive(Subcommand)]
enum Command {
  /// start the server
  Serve {
    #[clap(short='H', default_value = DEFAULT_HOST)]
    host: Option<String>,
    #[clap(short, default_value = DEFAULT_PORT_STR)]
    port: Option<u16>,
  },
}

#[tokio::main]
pub async fn main() {
  dotenv::dotenv().ok();
  tracing_subscriber::registry()
    .with(tracing_subscriber::fmt::layer())
    .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
    .init();
  let result = start().await;
  if let Err(err) = result {
    tracing::warn!(err = ?err, "application exited with error");
    std::process::exit(1);
  }
}

async fn start() -> anyhow::Result<()> {
  let cli = Cli::parse();
  match cli.command {
    Command::Serve { host, port } => launch_server(host, port).await,
  }
}

async fn launch_server(host: Option<String>, port: Option<u16>) -> anyhow::Result<()> {
  let host = host.unwrap_or_else(|| String::from(DEFAULT_HOST));
  let port = port.unwrap_or_else(|| port_from_env_vars(std::env::var(ENV_BODHISERVER_PORT)));
  let ServerHandle { server, shutdown } = build_server(host, port).await?;
  let server_join = tokio::spawn(async move {
    if let Err(err) = server.await {
      tracing::error!(err = ?err, "Server error");
    }
  });
  tokio::spawn(async move {
    shutdown_signal().await;
    shutdown.send(()).unwrap();
  });
  server_join.await?;
  Ok(())
}
