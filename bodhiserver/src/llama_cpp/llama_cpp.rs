use std::path::Path;

use anyhow::Context;
use llama_cpp_2::{
  llama_backend::LlamaBackend,
  model::{params::LlamaModelParams, LlamaModel},
};

pub(crate) struct LlamaCpp {
  pub(crate) llama_backend: LlamaBackend,
}

impl LlamaCpp {
  pub(crate) fn init() -> anyhow::Result<LlamaCpp> {
    let llama_backend = LlamaBackend::init()?;
    Ok(Self { llama_backend })
  }

  pub(crate) fn load_model(&self, model_path: &Path) -> anyhow::Result<LlamaModel> {
    let params = LlamaModelParams::default();
    let llama_model = LlamaModel::load_from_file(&self.llama_backend, model_path, &params)
      .context("init_llama_model: loading model")?;
    Ok(llama_model)
  }
}
