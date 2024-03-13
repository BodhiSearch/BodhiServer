mod utils;
use anyhow::Result;
use async_openai::types::CreateChatCompletionResponse;
use rstest::rstest;
use serde_json::json;
use utils::test_server;

use crate::utils::TestServerHandle;

#[rstest]
#[tokio::test]
pub async fn test_server_chat(
  #[future] test_server: Result<TestServerHandle>,
) -> anyhow::Result<()> {
  let TestServerHandle {
    host,
    port,
    shutdown,
    join,
  } = test_server.await?;
  let chat_endpoint = format!("http://{}:{}/v1/chat/completions", host, port);
  let response = reqwest::Client::new()
    .post(&chat_endpoint)
    .header("Content-Type", "application/json")
    .json(&json!({
      "model": "tinyllama-15m-q8_0",
      "messages": [
        {
          "role": "user",
          "content": "What day comes after Monday?"
        }
      ]
    }))
    .send()
    .await?
    .json::<CreateChatCompletionResponse>()
    .await?;
  assert_eq!(response.choices.len(), 1);
  assert_eq!(
    response
      .choices
      .first()
      .unwrap()
      .message
      .content
      .as_ref()
      .unwrap(),
    "Tuesday"
  );
  shutdown.send(()).unwrap();
  assert!(join.await?.is_ok());
  Ok(())
}
