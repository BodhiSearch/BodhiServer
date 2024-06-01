use crate::{
  create::CreateCommandBuilder,
  objs::{
    Alias, AliasBuilder, ChatTemplate, ChatTemplateId, GptContextParams, LocalModelFile,
    LocalModelFileBuilder, OAIRequestParams, RemoteModel, REFS_MAIN, TOKENIZER_CONFIG_JSON,
  },
  server::BODHI_HOME,
  service::{
    AppService, AppServiceFn, DataService, HfHubService, HubService, LocalDataService,
    MockDataService, MockHubService,
  },
  CreateCommand, Repo, SharedContextRw, SharedContextRwFn,
};
use async_openai::types::CreateChatCompletionRequest;
use axum::{
  body::Body,
  http::{request::Builder, Request},
  response::Response,
};
use derive_new::new;
use dircpy::CopyBuilder;
use futures_util::Future;
use http_body_util::BodyExt;
use llama_server_bindings::Callback;
use llama_server_bindings::{bindings::llama_server_disable_logging, disable_llama_log, GptParams};
use reqwest::header::CONTENT_TYPE;
use rstest::fixture;
use serde::{de::DeserializeOwned, Deserialize};
use std::{
  env, fs,
  path::{Path, PathBuf},
};
use std::{
  ffi::{c_char, c_void},
  io::Cursor,
  slice,
};
use tempfile::{tempdir, TempDir};
use tokio::sync::mpsc::Sender;
use tracing_subscriber::{fmt, EnvFilter};

