mod app;
mod shutdown;
mod utils;
use crate::server::app::build_app;
pub use crate::server::shutdown::shutdown_signal;
pub use crate::server::utils::{port_from_env_vars, DEFAULT_PORT, DEFAULT_PORT_STR, DEFAULT_HOST};
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::net::TcpListener;
use tokio::sync::oneshot;

pub struct ServerHandle {
  pub server: axum::serve::WithGracefulShutdown<axum::Router, axum::Router, ShutdownWrapper>,
  pub shutdown: oneshot::Sender<()>,
}

pub async fn build_server(host: String, port: u16) -> anyhow::Result<ServerHandle> {
  let addr = format!("{}:{}", host, port);
  let listener = TcpListener::bind(&addr).await?;
  let (tx, rx) = oneshot::channel::<()>();
  let app = build_app();
  tracing::info!(addr = addr, "Server started");
  let result = ServerHandle {
    server: axum::serve(listener, app).with_graceful_shutdown(ShutdownWrapper { rx }),
    shutdown: tx,
  };
  Ok(result)
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
