# CLAUDE.md — AI Language Tutor Core

Rust web application providing real-time language tutoring via WebSocket. Processes audio through a 3-stage async pipeline: Speech-to-Text → LLM Reply Generation → Text-to-Speech.

---

## Directory Map

```
src/
├── main.rs              # Entry point: loads env, initialises DB pool, builds pipeline, starts server
├── error.rs             # All custom error types (AssistantError, SttError, TtsError)
├── metrics.rs           # Timing/analytics structs (AnalysisMetrics, SttMetrics, TtsMetrics, SessionMetrics)
├── pipeline.rs          # Three-stage async worker pipeline (STT → Reply → TTS) via mpsc channels
├── auth/
│   └── mod.rs           # JWT creation/verification + credential check against env vars
├── db/
│   ├── mod.rs           # Re-exports
│   ├── models.rs        # Conversation, Message, MessageType structs/enums
│   ├── pool.rs          # PgPool setup (min 5, max 20 connections)
│   └── repository.rs    # ConversationRepository — all DB queries
├── replygen/
│   ├── mod.rs           # Re-exports
│   ├── interface.rs     # ReplyGenerator trait + GenerationResponse struct
│   ├── intent.rs        # Intent enum (Conversation, GrammarQuestion, ConceptExplanation, TranslationRequest)
│   ├── intent_classifier.rs  # BedrockIntentClassifier (nova-micro model)
│   ├── bedrock_impl.rs  # BedrockReplyGenerator (nova-lite model) — main LLM integration
│   └── prompts/         # System prompt templates per intent + intent classifier prompt
│       ├── conversation_prompt.txt
│       ├── grammar_question_prompt.txt
│       ├── concept_explanation_prompt.txt
│       ├── translation_request_prompt.txt
│       └── intent_classifier_prompt.txt
├── stt/
│   ├── mod.rs
│   ├── interface.rs     # SttProvider trait + SttResult struct
│   └── cartesia_impl.rs # CartesiaSttProvider — WebSocket streaming, 30KB chunks, ink-whisper model
├── tts/
│   ├── mod.rs
│   ├── interface.rs     # TtsProvider trait + TtsResult struct
│   └── cartesia_impl.rs # CartesiaTtsProvider — HTTP POST, sonic-multilingual model, returns WAV
└── ws/
    ├── mod.rs
    ├── server.rs        # Axum router: /ws, /api/login, /health endpoints
    ├── protocol.rs      # ClientMessage, ServerMessage, TextResponse, AudioResponse, ErrorResponse
    └── handler.rs       # WebSocket connection lifecycle + per-connection state (WsState)

migrations/
└── 001_init.sql         # Creates conversations + messages tables, indexes, triggers
```

---

## Request / Response Flow

```
1. POST /api/login  {username, password}
   └─ auth::verify_credentials() → auth::create_jwt() → JWT (24h)

2. GET /ws?token={jwt}
   └─ auth::verify_jwt() → create_conversation() in DB → WS connection open

3. WebSocket ClientMessage::Reply {audio_bytes, target_language, history, request_id}
   │
   ├─ [STT Worker]   CartesiaSttProvider::transcribe()  → Transcription
   │                 → save user Message to DB
   │
   ├─ [Reply Worker] BedrockIntentClassifier (nova-micro) → Intent
   │                 BedrockReplyGenerator (nova-lite, intent-specific prompt)
   │                 → GenerationResponse {reply, original_language_reply, corrections, tip}
   │                 → save AI tutor Message to DB
   │                 → send ServerMessage::Text to client
   │
   └─ [TTS Worker]   CartesiaTtsProvider::synthesize()  → WAV bytes
                     → send ServerMessage::Audio to client
```

---

## Key Traits (Extension Points)

| Trait | File | Implementors |
|-------|------|-------------|
| `SttProvider` | `src/stt/interface.rs` | `CartesiaSttProvider` |
| `TtsProvider` | `src/tts/interface.rs` | `CartesiaTtsProvider` |
| `ReplyGenerator` | `src/replygen/interface.rs` | `BedrockReplyGenerator` |
| `IntentClassifier` | `src/replygen/intent_classifier.rs` | `BedrockIntentClassifier` |

---

## WebSocket Protocol

**Client → Server:**
```json
{
  "type": "reply",
  "audio_bytes": "<base64 PCM s16le 16kHz>",
  "target_language": "es",
  "history": [{"role": "user"|"assistant", "content": "..."}],
  "request_id": "<uuid>"
}
```