pub static TEST_REPO: &str = "meta-llama/Meta-Llama-3-8B";
pub struct ConfigDirs(pub TempDir, pub PathBuf, pub &'static str);
pub static SNAPSHOT: &str = "5007652f7a641fe7170e0bad4f63839419bd9213";

#[fixture]
pub fn config_dirs(bodhi_home: TempDir) -> ConfigDirs {
  let repo_dir = TEST_REPO.replace('/', "--");
  let repo_dir = format!("configs--{repo_dir}");
  let repo_dir = bodhi_home.path().join(repo_dir);
  fs::create_dir_all(repo_dir.clone()).unwrap();
  ConfigDirs(bodhi_home, repo_dir, TEST_REPO)
}

#[fixture]
pub fn bodhi_home() -> TempDir {
  let bodhi_home = tempfile::Builder::new()
    .prefix("bodhi_home")
    .tempdir()
    .unwrap();
  env::set_var(BODHI_HOME, format!("{}", bodhi_home.path().display()));
  bodhi_home
}

pub trait ResponseTestExt {
  async fn json<T>(self) -> anyhow::Result<T>
  where
    T: DeserializeOwned;

  async fn json_obj<T>(self) -> anyhow::Result<T>
  where
    T: for<'a> Deserialize<'a>;

  async fn text(self) -> anyhow::Result<String>;

  async fn sse<T>(self) -> anyhow::Result<Vec<T>>
  where
    T: DeserializeOwned;
}

impl ResponseTestExt for Response {
  async fn json<T>(self) -> anyhow::Result<T>
  where
    T: DeserializeOwned,
  {
    let bytes = self.into_body().collect().await.unwrap().to_bytes();
    let str = String::from_utf8_lossy(&bytes);
    let reader = Cursor::new(str.into_owned());
    let result = serde_json::from_reader::<_, T>(reader)?;
    Ok(result)
  }

  async fn json_obj<T>(self) -> anyhow::Result<T>
  where
    T: for<'de> Deserialize<'de>,
  {
    let bytes = self.into_body().collect().await.unwrap().to_bytes();
    let str = String::from_utf8_lossy(&bytes).into_owned();
    let result = serde_json::from_str(&str)?;
    Ok(result)
  }

  async fn text(self) -> anyhow::Result<String> {
    let bytes = self.into_body().collect().await.unwrap().to_bytes();
    let str = String::from_utf8_lossy(&bytes);
    Ok(str.into_owned())
  }

  async fn sse<T>(self) -> anyhow::Result<Vec<T>>
  where
    T: DeserializeOwned,
  {
    let text = self.text().await?;
    let lines = text.lines().peekable();
    let mut result = Vec::<T>::new();
    for line in lines {
      if line.is_empty() {
        continue;
      }
      let (_, value) = line.split_once(':').unwrap();
      let value = value.trim();
      let value = serde_json::from_reader::<_, T>(Cursor::new(value.to_owned()))?;
      result.push(value);
    }
    Ok(result)
  }
}

pub trait RequestTestExt {
  fn json<T: serde::Serialize>(self, value: T) -> Result<Request<Body>, anyhow::Error>;
}

impl RequestTestExt for Builder {
  fn json<T: serde::Serialize>(
    self,
    value: T,
  ) -> std::result::Result<Request<Body>, anyhow::Error> {
    let this = self.header(CONTENT_TYPE, "application/json");
    let content = serde_json::to_string(&value)?;
    let result = this.body(Body::from(content))?;
    Ok(result)
  }
}

pub(crate) fn init_test_tracing() {
  let filter = EnvFilter::from_default_env(); // Use RUST_LOG environment variable
  let subscriber = fmt::Subscriber::builder()
    .with_env_filter(filter) // Set the filter to use the RUST_LOG environment variable
    .finish();
  let _ = tracing::subscriber::set_global_default(subscriber);
}

pub(crate) fn disable_test_logging() {
  disable_llama_log();
  unsafe {
    llama_server_disable_logging();
  }
}

pub unsafe extern "C" fn test_callback(
  contents: *const c_char,
  size: usize,
  userdata: *mut c_void,
) -> usize {
  let slice = unsafe { slice::from_raw_parts(contents as *const u8, size) };
  let input_str = match std::str::from_utf8(slice) {
    Ok(s) => s,
    Err(_) => return 0,
  };
  let user_data_str = unsafe { &mut *(userdata as *mut String) };
  user_data_str.push_str(input_str);
  size
}

pub unsafe extern "C" fn test_callback_stream(
  contents: *const c_char,
  size: usize,
  userdata: *mut c_void,
) -> usize {
  let slice = unsafe { slice::from_raw_parts(contents as *const u8, size) };
  let input_str = match std::str::from_utf8(slice) {
    Ok(s) => s,
    Err(_) => return 0,
  }
  .to_owned();
  let sender = unsafe { &mut *(userdata as *mut Sender<String>) }.clone();
  // TODO: handle closed receiver
  tokio::spawn(async move { sender.send(input_str).await.unwrap() });
  size
}

#[fixture]
pub(crate) fn hf_test_token_allowed() -> Option<String> {
  dotenv::from_filename(".env.test").ok().unwrap();
  Some(std::env::var("HF_TEST_TOKEN_ALLOWED").unwrap())
}

pub(crate) fn hf_test_token_public() -> Option<String> {
  dotenv::from_filename(".env.test").ok().unwrap();
  Some(std::env::var("HF_TEST_TOKEN_PUBLIC").unwrap())
}

#[fixture]
pub(crate) fn temp_hf_home() -> TempDir {
  let temp_dir = tempdir().expect("Failed to create a temporary directory");
  let dst_path = temp_dir.path().join("huggingface");
  copy_test_dir("tests/data/huggingface", &dst_path);
  temp_dir
}

#[fixture]
pub(crate) fn hf_cache(temp_hf_home: TempDir) -> (TempDir, PathBuf) {
  let hf_cache = temp_hf_home
    .path()
    .to_path_buf()
    .join("huggingface")
    .join("hub");
  (temp_hf_home, hf_cache)
}

#[fixture]
pub(crate) fn temp_bodhi_home() -> TempDir {
  let temp_dir = tempdir().expect("Failed to create a temporary directory");
  let dst_path = temp_dir.path().join("bodhi");
  copy_test_dir("tests/data/bodhi", &dst_path);
  temp_dir
}

fn copy_test_dir(src: &str, dst_path: &Path) {
  let src_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(src);
  CopyBuilder::new(src_path, dst_path)
    .overwrite(true)
    .run()
    .unwrap();
}

pub struct HubServiceTuple(pub TempDir, pub PathBuf, pub HfHubService);

#[fixture]
pub fn hub_service(temp_hf_home: TempDir) -> HubServiceTuple {
  let hf_cache = temp_hf_home.path().join("huggingface/hub");
  let hub_service = HfHubService::new(hf_cache.clone(), false, None);
  HubServiceTuple(temp_hf_home, hf_cache, hub_service)
}

pub struct DataServiceTuple(pub TempDir, pub PathBuf, pub LocalDataService);

#[fixture]
pub fn data_service(temp_bodhi_home: TempDir) -> DataServiceTuple {
  let bodhi_home = temp_bodhi_home.path().join("bodhi");
  let data_service = LocalDataService::new(bodhi_home.clone());
  DataServiceTuple(temp_bodhi_home, bodhi_home, data_service)
}

pub struct AppServiceTuple(
  pub TempDir,
  pub TempDir,
  pub PathBuf,
  pub PathBuf,
  pub AppService,
);

#[fixture]
pub fn app_service_stub(
  hub_service: HubServiceTuple,
  data_service: DataServiceTuple,
) -> AppServiceTuple {
  let DataServiceTuple(temp_bodhi_home, bodhi_home, data_service) = data_service;
  let HubServiceTuple(temp_hf_home, hf_cache, hub_service) = hub_service;
  let service = AppService::new(hub_service, data_service);
  AppServiceTuple(temp_bodhi_home, temp_hf_home, bodhi_home, hf_cache, service)
}

#[derive(Debug, new)]
pub struct MockAppServiceFn {
  pub hub_service: MockHubService,
  pub data_service: MockDataService,
}

impl HubService for MockAppServiceFn {
  fn download(
    &self,
    repo: &Repo,
    filename: &str,
    force: bool,
  ) -> crate::service::Result<LocalModelFile> {
    self.hub_service.download(repo, filename, force)
  }

  fn list_local_models(&self) -> Vec<LocalModelFile> {
    self.hub_service.list_local_models()
  }

  fn find_local_file(
    &self,
    repo: &Repo,
    filename: &str,
    snapshot: &str,
  ) -> crate::service::Result<Option<LocalModelFile>> {
    self.hub_service.find_local_file(repo, filename, snapshot)
  }

  fn hf_home(&self) -> PathBuf {
    self.hub_service.hf_home()
  }

  fn model_file_path(&self, repo: &Repo, filename: &str, snapshot: &str) -> PathBuf {
    self.hub_service.model_file_path(repo, filename, snapshot)
  }
}

impl DataService for MockAppServiceFn {
  fn list_aliases(&self) -> crate::service::Result<Vec<Alias>> {
    self.data_service.list_aliases()
  }

  fn find_remote_model(&self, alias: &str) -> crate::service::Result<Option<RemoteModel>> {
    self.data_service.find_remote_model(alias)
  }

  fn save_alias(&self, alias: Alias) -> crate::service::Result<PathBuf> {
    self.data_service.save_alias(alias)
  }

  fn find_alias(&self, alias: &str) -> Option<Alias> {
    self.data_service.find_alias(alias)
  }

  fn list_remote_models(&self) -> crate::service::Result<Vec<RemoteModel>> {
    self.data_service.list_remote_models()
  }
}

// Implement AppServiceFn for the combined struct
impl AppServiceFn for MockAppServiceFn {}

mockall::mock! {
  pub AppService {}

  impl std::fmt::Debug for AppService {
    fn fmt<'a>(&self, f: &mut std::fmt::Formatter<'a>) -> std::fmt::Result;
  }

  unsafe impl Send for AppService { }

  unsafe impl Sync for AppService { }

  impl HubService for AppService {
    fn download(&self, repo: &Repo, filename: &str, force: bool) -> crate::service::Result<LocalModelFile>;

    fn list_local_models(&self) -> Vec<LocalModelFile>;

    fn find_local_file(
      &self,
      repo: &Repo,
      filename: &str,
      snapshot: &str,
    ) -> crate::service::Result<Option<LocalModelFile>>;

    fn hf_home(&self) -> PathBuf;

    fn model_file_path(&self, repo: &Repo, filename: &str, snapshot: &str) -> PathBuf;
  }

  impl DataService for AppService {
    fn list_aliases(&self) -> crate::service::Result<Vec<Alias>>;

    fn save_alias(&self, alias: Alias) -> crate::service::Result<PathBuf>;

    fn find_alias(&self, alias: &str) -> Option<Alias>;

    fn list_remote_models(&self) -> crate::service::Result<Vec<RemoteModel>>;

    fn find_remote_model(&self, alias: &str) -> crate::service::Result<Option<RemoteModel>>;
  }

  impl AppServiceFn for AppService { }
}

