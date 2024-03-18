mod utils;
use crate::utils::{test_server, TestServerHandle};
use anyhow::{Context, Result};
use bodhiserver::{build_server_handle, ServerArgs, ServerHandle};
use rstest::rstest;
use std::path::PathBuf;
use utils::{tiny_llama, LLAMA_BACKEND_LOCK};

#[rstest]
#[tokio::test]
pub async fn test_server_ping(#[future] tiny_llama: Result<PathBuf>) -> anyhow::Result<()> {
  let _guard = LLAMA_BACKEND_LOCK.lock().await;
  let port = rand::random::<u16>();
  let host = String::from("127.0.0.1");
  let server_args = ServerArgs {
    host: host.clone(),
    port,
    model: tiny_llama.await?,
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
  shutdown
    .send(())
    .map_err(|_| anyhow::anyhow!("error sending shutdown signal"))
    .context("sending shutdown signal to server")?;
  let server_result = join.await?;
  assert!(server_result.is_ok());
  let response = reqwest::get(&ping_endpoint).await;
  assert!(response.is_err());
  assert!(reqwest::Error::is_connect(&response.unwrap_err()));
  Ok(())
}

#[rstest]
#[tokio::test]
pub async fn test_server_ping_with_model_load(
  #[future] test_server: Result<TestServerHandle>,
) -> anyhow::Result<()> {
  let TestServerHandle {
    host,
    port,
    shutdown,
    join,
  } = test_server.await.context("initializing server handle")?;
  let ping_endpoint = format!("http://{}:{}/ping", host, port);
  let response = reqwest::get(&ping_endpoint)
    .await
    .context("querying server ping endpoint")?
    .text()
    .await
    .context("parsing response")?;
  assert_eq!(response, "pong");
  shutdown
    .send(())
    .map_err(|_| anyhow::anyhow!("error sending shutdown signal"))
    .context("sending shutdown signal to server")?;
  let server_result = join.await.context("waiting for server to stop")?;
  assert!(server_result.is_ok());
  let response = reqwest::get(&ping_endpoint).await;
  assert!(response.is_err());
  assert!(reqwest::Error::is_connect(&response.unwrap_err()));
  Ok(())
}