**Server → Client (text):**
```json
{
  "type": "text",  "request_id": "...",
  "transcription": "...", "reply": "...",
  "original_language_reply": "...", "corrections": "...", "tip": "..."
}
```

**Server → Client (audio):**
```json
{ "type": "audio", "request_id": "...", "audio_bytes": "<base64 WAV>", "format": "wav" }
```

**Error codes:** `invalid_message`, `transcription_failed`, `reply_generation_failed`, `tts_failed`, `internal_error`

---

## Database Schema (PostgreSQL)

```sql
conversations (id UUID PK, user_id UUID, started_at TIMESTAMPTZ, ended_at TIMESTAMPTZ, created_at, updated_at)
messages      (id UUID PK, conversation_id UUID FK→conversations CASCADE, message_type ENUM('user','ai_tutor'),
               content TEXT, audio_duration_ms INT, created_at TIMESTAMPTZ)
```

Indexes on: `conversations.user_id`, `conversations.started_at DESC`, `messages.conversation_id`, `messages.created_at DESC`.

Auto-updates `conversations.updated_at` via trigger.

**Repository methods** (`src/db/repository.rs`):
- `create_conversation`, `end_conversation`, `get_conversation`, `get_user_conversations`
- `add_message`, `get_conversation_messages`, `get_conversation_messages_paginated`, `delete_conversation`

---

## Authentication (`src/auth/mod.rs`)

- Credentials verified against `AUTH_USERNAME` / `AUTH_PASSWORD` env vars
- JWT signed with HS256, secret from `JWT_SECRET`, 24-hour expiry
- Token passed as query param on WS upgrade: `GET /ws?token={jwt}`
- Claims: `{ sub: username, exp: unix_timestamp }`

---

## Environment Variables

```env
DATABASE_URL            postgres://...
AWS_REGION              eu-central-1
AWS_ACCESS_KEY_ID / AWS_SECRET_ACCESS_KEY
CARTESIA_API_KEY
CARTESIA_STT_MODEL_ID   ink-whisper
CARTESIA_VERSION        2026-03-01
CARTESIA_VOICE_ID       5c5ad5e7-1020-476b-8b91-fdcbe9cc313c
CARTESIA_MODEL_ID       sonic-3
CARTESIA_SPEED          0.80
CARTESIA_OUTPUT_FORMAT  wav
DEFAULT_LANGUAGE        en
AUDIO_ENCODING          pcm_s16le
SAMPLE_RATE             16000
AUTH_USERNAME / AUTH_PASSWORD
JWT_SECRET
LISTEN_ADDR             0.0.0.0:8080
RUST_LOG                info
```

---

## External Services

| Service | Protocol | Where configured |
|---------|----------|-----------------|
| AWS Bedrock (nova-lite, nova-micro) | AWS SDK | `src/replygen/bedrock_impl.rs`, `intent_classifier.rs` |
| Cartesia STT (`wss://api.cartesia.ai/stt/websocket`) | WebSocket, 30KB chunks | `src/stt/cartesia_impl.rs` |
| Cartesia TTS (`https://api.cartesia.ai/tts/bytes`) | HTTP POST | `src/tts/cartesia_impl.rs` |
| PostgreSQL | sqlx connection pool | `src/db/pool.rs` |

---

## Pipeline Internals (`src/pipeline.rs`)

Three tokio tasks communicating via `mpsc` channels:

```
ws_handler
  → stt_tx (SttRequest)  → stt_task → reply_tx (ReplyRequest)
                                     → reply_task → tts_tx (TtsRequest)
                                                  → tts_task
                                                     │
                                          ←──────────┘ PipelineResponse (via response_tx)
ws_handler ← response_rx
```

`PipelineResponse` variants: `Transcription`, `Text`, `Audio`, `Error`

---

## Adding a New STT/TTS/LLM Provider

1. Implement the relevant trait (`SttProvider`, `TtsProvider`, `ReplyGenerator`)
2. Add new config struct and env-var loading in `main.rs`
3. Pass the new implementation into `Pipeline::new()` in `main.rs`
   — no other files need changing

## Adding a New Intent

1. Add variant to `Intent` enum in `src/replygen/intent.rs`
2. Add matching prompt file in `src/replygen/prompts/`
3. Update `intent_classifier_prompt.txt` to include the new intent label
4. Add match arm in `bedrock_impl.rs` to select the new prompt
