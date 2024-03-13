mod app;
mod shutdown;
mod utils;
use crate::llama_cpp::LlamaCpp;
use crate::server::app::build_app;
pub use crate::server::shutdown::shutdown_signal;
pub use crate::server::utils::{port_from_env_vars, DEFAULT_HOST, DEFAULT_PORT, DEFAULT_PORT_STR};
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::net::TcpListener;
use tokio::sync::oneshot;

#[derive(Debug, Clone)]
pub struct ServerArgs {
  pub host: String,
  pub port: u16,
  pub model: PathBuf,
}

pub struct ServerHandle {
  pub server: Server,
  pub shutdown: oneshot::Sender<()>,
}

pub async fn build_server(server_args: ServerArgs) -> anyhow::Result<ServerHandle> {
  let addr = format!("{}:{}", server_args.host, server_args.port);
  let listener = TcpListener::bind(&addr).await?;
  let (tx, rx) = oneshot::channel::<()>();
  let app = build_app();
  tracing::info!(addr = addr, "Server started");
  let server = Server {
    server: axum::serve(listener, app).with_graceful_shutdown(ShutdownWrapper { rx }),
  };
  let result = ServerHandle {
    server,
    shutdown: tx,
  };
  Ok(result)
}

pub struct Server {
  server: axum::serve::WithGracefulShutdown<axum::Router, axum::Router, ShutdownWrapper>,
}

impl Server {
  pub async fn start(self) -> anyhow::Result<()> {
    let _llama_cpp = LlamaCpp::init()?;
    self.server.await?;
    Ok(())
  }
}

pub struct ShutdownWrapper {
  rx: tokio::sync::oneshot::Receiver<()>,
}

impl Future for ShutdownWrapper {
  type Output = ();

  fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
    match Pin::new(&mut self.rx).poll(cx) {
      Poll::Ready(_) => Poll::Ready(()),
      Poll::Pending => Poll::Pending,
    }
  }
}
