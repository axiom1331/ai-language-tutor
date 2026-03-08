use crate::error::{AssistantError, TtsError};
use crate::metrics::SessionMetrics;
use crate::replygen::{Message, ReplyGenerator};
use crate::tts::TtsProvider;
use std::time::Instant;
use tracing::info;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::{AssistantError, TtsError};
    use crate::metrics::{AnalysisMetrics, TtsMetrics};
    use crate::replygen::{GenerationResponse, Message, ReplyGenerator, Role};
    use crate::tts::{TtsProvider, TtsResult};
    use async_trait::async_trait;
    use std::time::Duration;

    /// Mock ReplyGenerator for testing
    struct MockReplyGenerator {
        should_fail: bool,
        reply: String,
    }

    impl MockReplyGenerator {
        fn success(reply: String) -> Self {
            Self {
                should_fail: false,
                reply,
            }
        }

        fn failure() -> Self {
            Self {
                should_fail: true,
                reply: String::new(),
            }
        }
    }

    #[async_trait]
    impl ReplyGenerator for MockReplyGenerator {
        async fn generate(
            &self,
            target_language: &str,
            history: &[Message],
        ) -> Result<GenerationResponse, AssistantError> {
            if self.should_fail {
                return Err(AssistantError::EmptyResponse);
            }

            Ok(GenerationResponse {
                reply: self.reply.clone(),
                original_language_translated_reply: "Translation".to_string(),
                corrections: Some("Some correction".to_string()),
                tip: Some("Some tip".to_string()),
                metrics: AnalysisMetrics {
                    total_duration: Duration::from_millis(100),
                    api_call_duration: Duration::from_millis(80),
                    parse_duration: Duration::from_millis(20),
                    input_tokens: Some(10),
                    output_tokens: Some(30),
                    message_count: history.len(),
                    target_language: target_language.to_string(),
                },
            })
        }
    }

    /// Mock TtsProvider for testing
    struct MockTtsProvider {
        should_fail: bool,
        audio_data: Vec<u8>,
    }

    impl MockTtsProvider {
        fn success(audio_data: Vec<u8>) -> Self {
            Self {
                should_fail: false,
                audio_data,
            }
        }

        fn failure() -> Self {
            Self {
                should_fail: true,
                audio_data: vec![],
            }
        }
    }

    #[async_trait]
    impl TtsProvider for MockTtsProvider {
        async fn synthesize(
            &self,
            text: &str,
            target_language: &str,
        ) -> Result<TtsResult, TtsError> {
            if self.should_fail {
                return Err(TtsError::NoAudioData);
            }

            Ok(TtsResult {
                audio_bytes: self.audio_data.clone(),
                metrics: TtsMetrics {
                    total_duration: Duration::from_millis(150),
                    api_call_duration: Duration::from_millis(130),
                    audio_size_bytes: self.audio_data.len(),
                    text_length: text.len(),
                    target_language: target_language.to_string(),
                    output_format: "wav".to_string(),
                },
            })
        }
    }

    #[tokio::test]
    async fn test_tutor_process_success() {
        let reply_gen = MockReplyGenerator::success("Hola!".to_string());
        let tts = MockTtsProvider::success(vec![1, 2, 3, 4]);
        let tutor = LanguageTutor::new(reply_gen, tts);

        let history = vec![Message {
            role: Role::User,
            content: "Hello".to_string(),
        }];

        let result = tutor.process("es", &history).await;
        assert!(result.is_ok());

        let response = result.unwrap();
        assert_eq!(response.reply, "Hola!");
        assert_eq!(response.original_language_translated_reply, "Translation");
        assert!(response.corrections.is_some());
        assert!(response.tip.is_some());
        assert_eq!(response.audio_bytes, vec![1, 2, 3, 4]);
        assert!(response.metrics.has_corrections);
        assert!(response.metrics.has_tip);
    }

    #[tokio::test]
    async fn test_tutor_process_reply_generator_failure() {
        let reply_gen = MockReplyGenerator::failure();
        let tts = MockTtsProvider::success(vec![1, 2, 3]);
        let tutor = LanguageTutor::new(reply_gen, tts);

        let history = vec![Message {
            role: Role::User,
            content: "Hello".to_string(),
        }];

        let result = tutor.process("es", &history).await;
        assert!(result.is_err());

        match result {
            Err(TutorError::ReplyGeneration(AssistantError::EmptyResponse)) => {},
            _ => panic!("Expected ReplyGeneration error"),
        }
    }

    #[tokio::test]
    async fn test_tutor_process_tts_failure() {
        let reply_gen = MockReplyGenerator::success("Bonjour!".to_string());
        let tts = MockTtsProvider::failure();
        let tutor = LanguageTutor::new(reply_gen, tts);

        let history = vec![Message {
            role: Role::User,
            content: "Hi".to_string(),
        }];

        let result = tutor.process("fr", &history).await;
        assert!(result.is_err());

        match result {
            Err(TutorError::TtsSynthesis(TtsError::NoAudioData)) => {},
            _ => panic!("Expected TtsSynthesis error"),
        }
    }

    #[tokio::test]
    async fn test_tutor_process_no_corrections_or_tips() {
        struct NoFeedbackReplyGenerator;

        #[async_trait]
        impl ReplyGenerator for NoFeedbackReplyGenerator {
            async fn generate(
                &self,
                target_language: &str,
                history: &[Message],
            ) -> Result<GenerationResponse, AssistantError> {
                Ok(GenerationResponse {
                    reply: "Perfect!".to_string(),
                    original_language_translated_reply: "Perfect!".to_string(),
                    corrections: None,
                    tip: None,
                    metrics: AnalysisMetrics {
                        total_duration: Duration::from_millis(50),
                        api_call_duration: Duration::from_millis(40),
                        parse_duration: Duration::from_millis(10),
                        input_tokens: Some(5),
                        output_tokens: Some(15),
                        message_count: history.len(),
                        target_language: target_language.to_string(),
                    },
                })
            }
        }

        let reply_gen = NoFeedbackReplyGenerator;
        let tts = MockTtsProvider::success(vec![5, 6, 7]);
        let tutor = LanguageTutor::new(reply_gen, tts);

        let history = vec![Message {
            role: Role::User,
            content: "Bonjour".to_string(),
        }];

        let result = tutor.process("fr", &history).await;
        assert!(result.is_ok());

        let response = result.unwrap();
        assert!(response.corrections.is_none());
        assert!(response.tip.is_none());
        assert!(!response.metrics.has_corrections);
        assert!(!response.metrics.has_tip);
    }

    #[tokio::test]
    async fn test_tutor_new() {
        let reply_gen = MockReplyGenerator::success("Test".to_string());
        let tts = MockTtsProvider::success(vec![]);
        let _tutor = LanguageTutor::new(reply_gen, tts);
        // If this compiles and runs, the constructor works
    }

    #[tokio::test]
    async fn test_tutor_process_multiple_messages() {
        let reply_gen = MockReplyGenerator::success("Response".to_string());
        let tts = MockTtsProvider::success(vec![8, 9, 10]);
        let tutor = LanguageTutor::new(reply_gen, tts);

        let history = vec![
            Message {
                role: Role::User,
                content: "First message".to_string(),
            },
            Message {
                role: Role::Assistant,
                content: "First response".to_string(),
            },
            Message {
                role: Role::User,
                content: "Second message".to_string(),
            },
        ];

        let result = tutor.process("de", &history).await;
        assert!(result.is_ok());

        let response = result.unwrap();
        assert_eq!(response.metrics.analysis.message_count, 3);
    }

    #[tokio::test]
    async fn test_tutor_response_metrics() {
        let reply_gen = MockReplyGenerator::success("Test reply".to_string());
        let tts = MockTtsProvider::success(vec![11, 12, 13, 14, 15]);
        let tutor = LanguageTutor::new(reply_gen, tts);

        let history = vec![Message {
            role: Role::User,
            content: "Test".to_string(),
        }];

        let result = tutor.process("it", &history).await;
        assert!(result.is_ok());

        let response = result.unwrap();
        // Duration might be very small but should be present
        assert_eq!(response.metrics.analysis.target_language, "it");
        assert!(response.metrics.tts.is_some());
        let tts_metrics = response.metrics.tts.unwrap();
        assert_eq!(tts_metrics.audio_size_bytes, 5);
        assert_eq!(tts_metrics.output_format, "wav");
    }
}
