use bodhiserver::{build_server, ServerArgs, ServerHandle};
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
  };
  let ServerHandle { server, shutdown } = build_server(server_args.clone()).await?;
  #[allow(clippy::redundant_async_block)]
  let join = tokio::spawn(async move { server.await });
  let response = reqwest::get(format!(
    "http://{}:{}/ping",
    server_args.host, server_args.port
  ))
  .await?
  .text()
  .await?;
  assert_eq!(response, "pong");
  shutdown.send(()).unwrap();
  (join.await?)?;
  Ok(())
}
