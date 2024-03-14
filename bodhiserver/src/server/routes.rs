use async_openai::types::{CreateChatCompletionRequest, CreateChatCompletionResponse};
use axum::{
  http::StatusCode,
  response::IntoResponse,
  routing::{get, post},
  Json,
};
use llama_cpp_2::model::LlamaModel;
use serde_json::json;
use tower_http::trace::TraceLayer;

// TODO: serialize error in OpenAI format
#[derive(Debug)]
enum ApiError {
  Json(serde_json::Error),
}

impl IntoResponse for ApiError {
  fn into_response(self) -> axum::response::Response {
    match self {
      ApiError::Json(e) => (
        StatusCode::INTERNAL_SERVER_ERROR,
        format!("Error while marshalling response: {e}"),
      )
        .into_response(),
    }
  }
}

type Result<T> = std::result::Result<T, ApiError>;

#[derive(Clone)]
struct RouterState {}

impl RouterState {
  fn new() -> Self {
    Self {}
  }
}

pub(super) fn build_routes(_model: Option<LlamaModel>) -> axum::Router {
  axum::Router::new()
    .route("/ping", get(|| async { "pong" }))
    .route("/v1/chat/completions", post(chat_completions_handler))
    .layer(TraceLayer::new_for_http())
    .with_state(RouterState::new())
}

async fn chat_completions_handler(
  Json(_request): Json<CreateChatCompletionRequest>,
) -> Result<Json<CreateChatCompletionResponse>> {
  let response = json!({
    "id": "chatcmpl-TESTID",
    "object": "chat.completion",
    "created": 1710320000,
    "model": "tinyllama-15m-q8_0",
    "choices": [
      {
        "index": 0,
        "message": {
          "role": "assistant",
          "content": "Tuesday"
        },
        "logprobs": null,
        "finish_reason": "stop"
      },
    ],
    "usage": {
      "prompt_tokens": 10,
      "completion_tokens": 2,
      "total_tokens": 12
    },
    "system_fingerprint": "fp_4f0b692a78"
  });
  serde_json::from_value(response)
    .map(Json)
    .map_err(ApiError::Json)
}
