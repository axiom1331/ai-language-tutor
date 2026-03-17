use super::models::{Conversation, CreateMessage, Message, MessageType};
use crate::db::pool::DbPool;
use chrono::Utc;
use sqlx::types::Uuid;
use tracing::{debug, error, info};

#[derive(Clone)]
pub struct ConversationRepository {
    pool: DbPool,
}

impl ConversationRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    /// Creates a new conversation for a user
    pub async fn create_conversation(&self, user_id: Uuid) -> Result<Conversation, sqlx::Error> {
        info!("Creating new conversation for user {}", user_id);

        let conversation = sqlx::query_as::<_, Conversation>(
            r#"
            INSERT INTO conversations (user_id, started_at)
            VALUES ($1, NOW())
            RETURNING id, user_id, started_at, ended_at, created_at, updated_at
            "#,
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await?;

        info!(
            "Created conversation {} for user {}",
            conversation.id, user_id
        );
        Ok(conversation)
    }

    /// Ends a conversation by setting the ended_at timestamp
    pub async fn end_conversation(&self, conversation_id: Uuid) -> Result<(), sqlx::Error> {
        info!("Ending conversation {}", conversation_id);

        sqlx::query(
            r#"
            UPDATE conversations
            SET ended_at = NOW()
            WHERE id = $1 AND ended_at IS NULL
            "#,
        )
        .bind(conversation_id)
        .execute(&self.pool)
        .await?;

        info!("Conversation {} ended", conversation_id);
        Ok(())
    }

    /// Gets a conversation by ID
    pub async fn get_conversation(
        &self,
        conversation_id: Uuid,
    ) -> Result<Option<Conversation>, sqlx::Error> {
        debug!("Fetching conversation {}", conversation_id);

        let conversation = sqlx::query_as::<_, Conversation>(
            r#"
            SELECT id, user_id, started_at, ended_at, created_at, updated_at
            FROM conversations
            WHERE id = $1
            "#,
        )
        .bind(conversation_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(conversation)
    }

    /// Gets all conversations for a user
    pub async fn get_user_conversations(
        &self,
        user_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Conversation>, sqlx::Error> {
        debug!(
            "Fetching conversations for user {} (limit: {}, offset: {})",
            user_id, limit, offset
        );

        let conversations = sqlx::query_as::<_, Conversation>(
            r#"
            SELECT id, user_id, started_at, ended_at, created_at, updated_at
            FROM conversations
            WHERE user_id = $1
            ORDER BY started_at DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(user_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        Ok(conversations)
    }

    /// Adds a message to a conversation
    pub async fn add_message(&self, message: CreateMessage) -> Result<Message, sqlx::Error> {
        debug!(
            "Adding {:?} message to conversation {}",
            message.message_type, message.conversation_id
        );

        let msg = sqlx::query_as::<_, Message>(
            r#"
            INSERT INTO messages (conversation_id, message_type, content, audio_duration_ms)
            VALUES ($1, $2, $3, $4)
            RETURNING id, conversation_id, message_type, content, audio_duration_ms, created_at
            "#,
        )
        .bind(message.conversation_id)
        .bind(message.message_type)
        .bind(&message.content)
        .bind(message.audio_duration_ms)
        .fetch_one(&self.pool)
        .await?;

        debug!("Message {} added successfully", msg.id);
        Ok(msg)
    }

    /// Gets all messages for a conversation
    pub async fn get_conversation_messages(
        &self,
        conversation_id: Uuid,
    ) -> Result<Vec<Message>, sqlx::Error> {
        debug!("Fetching messages for conversation {}", conversation_id);

        let messages = sqlx::query_as::<_, Message>(
            r#"
            SELECT id, conversation_id, message_type, content, audio_duration_ms, created_at
            FROM messages
            WHERE conversation_id = $1
            ORDER BY created_at ASC
            "#,
        )
        .bind(conversation_id)
        .fetch_all(&self.pool)
        .await?;

        debug!(
            "Fetched {} messages for conversation {}",
            messages.len(),
            conversation_id
        );
        Ok(messages)
    }

    /// Gets messages with pagination
    pub async fn get_conversation_messages_paginated(
        &self,
        conversation_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Message>, sqlx::Error> {
        debug!(
            "Fetching messages for conversation {} (limit: {}, offset: {})",
            conversation_id, limit, offset
        );

        let messages = sqlx::query_as::<_, Message>(
            r#"
            SELECT id, conversation_id, message_type, content, audio_duration_ms, created_at
            FROM messages
            WHERE conversation_id = $1
            ORDER BY created_at ASC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(conversation_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        Ok(messages)
    }

    /// Deletes a conversation and all its messages (cascade delete)
    pub async fn delete_conversation(&self, conversation_id: Uuid) -> Result<(), sqlx::Error> {
        info!("Deleting conversation {}", conversation_id);

        sqlx::query(
            r#"
            DELETE FROM conversations
            WHERE id = $1
            "#,
        )
        .bind(conversation_id)
        .execute(&self.pool)
        .await?;

        info!("Conversation {} deleted", conversation_id);
        Ok(())
    }
}
