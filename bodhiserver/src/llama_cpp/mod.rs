use llama_cpp_2::llama_backend::LlamaBackend;

pub(crate) struct LlamaCpp {
  pub(crate) llama_backend: LlamaBackend,
}

impl LlamaCpp {
  pub(crate) fn init() -> anyhow::Result<LlamaCpp> {
    let llama_backend = LlamaBackend::init()?;
    Ok(Self { llama_backend })
  }
}