impl Default for ChatTemplate {
  fn default() -> Self {
    ChatTemplate::Id(ChatTemplateId::Llama3)
  }
}

impl LocalModelFile {
  pub fn never_download() -> LocalModelFile {
    LocalModelFile::never_download_builder().build().unwrap()
  }

  pub fn never_download_builder() -> LocalModelFileBuilder {
    LocalModelFileBuilder::default()
      .hf_cache(PathBuf::from("/tmp/ignored/huggingface/hub"))
      .repo(Repo::try_new("MyFactory/testalias-neverdownload-gguf".to_string()).unwrap())
      .filename("testalias-neverdownload.Q8_0.gguf".to_string())
      .snapshot(SNAPSHOT.to_string())
      .size(Some(22))
      .to_owned()
  }

  pub fn never_download_tokenizer_builder() -> LocalModelFileBuilder {
    LocalModelFileBuilder::default()
      .hf_cache(PathBuf::from("/tmp/ignored/huggingface/hub"))
      .repo(Repo::try_new("MyFactory/testalias-neverdownload-gguf".to_string()).unwrap())
      .filename(TOKENIZER_CONFIG_JSON.to_string())
      .snapshot(SNAPSHOT.to_string())
      .size(Some(22))
      .to_owned()
  }

  pub fn testalias() -> LocalModelFile {
    LocalModelFile::new(
      PathBuf::from("/tmp/ignored/huggingface/hub"),
      Repo::try_new("MyFactory/testalias-gguf".to_string()).unwrap(),
      "testalias.Q8_0.gguf".to_string(),
      SNAPSHOT.to_string(),
      Some(22),
    )
  }

