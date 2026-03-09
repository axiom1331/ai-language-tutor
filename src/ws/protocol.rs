use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Messages sent from the client to the server
#[derive(Debug, Deserialize, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    /// Audio-based reply request
    Reply(ReplyRequest),
}

/// Request for generating a reply from user audio
#[derive(Debug, Deserialize, Clone)]
pub struct ReplyRequest {
    /// Raw PCM audio bytes (16000 Hz sample rate)
    #[serde(with = "serde_bytes")]
    pub audio_bytes: Vec<u8>,

    /// Target language code (e.g., "es", "fr", "de")
    pub target_language: String,

    /// Optional conversation history context
    #[serde(default)]
    pub history: Vec<HistoryMessage>,

    /// Unique client-provided request ID for tracking
    #[serde(default = "Uuid::new_v4")]
    pub request_id: Uuid,
}

/// Message in conversation history
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HistoryMessage {
    pub role: MessageRole,
    pub content: String,
}

/// Role of a message in conversation
#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    User,
    Assistant,
}

/// Messages sent from the server to the client
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    /// Text response with transcription and generated reply
    Text(TextResponse),

    /// Audio response with TTS bytes
    Audio(AudioResponse),

    /// Error occurred during processing
    Error(ErrorResponse),
}

/// Text-based response containing transcription and reply
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TextResponse {
    /// Original request ID
    pub request_id: Uuid,

    /// Transcribed user audio
    pub transcription: String,

    /// Generated reply in target language
    pub reply: String,

    /// Reply translated to user's original language
    pub original_language_reply: String,

    /// Optional corrections to user's input
    #[serde(skip_serializing_if = "Option::is_none")]
    pub corrections: Option<String>,

    /// Optional grammar/vocabulary tip
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tip: Option<String>,
}

/// Audio response containing TTS output
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AudioResponse {
    /// Original request ID
    pub request_id: Uuid,

    /// Audio bytes of the reply (WAV format)
    #[serde(with = "serde_bytes")]
    pub audio_bytes: Vec<u8>,

    /// Audio format (e.g., "wav")
    pub format: String,
}

/// Error response
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ErrorResponse {
    /// Original request ID if available
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<Uuid>,

    /// Error message
    pub message: String,

    /// Error code for client handling
    pub code: ErrorCode,
}

/// Error codes for different failure scenarios
#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// Invalid message format
    InvalidMessage,

    /// STT transcription failed
    TranscriptionFailed,

    /// LLM reply generation failed
    ReplyGenerationFailed,

    /// TTS synthesis failed
    TtsFailed,

    /// Internal server error
    InternalError,
}

// Custom serde module for binary data
mod serde_bytes {
    use serde::{Deserialize, Deserializer, Serializer};
    use base64::{Engine as _, engine::general_purpose};

    pub fn serialize<S>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let encoded = general_purpose::STANDARD.encode(bytes);
        serializer.serialize_str(&encoded)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let encoded = String::deserialize(deserializer)?;
        general_purpose::STANDARD
            .decode(encoded)
            .map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_message_deserialization() {
        let json = r#"{
            "type": "reply",
            "audio_bytes": "SGVsbG8gV29ybGQ=",
            "target_language": "es",
            "history": [],
            "request_id": "550e8400-e29b-41d4-a716-446655440000"
        }"#;

        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        match msg {
            ClientMessage::Reply(req) => {
                assert_eq!(req.target_language, "es");
                assert_eq!(req.audio_bytes, b"Hello World");
            }
        }
    }

    #[test]
    fn test_server_message_serialization() {
        let response = ServerMessage::Text(TextResponse {
            request_id: Uuid::new_v4(),
            transcription: "Hola".to_string(),
            reply: "¡Hola! ¿Cómo estás?".to_string(),
            original_language_reply: "Hello! How are you?".to_string(),
            corrections: None,
            tip: Some("Use '¿' at the beginning of questions".to_string()),
        });

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"type\":\"text\""));
        assert!(json.contains("Hola"));
    }

    #[test]
    fn test_audio_response_binary_encoding() {
        let audio_data = vec![0x52, 0x49, 0x46, 0x46]; // "RIFF" header
        let response = ServerMessage::Audio(AudioResponse {
            request_id: Uuid::new_v4(),
            audio_bytes: audio_data.clone(),
            format: "wav".to_string(),
        });

        let json = serde_json::to_string(&response).unwrap();
        let deserialized: ServerMessage = serde_json::from_str(&json).unwrap();

        match deserialized {
            ServerMessage::Audio(audio) => {
                assert_eq!(audio.audio_bytes, audio_data);
                assert_eq!(audio.format, "wav");
            }
            _ => panic!("Expected Audio message"),
        }
    }

    #[test]
    fn test_error_response() {
        let error = ServerMessage::Error(ErrorResponse {
            request_id: Some(Uuid::new_v4()),
            message: "Transcription failed".to_string(),
            code: ErrorCode::TranscriptionFailed,
        });

        let json = serde_json::to_string(&error).unwrap();
        assert!(json.contains("\"code\":\"transcription_failed\""));
    }
}
