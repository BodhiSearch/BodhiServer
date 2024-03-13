use anyhow::Context;
use bodhiserver::{build_server_handle, ServerArgs, ServerHandle};
use fs2::FileExt;
use lazy_static::lazy_static;
use rstest::{fixture, rstest};
use std::{
  fs::{self, File},
  io::Write,
  path::PathBuf,
};
use tempdir::TempDir;
use tokio::sync::Mutex;

lazy_static! {
  static ref ULTRATINY_LLAMA: &'static [u8] =
    include_bytes!("data/tinyllama-15m-q8_0.gguf");
  static ref INIT_LOCK: Mutex<()> = Mutex::new(());
  static ref LLAMA_BACKEND_LOCK: Mutex<()> = Mutex::new(());
}

async fn tiny_llama() -> anyhow::Result<PathBuf> {
  let lock = INIT_LOCK.lock().await;
  let temp_dir = std::env::temp_dir().join("bodhiserver_test_data");
  let file_path = temp_dir.join("tinyllama.gguf");

  if !file_path.exists() {
    // Folder level lock if tests are run in/as separate runtimes/applications
    let lock_file_path = temp_dir.join("tinyllama.gguf.lock");
    fs::create_dir_all(&temp_dir).context("creating test temp dir")?;
    let lock_file = File::create(&lock_file_path).context("creating lock file")?;
    lock_file
      .lock_exclusive()
      .context("acquiring exclusive file lock")?;

    if !file_path.exists() {
      let mut file =
        File::create(&file_path).context(format!("creating tiny llama file {file_path:?}"))?;
      file
        .write_all(&ULTRATINY_LLAMA)
        .context(format!("dumping tiny llama content to file {file_path:?}"))?;
      drop(lock_file);
      fs::remove_file(&lock_file_path).context(format!("removing lock file {lock_file_path:?}"))?;
    }
  }
  drop(lock);
  Ok(file_path)
}

#[fixture]
fn empty_model(tmp_dir: PathBuf) -> PathBuf {
  let empty_gguf = include_bytes!("data/empty.gguf");
  let model_path = tmp_dir.join("empty.gguf");
  std::fs::write(&model_path, empty_gguf).unwrap();
  model_path
}

#[fixture]
fn tmp_dir() -> PathBuf {
  TempDir::new("test_dir").unwrap().into_path()
}

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
