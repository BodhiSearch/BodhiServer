use super::{llama_log_set, null_log_callback, tiny_llama, LLAMA_BACKEND_LOCK};
use anyhow::Result;
use bodhiserver::{build_server_handle, ServerArgs, ServerHandle};
use rstest::fixture;
use std::path::PathBuf;

pub struct TestServerHandle {
  pub host: String,
  pub port: u16,
  pub shutdown: tokio::sync::oneshot::Sender<()>,
  pub join: tokio::task::JoinHandle<Result<()>>,
}

#[fixture]
pub async fn test_server(
  #[future] tiny_llama: Result<PathBuf>,
) -> anyhow::Result<TestServerHandle> {
  unsafe {
    llama_log_set(null_log_callback, std::ptr::null_mut());
  }
  let _guard = LLAMA_BACKEND_LOCK.lock().await;
  let host = String::from("127.0.0.1");
  let port = rand::random::<u16>();
  let server_args = ServerArgs {
    host: host.clone(),
    port,
    model: tiny_llama.await?,
    lazy_load_model: false,
  };
  let ServerHandle {
    server,
    shutdown,
    ready_rx,
  } = build_server_handle(server_args)?;
  let join = tokio::spawn(server.start());
  ready_rx.await?;
  Ok(TestServerHandle {
    host,
    port,
    shutdown,
    join,
  })
}
