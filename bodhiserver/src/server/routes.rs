use crate::server::bodhi_ctx::BodhiContextWrapper;
use axum::{
  http::StatusCode,
  response::IntoResponse,
  routing::{get, post},
};
use std::sync::{Arc, Mutex};
use tower_http::trace::TraceLayer;

use super::routes_chat::chat_completions_handler;

// TODO: serialize error in OpenAI format
#[derive(Debug)]
pub(crate) enum ApiError {
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

#[derive(Clone)]
pub(crate) struct RouterState {
  pub(crate) bodhi_ctx: Arc<Mutex<BodhiContextWrapper>>,
}

impl RouterState {
  fn new(bodhi_ctx: Arc<Mutex<BodhiContextWrapper>>) -> Self {
    Self { bodhi_ctx }
  }
}

pub(super) fn build_routes(bodhi_ctx: Arc<Mutex<BodhiContextWrapper>>) -> axum::Router {
  axum::Router::new()
    .route("/ping", get(|| async { "pong" }))
    .route("/v1/chat/completions", post(chat_completions_handler))
    .layer(TraceLayer::new_for_http())
    .with_state(RouterState::new(bodhi_ctx))
}
