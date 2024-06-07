use super::objs::{Conversation, Message};
use chrono::{DateTime, Timelike, Utc};
use derive_new::new;
use sqlx::SqlitePool;
use std::sync::Arc;

pub trait TimeServiceFn: std::fmt::Debug + Send + Sync {
  fn utc_now(&self) -> DateTime<Utc>;
}

#[derive(Debug, Clone, Default)]
pub struct TimeService;

impl TimeServiceFn for TimeService {
  fn utc_now(&self) -> DateTime<Utc> {
    let now = chrono::Utc::now();
    now.with_nanosecond(0).unwrap_or(now)
  }
}

#[derive(Debug, thiserror::Error)]
pub enum DbError {
  #[error(transparent)]
  Sqlx(#[from] sqlx::Error),
}

#[async_trait::async_trait]
pub trait DbServiceFn {
  async fn save_conversation(&self, conversation: &mut Conversation) -> Result<(), DbError>;

  async fn save_message(&self, message: &mut Message) -> Result<(), DbError>;

  async fn list_conversations(&self) -> Result<Vec<Conversation>, DbError>;

  async fn delete_conversations(&self, id: &str) -> Result<(), DbError>;

  async fn delete_all_conversations(&self) -> Result<(), DbError>;

  async fn get_conversation_with_messages(&self, id: &str) -> Result<Conversation, DbError>;
}

#[derive(Debug, Clone, new)]
pub struct DbService {
  pool: SqlitePool,
  time_service: Arc<dyn TimeServiceFn>,
}

#[async_trait::async_trait]
impl DbServiceFn for DbService {
  async fn save_conversation(&self, conversation: &mut Conversation) -> Result<(), DbError> {
    conversation.updated_at = self.time_service.utc_now();
    sqlx::query(
      "INSERT INTO conversations
        (
          id,
          title,
          created_at,
          updated_at
        )
        VALUES (?, ?, ?, ?)
        ON CONFLICT(id) DO UPDATE SET title = ?, updated_at = ?",
    )
    .bind(&conversation.id)
    .bind(&conversation.title)
    .bind(conversation.created_at.timestamp())
    .bind(conversation.updated_at.timestamp())
    .bind(&conversation.title)
    .bind(conversation.updated_at.timestamp())
    .execute(&self.pool)
    .await?;
    Ok(())
  }

  async fn save_message(&self, message: &mut Message) -> Result<(), DbError> {
    message.updated_at = self.time_service.utc_now();
    sqlx::query(
      "INSERT INTO messages
        (
          id,
          conversation_id,
          role,
          name,
          content,
          created_at,
          updated_at
        )
        VALUES (?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(id) DO UPDATE SET role = ?, name = ?, content = ?, updated_at = ?",
    )
    .bind(&message.id)
    .bind(&message.conversation_id)
    .bind(&message.role)
    .bind(&message.name)
    .bind(&message.content)
    .bind(message.created_at.timestamp())
    .bind(message.updated_at.timestamp())
    .bind(&message.role)
    .bind(&message.name)
    .bind(&message.content)
    .bind(message.updated_at.timestamp())
    .execute(&self.pool)
    .await?;
    Ok(())
  }

  async fn list_conversations(&self) -> Result<Vec<Conversation>, DbError> {
    let conversations = sqlx::query_as::<_, (String, String, i64, i64)>(
      "SELECT id, title, created_at, updated_at FROM conversations ORDER BY created_at DESC",
    )
    .fetch_all(&self.pool)
    .await?;

    let mut result = Vec::new();
    for (id, title, created_at, updated_at) in conversations {
      result.push(Conversation {
        id,
        title,
        created_at: chrono::DateTime::<Utc>::from_timestamp(created_at, 0).unwrap_or_default(),
        updated_at: chrono::DateTime::<Utc>::from_timestamp(updated_at, 0).unwrap_or_default(),
        messages: Vec::new(),
      });
    }

    Ok(result)
  }

  async fn get_conversation_with_messages(&self, id: &str) -> Result<Conversation, DbError> {
    let messages = sqlx::query_as::<_, Message>(
      "SELECT id, conversation_id, role, name, content, created_at, updated_at FROM messages WHERE conversation_id = ?"
    )
    .bind(&id)
    .fetch_all(&self.pool)
    .await?;

    let row = sqlx::query_as::<_, (String, String, i64, i64)>(
      "SELECT id, title, created_at, updated_at FROM conversations WHERE id = ?",
    )
    .bind(&id)
    .fetch_one(&self.pool)
    .await?;

    let conversation = Conversation {
      id: row.0.clone(),
      title: row.1,
      created_at: chrono::DateTime::<Utc>::from_timestamp(row.2, 0).unwrap_or_default(),
      updated_at: chrono::DateTime::<Utc>::from_timestamp(row.3, 0).unwrap_or_default(),
      messages,
    };

    Ok(conversation)
  }

  async fn delete_conversations(&self, id: &str) -> Result<(), DbError> {
    sqlx::query("DELETE FROM messages where conversation_id=?")
      .bind(id)
      .execute(&self.pool)
      .await?;
    sqlx::query("DELETE FROM conversations where id=?")
      .bind(id)
      .execute(&self.pool)
      .await?;
    Ok(())
  }

  async fn delete_all_conversations(&self) -> Result<(), DbError> {
    sqlx::query("DELETE FROM messages")
      .execute(&self.pool)
      .await?;
    sqlx::query("DELETE FROM conversations")
      .execute(&self.pool)
      .await?;
    Ok(())
  }
}

#[cfg(test)]
mod test {
  use super::DbService;
  use crate::{
    db::{
      objs::{ConversationBuilder, MessageBuilder},
      service::DbServiceFn,
    },
    test_utils::db_service,
  };
  use chrono::{DateTime, Days, Timelike, Utc};
  use rstest::rstest;
  use tempfile::TempDir;
  use uuid::Uuid;

