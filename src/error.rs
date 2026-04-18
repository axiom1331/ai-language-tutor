use thiserror::Error;

#[derive(Debug, Error)]
pub enum AssistantError {
    #[error("Bedrock API error: {0}")]
    Bedrock(String),

    #[error("OpenAI API error: {0}")]
    OpenAi(String),

    #[error("Failed to serialize request: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Model returned an empty response")]
    EmptyResponse,

    #[error("Failed to parse model output: {0}")]
    ParseResponse(String),
}

#[derive(Debug, Error)]
pub enum TtsError {
    #[error("HTTP request failed: {0}")]
    HttpError(#[from] reqwest::Error),

    #[error("Failed to serialize request: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("TTS API error: {0}")]
    ApiError(String),

    #[error("Invalid response format")]
    InvalidResponse,

    #[error("No audio data received")]
    NoAudioData,
}

#[derive(Debug, Error)]
pub enum SttError {
    #[error("WebSocket error: {0}")]
    WebSocketError(String),

    #[error("Failed to serialize request: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("STT API error: {0}")]
    ApiError(String),

    #[error("Invalid response format")]
    InvalidResponse,

    #[error("No transcription received")]
    NoTranscription,

    #[error("Connection error: {0}")]
    ConnectionError(String),

    #[error("Audio encoding error: {0}")]
    AudioEncodingError(String),
}