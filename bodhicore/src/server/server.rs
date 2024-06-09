use crate::error::Common;
use axum::Router;
use tokio::{
  net::TcpListener,
  sync::oneshot::{self, Receiver, Sender},
};

/// Server encapsulates the parameters to start, broadcast ready lifecycle, and receive shutdown request for a server
/// It contains the parameters to start the server on given host, port etc. and
/// contains a ready sender channel to notify the requester when the server is ready to receive connection and
/// contains the shutdown receiver channel to listen to shutdown request from requester
pub struct Server {
  host: String,
  port: u16,
  ready: Sender<()>,
  shutdown_rx: Receiver<()>,
}

#[async_trait::async_trait]
pub trait ShutdownCallback: Send + Sync {
  async fn shutdown(&self);
}

/// ServerHandle encapuslates the handles to start, listen to when server is ready, and request shutdown for a running server
pub struct ServerHandle {
  pub server: Server,
  pub shutdown: oneshot::Sender<()>,
  pub ready_rx: oneshot::Receiver<()>,
}

pub fn build_server_handle(host: &str, port: u16) -> ServerHandle {
  let (shutdown, shutdown_rx) = oneshot::channel::<()>();
  let (ready, ready_rx) = oneshot::channel::<()>();
  let server = Server::new(host, port, ready, shutdown_rx);
  ServerHandle {
    server,
    shutdown,
    ready_rx,
  }
}

impl Server {
  fn new(host: &str, port: u16, ready: Sender<()>, shutdown_rx: Receiver<()>) -> Self {
    Self {
      host: host.to_string(),
      port,
      ready,
      shutdown_rx,
    }
  }

  pub async fn start_new(
    self,
    app: Router,
    callback: Option<Box<dyn ShutdownCallback>>,
  ) -> crate::error::Result<()> {
    let Server {
      host,
      port,
      ready,
      shutdown_rx,
    } = self;
    let addr = format!("{}:{}", host, port);
    let listener = TcpListener::bind(&addr).await.map_err(Common::Io)?;
    tracing::info!(addr = addr, "server started");
    let axum_server = axum::serve(listener, app).with_graceful_shutdown(async move {
      match shutdown_rx.await {
        Ok(()) => {
          tracing::info!("received signal to shutdown the server");
        }
        Err(err) => {
          tracing::warn!(
            ?err,
            "shutdown sender dropped without sending shutdown signal"
          );
        }
      };
      if let Some(callback) = callback {
        (*callback).shutdown().await;
      }
    });
    if ready.send(()).is_err() {
      tracing::warn!("ready receiver dropped before start signal notified")
    };
    axum_server.await.map_err(Common::Io)?;
    Ok(())
  }
}

#[cfg(test)]
mod test {
  use super::{build_server_handle, ServerHandle, ShutdownCallback};
  use anyhow::anyhow;
  use axum::{routing::get, Router};
  use reqwest::StatusCode;
  use std::sync::{Arc, Mutex};

  struct ShutdownTestCallback {
    callback: Arc<Mutex<bool>>,
  }

  #[async_trait::async_trait]
  impl ShutdownCallback for ShutdownTestCallback {
    async fn shutdown(&self) {
      let mut c = self.callback.lock().unwrap();
      *c = true;
    }
  }

  // TODO: unstable test, use ctrlc crate
  #[tokio::test]
  pub async fn test_server_start_stop_with_callback() -> anyhow::Result<()> {
    let host = "localhost".to_string();
    let port = rand::random::<u16>() % 65535;
    let ServerHandle {
      server,
      shutdown,
      ready_rx,
    } = build_server_handle(&host, port);
    let app = Router::new().route("/ping", get(|| async { (StatusCode::OK, "pong") }));
    let callback_received = Arc::new(Mutex::new(false));
    let callback = ShutdownTestCallback {
      callback: callback_received.clone(),
    };
    let join_handle = tokio::spawn(server.start_new(app, Some(Box::new(callback))));
    ready_rx.await?;
    let response = reqwest::Client::new()
      .get(format!("http://{host}:{port}/ping"))
      .send()
      .await?
      .text()
      .await?;
    assert_eq!("pong", response);
    shutdown
      .send(())
      .map_err(|_| anyhow!("shutdown send failed"))?;
    (join_handle.await?)?;
    assert!(*callback_received.lock().unwrap());
    let response = reqwest::Client::new()
      .get(format!("http://{host}:{port}/ping"))
      .send()
      .await;
    assert!(response.is_err());
    Ok(())
  }
}
