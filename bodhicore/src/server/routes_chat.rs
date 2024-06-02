use super::RouterStateFn;
use crate::oai::OpenAIApiError;
use async_openai::types::CreateChatCompletionRequest;
use axum::{
  body::Body,
  extract::State,
  http::{header, HeaderValue, StatusCode},
  response::{sse::Event, IntoResponse, Response, Sse},
  Json,
};
use futures_util::StreamExt;
use std::{convert::Infallible, sync::Arc};
use tokio_stream::wrappers::ReceiverStream;

pub(crate) async fn chat_completions_handler(
  State(state): State<Arc<dyn RouterStateFn>>,
  Json(request): Json<CreateChatCompletionRequest>,
) -> Result<Response, OpenAIApiError> {
  let stream = request.stream.unwrap_or(false);
  let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(100);
  let handle = tokio::spawn(async move { state.chat_completions(request, tx).await });
  if !stream {
    if let Some(message) = rx.recv().await {
      drop(rx);
      _ = handle.await;
      let response = Response::builder()
        .status(StatusCode::OK)
        .header(
          header::CONTENT_TYPE,
          HeaderValue::from_static(mime::APPLICATION_JSON.as_ref()),
        )
        .body(Body::from(message))
        .map_err(|err| OpenAIApiError::InternalServer(err.to_string()))?;
      Ok(response)
    } else {
      Err(OpenAIApiError::InternalServer(
        "receiver stream abruptly closed".to_string(),
      ))
    }
  } else {
    let stream = ReceiverStream::new(rx).map::<Result<Event, Infallible>, _>(move |msg| {
      let data = if msg.starts_with("data: ") {
        msg
          .strip_prefix("data: ")
          .unwrap()
          .strip_suffix("\n\n")
          .unwrap()
      } else if msg.starts_with("error: ") {
        msg
          .strip_prefix("error: ")
          .unwrap()
          .strip_suffix("\n\n")
          .unwrap()
      } else {
        tracing::error!(msg, "unknown event type raised from bodhi_server");
        &msg
      };
      Ok(Event::default().data(data))
    });
    Ok(Sse::new(stream).into_response())
  }
}

#[cfg(test)]
mod test {
  use crate::{
    server::routes_chat::chat_completions_handler,
    test_utils::{MockRouterState, RequestTestExt, ResponseTestExt},
  };
  use anyhow_trace::anyhow_trace;
  use async_openai::types::{
    ChatCompletionRequestMessage, ChatCompletionRequestUserMessageArgs,
    CreateChatCompletionRequestArgs, CreateChatCompletionResponse,
    CreateChatCompletionStreamResponse,
  };
  use axum::{extract::Request, routing::post, Router};
  use mockall::predicate::always;
  use reqwest::StatusCode;
  use rstest::rstest;
  use serde_json::json;
  use std::sync::Arc;
  use tokio::sync::mpsc::Sender;
  use tower::ServiceExt;

  #[rstest]
  #[tokio::test]
  #[anyhow_trace]
  async fn test_routes_chat_completions_non_stream() -> anyhow::Result<()> {
    let mut router_state = MockRouterState::new();
    let request = CreateChatCompletionRequestArgs::default()
      .model("testalias:instruct")
      .messages(vec![ChatCompletionRequestMessage::User(
        ChatCompletionRequestUserMessageArgs::default()
          .content("What day comes after Monday?")
          .build()?,
      )])
      .build()?;
    router_state
      .expect_chat_completions()
      .with(always(), always())
      .return_once(|_, sender: Sender<String>| {
        let response = json! {{
          "id": "testid",
          "model": "testalias:instruct",
          "choices": [
            {
              "index": 0,
              "message": {
                "role": "assistant",
                "content": "The day that comes after Monday is Tuesday."
              },
            }],
          "created": 1704067200,
          "object": "chat.completion",
        }}
        .to_string();
        // let response: CreateChatCompletionResponse = serde_json::from_value(response).unwrap();
        // let response = serde_json::to_string(&response).unwrap();
        tokio::spawn(async move { sender.send(response).await });
        Ok(())
      });
    let app = Router::new()
      .route("/v1/chat/completions", post(chat_completions_handler))
      .with_state(Arc::new(router_state));
    let response = app
      .oneshot(Request::post("/v1/chat/completions").json(request).unwrap())
      .await
      .unwrap();
    assert_eq!(StatusCode::OK, response.status());
    let result: CreateChatCompletionResponse = response.json().await.unwrap();
    assert_eq!(
      "The day that comes after Monday is Tuesday.",
      result
        .choices
        .first()
        .unwrap()
        .message
        .content
        .as_ref()
        .unwrap()
    );
    Ok(())
  }

  #[rstest]
  #[tokio::test]
  #[anyhow_trace]
  async fn test_routes_chat_completions_stream() -> anyhow::Result<()> {
    let mut router_state = MockRouterState::new();
    let request = CreateChatCompletionRequestArgs::default()
      .model("testalias:instruct")
      .stream(true)
      .messages(vec![ChatCompletionRequestMessage::User(
        ChatCompletionRequestUserMessageArgs::default()
          .content("What day comes after Monday?")
          .build()?,
      )])
      .build()?;
    router_state
      .expect_chat_completions()
      .with(always(), always())
      .return_once(|_, sender: Sender<String>| {
        tokio::spawn(async move {
          for (i, value) in [
            " ", " After", " Monday", ",", " the", " next", " day", " is", " T", "ues", "day",
            ".",
          ]
          .into_iter()
          .enumerate()
          {
            let response = json! {{
              "id": format!("testid-{i}"),
              "model": "testalias:instruct",
              "choices": [
                {
                  "index": 0,
                  "delta": {
                    "role": "assistant",
                    "content": value,
                  },
                }],
              "created": 1704067200,
              "object": "chat.completion.chunk",
            }};
            let response: CreateChatCompletionStreamResponse =
              serde_json::from_value(response).unwrap();
            let response = serde_json::to_string(&response).unwrap();
            _ = sender.send(format!("data: {response}\n\n")).await;
          }
          let end_delta = r#"{"choices":[{"finish_reason":"stop","index":0,"delta":{}}],"created":1717317061,"id":"chatcmpl-Twf1ixroh9WzY9Pvm4IGwNF4kB4EjTp4","model":"llama2:chat","object":"chat.completion.chunk","usage":{"completion_tokens":13,"prompt_tokens":15,"total_tokens":28}}"#.to_string();
          let _ = sender.send(format!("data: {end_delta}\n\n")).await;
        });
        Ok(())
      });
    let app = Router::new()
      .route("/v1/chat/completions", post(chat_completions_handler))
      .with_state(Arc::new(router_state));
    let response = app
      .oneshot(Request::post("/v1/chat/completions").json(request).unwrap())
      .await
      .unwrap();
    assert_eq!(StatusCode::OK, response.status());
    let response: Vec<CreateChatCompletionStreamResponse> = response.sse().await.unwrap();
    let content = response.into_iter().fold(String::new(), |mut f, r| {
      let content = r
        .choices
        .first()
        .unwrap()
        .delta
        .content
        .as_deref()
        .unwrap_or_default();
      f.push_str(content);
      f
    });
    assert_eq!("  After Monday, the next day is Tuesday.", content);
    Ok(())
  }
}