  pub fn testalias_tokenizer() -> LocalModelFile {
    LocalModelFile::new(
      PathBuf::from("/tmp/ignored/huggingface/hub"),
      Repo::try_new("MyFactory/testalias-gguf".to_string()).unwrap(),
      "tokenizer_config.json".to_string(),
      SNAPSHOT.to_string(),
      Some(22),
    )
  }

  pub fn llama3_tokenizer() -> LocalModelFile {
    LocalModelFile::new(
      PathBuf::from("/tmp/ignored/huggingface/hub"),
      Repo::llama3(),
      TOKENIZER_CONFIG_JSON.to_string(),
      SNAPSHOT.to_string(),
      Some(33),
    )
  }
}

impl RemoteModel {
  pub fn llama3() -> RemoteModel {
    RemoteModel::new(
      "llama3:instruct".to_string(),
      "llama3".to_string(),
      Repo::try_new("QuantFactory/Meta-Llama-3-8B-Instruct-GGUF".to_string()).unwrap(),
      "Meta-Llama-3-8B-Instruct.Q8_0.gguf".to_string(),
      vec!["chat".to_string()],
      ChatTemplate::Id(ChatTemplateId::Llama3),
      OAIRequestParams::default(),
      GptContextParams::default(),
    )
  }

  pub fn test_alias() -> RemoteModel {
    RemoteModel::new(
      "testalias:instruct".to_string(),
      "testalias".to_string(),
      Repo::try_new("MyFactory/testalias-gguf".to_string()).unwrap(),
      "testalias.Q8_0.gguf".to_string(),
      vec!["chat".to_string()],
      ChatTemplate::Id(ChatTemplateId::Llama3),
      OAIRequestParams::default(),
      GptContextParams::default(),
    )
  }

  pub fn never_download() -> RemoteModel {
    RemoteModel::new(
      String::from("testalias-neverdownload:instruct"),
      String::from("testalias"),
      Repo::try_new(String::from("MyFactory/testalias-neverdownload-gguf")).unwrap(),
      String::from("testalias-neverdownload.Q8_0.gguf"),
      vec![String::from("chat")],
      ChatTemplate::Id(ChatTemplateId::Llama3),
      OAIRequestParams::default(),
      GptContextParams::default(),
    )
  }
}

impl CreateCommand {
  pub fn testalias() -> CreateCommand {
    CreateCommand::testalias_builder().build().unwrap()
  }

  pub fn testalias_builder() -> CreateCommandBuilder {
    CreateCommandBuilder::default()
      .alias("testalias:instruct".to_string())
      .repo(Repo::try_new("MyFactory/testalias-gguf".to_string()).unwrap())
      .filename("testalias.Q8_0.gguf".to_string())
      .chat_template(ChatTemplate::Id(ChatTemplateId::Llama3))
      .family(Some("testalias".to_string()))
      .force(false)
      .oai_request_params(OAIRequestParams::default())
      .context_params(GptContextParams::default())
      .to_owned()
  }
}

impl Alias {
  pub fn test_alias() -> Alias {
    Alias::test_alias_instruct_builder().build().unwrap()
  }

