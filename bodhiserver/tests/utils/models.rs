use anyhow::Context;
use fs2::FileExt;
use lazy_static::lazy_static;
use rstest::fixture;
use std::{
  fs::{self, File},
  io::Write,
  path::PathBuf,
};
use tokio::sync::Mutex;

lazy_static! {
  static ref ULTRATINY_LLAMA: &'static [u8] = include_bytes!("../data/tinyllama-15m-q8_0.gguf");
  static ref INIT_LOCK: Mutex<()> = Mutex::new(());
  pub static ref LLAMA_BACKEND_LOCK: Mutex<()> = Mutex::new(());
}

#[fixture]
pub async fn tiny_llama() -> anyhow::Result<PathBuf> {
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
