use async_trait::async_trait;
use crate::error::SttError;
use crate::metrics::SttMetrics;

/// Result of STT transcription containing text and metrics.
#[derive(Debug)]
pub struct SttResult {
    /// The transcribed text
    pub text: String,
    /// Whether this is the final transcription result
    pub is_final: bool,
    /// Unique identifier for this transcription request
    pub request_id: String,
    /// Duration of the audio in seconds
    pub duration: f64,
    /// Detected or specified language code
    pub language: String,
    /// Word-level timestamps (if available)
    pub words: Option<Vec<WordTimestamp>>,
    /// Metrics about the transcription operation
    pub metrics: SttMetrics,
}

/// Word-level timestamp information.
#[derive(Debug, Clone)]
pub struct WordTimestamp {
    /// The transcribed word
    pub word: String,
    /// Start time in seconds
    pub start: f64,
    /// End time in seconds
    pub end: f64,
}

/// Core interface for speech-to-text providers.
///
/// Implementations are responsible for converting audio bytes to text
/// using a backing STT service.
#[async_trait]
pub trait SttProvider: Send + Sync {
    /// Transcribe audio bytes to text for the given language.
    ///
    /// Returns the transcribed text and metrics about the operation.
    async fn transcribe(
        &self,
        audio_bytes: &[u8],
        language: &str,
    ) -> Result<SttResult, SttError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// Mock implementation of SttProvider for testing.
    struct MockSttProvider {
        should_fail: bool,
        transcript: String,
    }

    impl MockSttProvider {
        fn success() -> Self {
            Self {
                should_fail: false,
                transcript: "Hello world".to_string(),
            }
        }

        fn success_with_transcript(transcript: String) -> Self {
            Self {
                should_fail: false,
                transcript,
            }
        }

        fn failure() -> Self {
            Self {
                should_fail: true,
                transcript: String::new(),
            }
        }
    }

    #[async_trait]
    impl SttProvider for MockSttProvider {
        async fn transcribe(
            &self,
            audio_bytes: &[u8],
            language: &str,
        ) -> Result<SttResult, SttError> {
            if self.should_fail {
                return Err(SttError::NoTranscription);
            }

            let metrics = SttMetrics {
                total_duration: Duration::from_millis(200),
                api_call_duration: Duration::from_millis(180),
                audio_size_bytes: audio_bytes.len(),
                transcript_length: self.transcript.len(),
                language: language.to_string(),
            };

            Ok(SttResult {
                text: self.transcript.clone(),
                is_final: true,
                request_id: "test-request-id".to_string(),
                duration: 1.5,
                language: language.to_string(),
                words: Some(vec![
                    WordTimestamp {
                        word: "Hello".to_string(),
                        start: 0.0,
                        end: 0.5,
                    },
                    WordTimestamp {
                        word: "world".to_string(),
                        start: 0.6,
                        end: 1.0,
                    },
                ]),
                metrics,
            })
        }
    }

    #[tokio::test]
    async fn test_mock_stt_provider_success() {
        let provider = MockSttProvider::success();
        let audio_data = vec![0u8; 1024]; // Mock audio data
        let language = "en";

        let result = provider.transcribe(&audio_data, language).await;
        assert!(result.is_ok());

        let stt_result = result.unwrap();
        assert_eq!(stt_result.text, "Hello world");
        assert!(stt_result.is_final);
        assert_eq!(stt_result.language, language);
        assert_eq!(stt_result.metrics.audio_size_bytes, 1024);
    }

    #[tokio::test]
    async fn test_mock_stt_provider_failure() {
        let provider = MockSttProvider::failure();
        let audio_data = vec![0u8; 1024];
        let result = provider.transcribe(&audio_data, "en").await;

        assert!(result.is_err());
        match result {
            Err(SttError::NoTranscription) => {},
            _ => panic!("Expected NoTranscription error"),
        }
    }

    #[tokio::test]
    async fn test_mock_stt_provider_empty_audio() {
        let provider = MockSttProvider::success();
        let audio_data: Vec<u8> = vec![];
        let result = provider.transcribe(&audio_data, "en").await;

        assert!(result.is_ok());
        let stt_result = result.unwrap();
        assert_eq!(stt_result.metrics.audio_size_bytes, 0);
    }

    #[tokio::test]
    async fn test_mock_stt_provider_different_languages() {
        let provider = MockSttProvider::success();
        let audio_data = vec![0u8; 512];

        let languages = vec!["en", "es", "fr", "de", "it"];
        for lang in languages {
            let result = provider.transcribe(&audio_data, lang).await;
            assert!(result.is_ok());
            let stt_result = result.unwrap();
            assert_eq!(stt_result.language, lang);
        }
    }

    #[tokio::test]
    async fn test_mock_stt_provider_custom_transcript() {
        let custom_transcript = "This is a custom transcription".to_string();
        let provider = MockSttProvider::success_with_transcript(custom_transcript.clone());
        let audio_data = vec![0u8; 1024];

        let result = provider.transcribe(&audio_data, "en").await;
        assert!(result.is_ok());

        let stt_result = result.unwrap();
        assert_eq!(stt_result.text, custom_transcript);
        assert_eq!(stt_result.metrics.transcript_length, custom_transcript.len());
    }

    #[tokio::test]
    async fn test_word_timestamps() {
        let provider = MockSttProvider::success();
        let audio_data = vec![0u8; 1024];

        let result = provider.transcribe(&audio_data, "en").await;
        assert!(result.is_ok());

        let stt_result = result.unwrap();
        assert!(stt_result.words.is_some());

        let words = stt_result.words.unwrap();
        assert_eq!(words.len(), 2);
        assert_eq!(words[0].word, "Hello");
        assert_eq!(words[0].start, 0.0);
        assert_eq!(words[0].end, 0.5);
        assert_eq!(words[1].word, "world");
        assert_eq!(words[1].start, 0.6);
        assert_eq!(words[1].end, 1.0);
    }
}
