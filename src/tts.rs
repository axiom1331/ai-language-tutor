use async_trait::async_trait;
use crate::error::TtsError;
use crate::metrics::TtsMetrics;

/// Result of TTS synthesis containing audio data and metrics.
#[derive(Debug)]
pub struct TtsResult {
    /// The synthesized audio data
    pub audio_bytes: Vec<u8>,
    /// Metrics about the synthesis operation
    pub metrics: TtsMetrics,
}

/// Core interface for text-to-speech providers.
///
/// Implementations are responsible for converting text to audio bytes
/// using a backing TTS service.
#[async_trait]
pub trait TtsProvider: Send + Sync {
    /// Convert text to speech audio for the given target language.
    ///
    /// Returns the audio data and metrics about the operation.
    async fn synthesize(
        &self,
        text: &str,
        target_language: &str,
    ) -> Result<TtsResult, TtsError>;
}
