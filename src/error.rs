use thiserror::Error;

#[derive(Debug, Error)]
pub enum AssistantError {
    #[error("Bedrock API error: {0}")]
    Bedrock(String),

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