  pub fn test_alias_instruct_builder() -> AliasBuilder {
    AliasBuilder::default()
      .alias("testalias:instruct".to_string())
      .family(Some("testalias".to_string()))
      .repo(Repo::try_new("MyFactory/testalias-gguf".to_string()).unwrap())
      .filename("testalias.Q8_0.gguf".to_string())
      .snapshot(SNAPSHOT.to_string())
      .features(vec!["chat".to_string()])
      .chat_template(ChatTemplate::Id(ChatTemplateId::Llama3))
      .request_params(OAIRequestParams::default())
      .context_params(GptContextParams::default())
      .to_owned()
  }

  pub fn never_download() -> Alias {
    Alias::new(
      "testalias-neverdownload:instruct".to_string(),
      Some("testalias".to_string()),
      Repo::try_new("MyFactory/testalias-neverdownload-gguf".to_string()).unwrap(),
      "testalias-neverdownload.Q8_0.gguf".to_string(),
      SNAPSHOT.to_string(),
      vec!["chat".to_string()],
      ChatTemplate::Id(ChatTemplateId::Llama3),
      OAIRequestParams::default(),
      GptContextParams::default(),
    )
  }

  pub fn test_alias_exists() -> Alias {
    Alias::new(
      String::from("testalias-exists:instruct"),
      Some(String::from("testalias")),
      Repo::try_new(String::from("MyFactory/testalias-exists-instruct-gguf")).unwrap(),
      String::from("testalias-exists-instruct.Q8_0.gguf"),
      SNAPSHOT.to_string(),
      vec![String::from("chat")],
      ChatTemplate::Id(ChatTemplateId::Llama3),
      OAIRequestParams::default(),
      GptContextParams::default(),
    )
  }

  pub fn llama3() -> Alias {
    Alias::new(
      String::from("llama3:instruct"),
      Some(String::from("llama3")),
      Repo::try_new(String::from("QuantFactory/Meta-Llama-3-8B-Instruct-GGUF")).unwrap(),
      String::from("Meta-Llama-3-8B-Instruct.Q8_0.gguf"),
      SNAPSHOT.to_string(),
      vec![String::from("chat")],
      ChatTemplate::Id(ChatTemplateId::Llama3),
      OAIRequestParams::default(),
      GptContextParams::default(),
    )
  }

  pub fn tinyllama() -> Alias {
    Alias::new(
      "tinyllama:instruct".to_string(),
      None,
      Repo::try_new("TheBloke/TinyLlama-1.1B-Chat-v0.3-GGUF".to_string()).unwrap(),
      "tinyllama-1.1b-chat-v0.3.Q2_K.gguf".to_string(),
      "b32046744d93031a26c8e925de2c8932c305f7b9".to_string(),
      vec!["chat".to_string()],
      ChatTemplate::Repo(Repo::try_new("TinyLlama/TinyLlama-1.1B-Chat-v1.0".to_string()).unwrap()),
      OAIRequestParams::default(),
      GptContextParams::default(),
    )
  }
}

#[fixture]
pub fn tinyllama() -> Alias {
  Alias::tinyllama()
}

#[fixture]
pub fn shared_context_rw(tinyllama: Alias) -> SharedContextRw {
  todo!()
}

mockall::mock! {
  pub SharedContext {}

  impl Clone for SharedContext {
    fn clone(&self) -> Self;
  }

  impl std::fmt::Debug for SharedContext {
    fn fmt<'a>(&self, f: &mut std::fmt::Formatter<'a>) -> std::fmt::Result;
  }

  unsafe impl Sync for SharedContext {}

  unsafe impl Send for SharedContext {}

  #[async_trait::async_trait]
  impl SharedContextRwFn for SharedContext {
    async fn reload(&self, gpt_params: Option<GptParams>) -> crate::shared_rw::Result<()>;

    async fn try_stop(&self) -> crate::shared_rw::Result<()>;

    async fn has_model(&self) -> bool;

    async fn get_gpt_params(&self) -> crate::shared_rw::Result<Option<GptParams>>;

    async fn chat_completions(
      &self,
      request: CreateChatCompletionRequest,
      alias: Alias,
      model_file: LocalModelFile,
      tokenizer_file: LocalModelFile,
      callback: Option<Callback>,
      userdata: &String,
    ) -> crate::shared_rw::Result<()>;
  }
}

impl Repo {
  pub fn llama3() -> Repo {
    Repo::try_new("meta-llama/Meta-Llama-3-8B-Instruct".to_string()).unwrap()
  }
}
