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

pub fn main() {
  dotenv::dotenv().ok();
  tracing_subscriber::registry()
    .with(tracing_subscriber::fmt::layer())
    .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
    .init();
  let result = start();
  if let Err(err) = result {
    tracing::warn!(err = ?err, "application exited with error");
    std::process::exit(1);
  }
}

fn start() -> anyhow::Result<()> {
  let cli = Cli::parse();
  match cli.command {
    Command::Serve { host, port, .. } => serve(host, port),
  }
}

fn serve(host: Option<String>, port: Option<u16>) -> anyhow::Result<()> {
  let host = host.unwrap_or_else(|| String::from(DEFAULT_HOST));
  let port = port.unwrap_or_else(|| port_from_env_vars(std::env::var(ENV_BODHISERVER_PORT)));
  let runtime = tokio::runtime::Builder::new_multi_thread()
    .enable_all()
    .build();
  match runtime {
    Ok(runtime) => runtime.block_on(async move { start_server(host, port).await }),
    Err(err) => Err(anyhow::Error::new(err)),
  }
}

async fn start_server(host: String, port: u16) -> anyhow::Result<()> {
  let ServerHandle { server, shutdown } = build_server(host, port).await?;
  let server_join = tokio::spawn(async move {
    match server.await {
      Ok(()) => Ok(()),
      Err(err) => {
        tracing::error!(err = ?err, "server encountered an error");
        Err(err)
      }
    }
  });
  tokio::spawn(async move {
    shutdown_signal().await;
    shutdown.send(()).unwrap();
  });
  (server_join.await?)?;
  Ok(())
}
