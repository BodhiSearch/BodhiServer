mod utils;
use bodhiserver::{build_server_handle, ServerArgs, ServerHandle};
use rstest::rstest;
use std::path::PathBuf;
use utils::{empty_model, tiny_llama, LLAMA_BACKEND_LOCK};

#[rstest]
#[tokio::test]
pub async fn test_build_server_ping(empty_model: PathBuf) -> anyhow::Result<()> {
  let host = String::from("127.0.0.1");
  let port = rand::random::<u16>();
  let server_args = ServerArgs {
    host: host.clone(),
    port,
    model: empty_model,
    lazy_load_model: true,
  };
  let ServerHandle {
    server,
    shutdown,
    ready_rx,
  } = build_server_handle(server_args)?;
  let join = tokio::spawn(server.start());
  ready_rx.await?;
  let ping_endpoint = format!("http://{}:{}/ping", host, port);
  let response = reqwest::get(&ping_endpoint).await?.text().await?;
  assert_eq!(response, "pong");
  shutdown.send(()).unwrap();
  let server_result = join.await?;
  assert!(server_result.is_ok());
  let response = reqwest::get(&ping_endpoint).await;
  assert!(response.is_err());
  assert!(reqwest::Error::is_connect(&response.unwrap_err()));
  Ok(())
}

#[tokio::test]
pub async fn test_build_server_with_model_load() -> anyhow::Result<()> {
  let _guard = LLAMA_BACKEND_LOCK.lock().await;
  let tiny_llama = tiny_llama().await?;
  let host = String::from("127.0.0.1");
  let port = rand::random::<u16>();
  let server_args = ServerArgs {
    host: host.clone(),
    port,
    model: tiny_llama,
    lazy_load_model: false,
  };
  let ServerHandle {
    server,
    shutdown,
    ready_rx,
  } = build_server_handle(server_args)?;
  let join = tokio::spawn(server.start());
  ready_rx.await?;
  let ping_endpoint = format!("http://{}:{}/ping", host, port);
  let response = reqwest::get(&ping_endpoint).await?.text().await?;
  assert_eq!(response, "pong");
  shutdown.send(()).unwrap();
  let server_result = join.await?;
  assert!(server_result.is_ok());
  Ok(())
}
