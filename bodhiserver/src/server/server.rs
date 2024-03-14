use crate::llama_cpp::LlamaCpp;
use crate::server::routes::build_routes;
use llama_cpp_2::model::LlamaModel;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::task::{Context as TaskContext, Poll};
use tokio::net::TcpListener;
use tokio::sync::oneshot::{self, Receiver, Sender};

#[derive(Debug, Clone)]
pub struct ServerArgs {
  pub host: String,
  pub port: u16,
  pub model: PathBuf,
  pub lazy_load_model: bool,
}

pub struct ServerHandle {
  pub server: Server,
  pub shutdown: oneshot::Sender<()>,
  pub ready_rx: oneshot::Receiver<()>,
}

pub fn build_server_handle(server_args: ServerArgs) -> anyhow::Result<ServerHandle> {
  let (shutdown, shutdown_rx) = oneshot::channel::<()>();
  let (ready, ready_rx) = oneshot::channel::<()>();
  let server = Server::new(server_args, ready, shutdown_rx);
  let result = ServerHandle {
    server,
    shutdown,
    ready_rx,
  };
  Ok(result)
}

pub struct Server {
  server_args: ServerArgs,
  ready: Sender<()>,
  rx: Receiver<()>,
}

impl Server {
  fn new(server_args: ServerArgs, ready: Sender<()>, rx: Receiver<()>) -> Self {
    Self {
      server_args,
      ready,
      rx,
    }
  }

  pub async fn start(mut self) -> anyhow::Result<()> {
    let model = if !self.server_args.lazy_load_model {
      let model = self.init_llama_model().await?;
      Some(model)
    } else {
      None
    };
    let app = build_routes(model);
    let addr = format!("{}:{}", &self.server_args.host, &self.server_args.port);
    let listener = TcpListener::bind(&addr).await?;
    tracing::info!(addr = addr, "Server started");
    let axum_server =
      axum::serve(listener, app).with_graceful_shutdown(ShutdownWrapper { rx: self.rx });
    self.ready.send(()).unwrap();
    axum_server.await?;
    Ok(())
  }

  pub async fn init_llama_model(&mut self) -> anyhow::Result<LlamaModel> {
    let llama_cpp = LlamaCpp::init()?;
    let llama_model = llama_cpp.load_model(&self.server_args.model)?;
    Ok(llama_model)
  }
}

pub struct ShutdownWrapper {
  rx: tokio::sync::oneshot::Receiver<()>,
}

impl Future for ShutdownWrapper {
  type Output = ();

  fn poll(mut self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<Self::Output> {
    match Pin::new(&mut self.rx).poll(cx) {
      Poll::Ready(_) => Poll::Ready(()),
      Poll::Pending => Poll::Pending,
    }
  }
}
