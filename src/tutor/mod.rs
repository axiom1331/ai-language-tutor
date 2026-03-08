use crate::error::{AssistantError, TtsError};
use crate::metrics::SessionMetrics;
use crate::replygen::{Message, ReplyGenerator};
use crate::tts::TtsProvider;
use std::time::Instant;
use tracing::{error, info};

/// Result returned by the tutor containing both text and audio.
#[derive(Debug)]
pub struct TutorResponse {
    /// The reply text in the target language
    pub reply: String,
    /// The reply text in the user's original language
    pub original_language_translated_reply: String,
    /// Optional corrections to the user's input
    pub corrections: Option<String>,
    /// Optional grammar or vocabulary tip
    pub tip: Option<String>,
    /// The audio bytes for the reply
    pub audio_bytes: Vec<u8>,
    /// Session metrics for the entire operation
    pub metrics: SessionMetrics,
}

/// Error type for tutor operations.
#[derive(Debug, thiserror::Error)]
pub enum TutorError {
    #[error("Reply generation failed: {0}")]
    ReplyGeneration(#[from] AssistantError),
    #[error("TTS synthesis failed: {0}")]
    TtsSynthesis(#[from] TtsError),
}

/// Language learning tutor that combines reply generation and TTS.
pub struct LanguageTutor<R, T>
where
    R: ReplyGenerator,
    T: TtsProvider,
{
    reply_generator: R,
    tts_provider: T,
}

impl<R, T> LanguageTutor<R, T>
where
    R: ReplyGenerator,
    T: TtsProvider,
{
    /// Create a new language tutor with the given reply generator and TTS provider.
    pub fn new(reply_generator: R, tts_provider: T) -> Self {
        Self {
            reply_generator,
            tts_provider,
        }
    }

    /// Process a user message and return a complete response with text and audio.
    ///
    /// # Arguments
    /// * `target_language` - The language the user is learning
    /// * `history` - The conversation history including the latest user message
    ///
    /// # Returns
    /// A `TutorResponse` containing the reply text, audio, and metrics
    pub async fn process(
        &self,
        target_language: &str,
        history: &[Message],
    ) -> Result<TutorResponse, TutorError> {
        let session_start = Instant::now();

        info!("Processing user message for language: {}", target_language);

        // Generate the reply
        let generation_response = self
            .reply_generator
            .generate(target_language, history)
            .await?;

        info!("Successfully generated reply");

        let analysis_metrics = generation_response.metrics;
        let has_corrections = generation_response.corrections.is_some();
        let has_tip = generation_response.tip.is_some();

        // Synthesize speech from the reply
        info!("Synthesizing speech for reply");
        let tts_result = self
            .tts_provider
            .synthesize(&generation_response.reply, target_language)
            .await?;

        info!("Successfully synthesized audio");

        // Construct final response
        let metrics = SessionMetrics {
            analysis: analysis_metrics,
            tts: Some(tts_result.metrics),
            total_duration: session_start.elapsed(),
            has_corrections,
            has_tip,
        };

        Ok(TutorResponse {
            reply: generation_response.reply,
            original_language_translated_reply: generation_response.original_language_translated_reply,
            corrections: generation_response.corrections,
            tip: generation_response.tip,
            audio_bytes: tts_result.audio_bytes,
            metrics,
        })
    }
}
