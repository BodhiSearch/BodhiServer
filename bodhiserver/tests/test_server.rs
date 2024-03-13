use bodhiserver::{build_server_handle, ServerArgs, ServerHandle};
use rstest::{fixture, rstest};
use std::path::PathBuf;
use tempdir::TempDir;

#[fixture]
fn tmp_dir() -> PathBuf {
  TempDir::new("test_dir").unwrap().into_path()
}

#[fixture]
fn empty_model(tmp_dir: PathBuf) -> PathBuf {
  let empty_gguf = include_bytes!("data/empty.gguf");
  let model_path = tmp_dir.join("empty.gguf");
  std::fs::write(&model_path, empty_gguf).unwrap();
  model_path
}

#[rstest]
#[tokio::test]
pub async fn test_build_server_ping(empty_model: PathBuf) -> anyhow::Result<()> {
  let host = String::from("127.0.0.1");
  let port = rand::random::<u16>();
  let server_args = ServerArgs {
    host,
    port,
    model: empty_model,
    lazy_load_model: true,
  };
  let ServerHandle {
    server,
    shutdown,
    ready_rx,
  } = build_server_handle(server_args.clone())?;
  let join = tokio::spawn(server.start());
  ready_rx.await?;
  let response = reqwest::get(format!(
    "http://{}:{}/ping",
    server_args.host, server_args.port
  ))
  .await?
  .text()
  .await?;
  assert_eq!(response, "pong");
  shutdown.send(()).unwrap();
  let server_result = join.await?;
  assert!(server_result.is_ok());
  let response = reqwest::get(format!(
    "http://{}:{}/ping",
    server_args.host, server_args.port
  ))
  .await;
  assert!(response.is_err());
  assert!(reqwest::Error::is_connect(&response.unwrap_err()));
  Ok(())
}