  #[rstest]
  #[awt]
  #[tokio::test]
  async fn test_db_service_conversations_create(
    #[future] db_service: (TempDir, DateTime<Utc>, DbService),
  ) -> anyhow::Result<()> {
    let (_tempdir, now, service) = db_service;
    let created = chrono::Utc::now()
      .checked_sub_days(Days::new(1))
      .unwrap()
      .with_nanosecond(0)
      .unwrap();
    let mut conversation = ConversationBuilder::default()
      .id(Uuid::new_v4().to_string())
      .title("test chat")
      .created_at(created)
      .updated_at(created)
      .build()?;
    service.save_conversation(&mut conversation.clone()).await?;
    let convos = service.list_conversations().await?;
    assert_eq!(1, convos.len());
    conversation.updated_at = now;
    assert_eq!(&conversation, convos.first().unwrap());
    Ok(())
  }

  #[rstest]
  #[awt]
  #[tokio::test]
  async fn test_db_service_conversations_update(
    #[future] db_service: (TempDir, DateTime<Utc>, DbService),
  ) -> anyhow::Result<()> {
    let (_tempdir, _now, service) = db_service;
    let created = chrono::Utc::now()
      .checked_sub_days(Days::new(1))
      .unwrap()
      .with_nanosecond(0)
      .unwrap();
    let mut conversation = ConversationBuilder::default()
      .id(Uuid::new_v4().to_string())
      .title("test chat")
      .created_at(created)
      .updated_at(created)
      .build()?;
    service.save_conversation(&mut conversation).await?;
    conversation.title = "new test chat".to_string();
    service.save_conversation(&mut conversation).await?;

    let convos = service.list_conversations().await?;
    assert_eq!(1, convos.len());
    assert_eq!(&conversation, convos.first().unwrap());
    Ok(())
  }

  #[rstest]
  #[awt]
  #[tokio::test]
  async fn test_db_service_list_conversation(
    #[future] db_service: (TempDir, DateTime<Utc>, DbService),
  ) -> anyhow::Result<()> {
    let (_tempdir, _now, service) = db_service;
    service
      .save_conversation(&mut ConversationBuilder::default().build().unwrap())
      .await?;
    service
      .save_conversation(&mut ConversationBuilder::default().build().unwrap())
      .await?;
    let convos = service.list_conversations().await?;
    assert_eq!(2, convos.len());
    Ok(())
  }

  #[rstest]
  #[awt]
  #[tokio::test]
  async fn test_db_service_save_message(
    #[future] db_service: (TempDir, DateTime<Utc>, DbService),
  ) -> anyhow::Result<()> {
    let (_tempdir, _now, service) = db_service;
    let mut conversation = ConversationBuilder::default()
      .title("test title")
      .build()
      .unwrap();
    service.save_conversation(&mut conversation).await?;
    let mut message = MessageBuilder::default()
      .id(Uuid::new_v4().to_string())
      .conversation_id(conversation.id.clone())
      .role("user")
      .content("test message")
      .build()
      .unwrap();
    service.save_message(&mut message).await?;
    let convos = service
      .get_conversation_with_messages(&conversation.id)
      .await?;
    assert_eq!(&message, convos.messages.first().unwrap());
    Ok(())
  }

  #[rstest]
  #[awt]
  #[tokio::test]
  async fn test_db_service_delete_conversation(
    #[future] db_service: (TempDir, DateTime<Utc>, DbService),
  ) -> anyhow::Result<()> {
    let (_tempdir, _now, service) = db_service;
    let mut conversation = ConversationBuilder::default()
      .title("test title")
      .build()
      .unwrap();
    service.save_conversation(&mut conversation).await?;
    let mut message = MessageBuilder::default()
      .id(Uuid::new_v4().to_string())
      .conversation_id(conversation.id.clone())
      .role("user")
      .content("test message")
      .build()
      .unwrap();
    service.save_message(&mut message).await?;
    service.delete_conversations(&conversation.id).await?;
    let convos = service
      .get_conversation_with_messages(&conversation.id)
      .await;
    assert!(convos.is_err());
    assert_eq!(
      "no rows returned by a query that expected to return at least one row",
      convos.unwrap_err().to_string()
    );
    Ok(())
  }

  #[rstest]
  #[awt]
  #[tokio::test]
  async fn test_db_service_delete_all_conversation(
    #[future] db_service: (TempDir, DateTime<Utc>, DbService),
  ) -> anyhow::Result<()> {
    let (_tempdir, _now, service) = db_service;
    let mut conversation = ConversationBuilder::default().build().unwrap();
    service.save_conversation(&mut conversation).await?;
    let mut message = MessageBuilder::default()
      .id(Uuid::new_v4().to_string())
      .conversation_id(conversation.id.clone())
      .build()
      .unwrap();
    service.save_message(&mut message).await?;
    service.delete_all_conversations().await?;
    let convos = service.list_conversations().await?;
    assert!(convos.is_empty());
    Ok(())
  }
}
