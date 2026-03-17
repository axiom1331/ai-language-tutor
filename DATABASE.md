# Database Setup

This document describes the database schema and setup for the AI Language Tutor application.

## Overview

The application uses PostgreSQL to store conversation history. Each WebSocket session creates a new conversation, and all messages (both user and AI responses) are saved to the database.

## Schema

### Tables

#### `conversations`
Stores conversation sessions.

| Column | Type | Description |
|--------|------|-------------|
| id | UUID | Primary key |
| user_id | UUID | User identifier (from WebSocket client ID) |
| started_at | TIMESTAMPTZ | When the conversation started |
| ended_at | TIMESTAMPTZ | When the conversation ended (NULL if still active) |
| created_at | TIMESTAMPTZ | Record creation timestamp |
| updated_at | TIMESTAMPTZ | Record update timestamp |

#### `messages`
Stores individual messages within conversations.

| Column | Type | Description |
|--------|------|-------------|
| id | UUID | Primary key |
| conversation_id | UUID | Foreign key to conversations table |
| message_type | message_type | Either 'user' or 'ai_tutor' |
| content | TEXT | The message content (transcription or reply) |
| audio_duration_ms | INTEGER | Optional duration of audio in milliseconds |
| created_at | TIMESTAMPTZ | When the message was created |

### Indexes

- `idx_conversations_user_id` - Index on user_id for fast user conversation lookups
- `idx_conversations_started_at` - Index on started_at for chronological queries
- `idx_messages_conversation_id` - Index on conversation_id for fast message retrieval
- `idx_messages_created_at` - Index on created_at for chronological ordering

## Setup Instructions

### 1. Start PostgreSQL

Using Docker Compose:

```bash
docker-compose up -d
```

This will start PostgreSQL on `localhost:5432` with:
- Database: `ai_language_tutor`
- Username: `postgres`
- Password: `postgres`

### 2. Configure Environment

Add to your `.env` file:

```
DATABASE_URL=postgres://postgres:postgres@localhost:5432/ai_language_tutor
```

### 3. Run Migrations

Migrations are automatically run when the application starts. They are located in the `migrations/` directory.

To manually run migrations:

```bash
cargo install sqlx-cli --no-default-features --features postgres
sqlx migrate run
```

## Usage

### Conversation Flow

1. **WebSocket Connection**: When a client connects via WebSocket, a new conversation is created
2. **User Message**: When audio is transcribed, it's saved as a 'user' message
3. **AI Response**: When the AI generates a reply, it's saved as an 'ai_tutor' message
4. **End Conversation**: When the WebSocket disconnects, the conversation is marked as ended

### Database Operations

The `ConversationRepository` provides the following methods:

- `create_conversation(user_id)` - Create a new conversation
- `end_conversation(conversation_id)` - Mark a conversation as ended
- `get_conversation(conversation_id)` - Retrieve a conversation by ID
- `get_user_conversations(user_id, limit, offset)` - Get all conversations for a user
- `add_message(message)` - Add a message to a conversation
- `get_conversation_messages(conversation_id)` - Get all messages in a conversation
- `get_conversation_messages_paginated(conversation_id, limit, offset)` - Get messages with pagination
- `delete_conversation(conversation_id)` - Delete a conversation and all its messages

## Best Practices

1. **Connection Pooling**: The application uses `sqlx` connection pooling with:
   - Min connections: 5
   - Max connections: 20
   - Acquire timeout: 10 seconds
   - Idle timeout: 5 minutes
   - Max lifetime: 30 minutes

2. **Automatic Timestamps**: The `updated_at` field is automatically updated via database trigger

3. **Cascade Delete**: Deleting a conversation automatically deletes all associated messages

4. **UUID Primary Keys**: All tables use UUID primary keys for global uniqueness

## Migration Files

- `001_init.sql` - Initial schema creation with conversations, messages, indexes, and triggers
