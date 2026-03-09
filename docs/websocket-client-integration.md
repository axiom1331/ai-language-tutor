# WebSocket Client Integration Guide

## Overview

This guide explains how to integrate with the AI Language Tutor WebSocket API for real-time voice conversations.

**Endpoint:** `ws://your-server:port/ws`

## Connection

Establish a standard WebSocket connection:

```javascript
const ws = new WebSocket('ws://localhost:3000/ws');
```

## Message Format

All messages are **JSON-encoded text** sent over the WebSocket. Binary data (audio) is **base64-encoded** within JSON strings.

### Client → Server Messages

Send a `Reply` message with audio input:

```json
{
  "type": "reply",
  "audio_bytes": "<base64-encoded PCM audio>",
  "target_language": "es",
  "history": [
    {
      "role": "user",
      "content": "Hola"
    },
    {
      "role": "assistant",
      "content": "¡Hola! ¿Cómo estás?"
    }
  ],
  "request_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

**Fields:**
- `type`: Always `"reply"`
- `audio_bytes`: Base64-encoded raw PCM audio (16000 Hz, 16-bit, mono)
- `target_language`: ISO language code (e.g., `"es"`, `"fr"`, `"de"`)
- `history`: Optional conversation context (array of user/assistant messages)
- `request_id`: UUID for tracking (optional, server generates if missing)

### Server → Client Messages

You'll receive **two messages** per request:

#### 1. Text Response

```json
{
  "type": "text",
  "request_id": "550e8400-e29b-41d4-a716-446655440000",
  "transcription": "Hola",
  "reply": "¡Hola! ¿Cómo estás?",
  "original_language_reply": "Hello! How are you?",
  "corrections": "Optional corrections to user's input",
  "tip": "Optional grammar/vocabulary tip"
}
```

#### 2. Audio Response

```json
{
  "type": "audio",
  "request_id": "550e8400-e29b-41d4-a716-446655440000",
  "audio_bytes": "<base64-encoded WAV audio>",
  "format": "wav"
}
```

**Fields:**
- `audio_bytes`: Base64-encoded WAV audio of the spoken reply
- `format`: Audio format (currently always `"wav"`)

#### 3. Error Response

```json
{
  "type": "error",
  "request_id": "550e8400-e29b-41d4-a716-446655440000",
  "message": "Transcription failed: ...",
  "code": "transcription_failed"
}
```

**Error Codes:**
- `invalid_message`: Malformed request
- `transcription_failed`: STT error
- `reply_generation_failed`: LLM error
- `tts_failed`: TTS synthesis error
- `internal_error`: Server error

## Example Flow

```javascript
const ws = new WebSocket('ws://localhost:3000/ws');

ws.onopen = () => {
  // Record audio from microphone
  const audioData = recordAudioAsPCM(); // 16kHz, 16-bit, mono

  // Send request
  ws.send(JSON.stringify({
    type: 'reply',
    audio_bytes: btoa(String.fromCharCode(...audioData)),
    target_language: 'es',
    history: [],
    request_id: crypto.randomUUID()
  }));
};

ws.onmessage = (event) => {
  const msg = JSON.parse(event.data);

  if (msg.type === 'text') {
    console.log('Transcription:', msg.transcription);
    console.log('Reply:', msg.reply);
    console.log('Translation:', msg.original_language_reply);
  }
  else if (msg.type === 'audio') {
    const audioBytes = atob(msg.audio_bytes);
    playAudio(audioBytes); // Play WAV audio
  }
  else if (msg.type === 'error') {
    console.error(`Error [${msg.code}]:`, msg.message);
  }
};
```

## Audio Format Requirements

### Input Audio
- **Format:** Raw PCM (no headers)
- **Sample Rate:** 16000 Hz
- **Bit Depth:** 16-bit
- **Channels:** Mono
- **Encoding:** Base64 string in JSON

### Output Audio
- **Format:** WAV
- **Encoding:** Base64 string in JSON
- Decode base64 and play directly

## Conversation Management

Maintain conversation history by including previous messages in the `history` field. This provides context for more natural conversations:

```json
{
  "type": "reply",
  "audio_bytes": "...",
  "target_language": "es",
  "history": [
    {"role": "user", "content": "¿Cómo te llamas?"},
    {"role": "assistant", "content": "Me llamo AI. ¿Y tú?"}
  ]
}
```

## Health Check

HTTP endpoint for checking server status:
```
GET /health
→ Response: "OK" (200 status)
```

## Notes

- Each request typically takes 1-3 seconds to process
- The server handles STT → LLM → TTS pipeline automatically
- All responses include the original `request_id` for correlation
- Connection supports standard WebSocket ping/pong for keep-alive
