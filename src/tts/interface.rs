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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// Mock implementation of TtsProvider for testing.
    struct MockTtsProvider {
        should_fail: bool,
        audio_data: Vec<u8>,
    }

    impl MockTtsProvider {
        fn success() -> Self {
            // Create mock WAV audio data (just a simple header-like pattern)
            let audio_data = vec![0x52, 0x49, 0x46, 0x46, 0x00, 0x00, 0x00, 0x00];
            Self {
                should_fail: false,
                audio_data,
            }
        }

        fn success_with_data(data: Vec<u8>) -> Self {
            Self {
                should_fail: false,
                audio_data: data,
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

            let metrics = TtsMetrics {
                total_duration: Duration::from_millis(200),
                api_call_duration: Duration::from_millis(180),
                audio_size_bytes: self.audio_data.len(),
                text_length: text.len(),
                target_language: target_language.to_string(),
                output_format: "wav".to_string(),
            };

            Ok(TtsResult {
                audio_bytes: self.audio_data.clone(),
                metrics,
            })
        }
    }

    #[tokio::test]
    async fn test_mock_tts_provider_success() {
        let provider = MockTtsProvider::success();
        let text = "Hola! Estoy bien, gracias.";
        let language = "es";

        let result = provider.synthesize(text, language).await;
        assert!(result.is_ok());

        let tts_result = result.unwrap();
        assert_eq!(tts_result.audio_bytes.len(), 8);
        assert_eq!(tts_result.metrics.text_length, text.len());
        assert_eq!(tts_result.metrics.target_language, language);
        assert_eq!(tts_result.metrics.audio_size_bytes, 8);
        assert_eq!(tts_result.metrics.output_format, "wav");
    }

    #[tokio::test]
    async fn test_mock_tts_provider_failure() {
        let provider = MockTtsProvider::failure();
        let result = provider.synthesize("Hello", "en").await;

        assert!(result.is_err());
        match result {
            Err(TtsError::NoAudioData) => {},
            _ => panic!("Expected NoAudioData error"),
        }
    }

    #[tokio::test]
    async fn test_mock_tts_provider_empty_text() {
        let provider = MockTtsProvider::success();
        let result = provider.synthesize("", "en").await;

        assert!(result.is_ok());
        let tts_result = result.unwrap();
        assert_eq!(tts_result.metrics.text_length, 0);
    }

    #[tokio::test]
    async fn test_mock_tts_provider_different_languages() {
        let provider = MockTtsProvider::success();

        let languages = vec!["en", "es", "fr", "de", "it"];
        for lang in languages {
            let result = provider.synthesize("Test text", lang).await;
            assert!(result.is_ok());
            let tts_result = result.unwrap();
            assert_eq!(tts_result.metrics.target_language, lang);
        }
    }

    #[tokio::test]
    async fn test_mock_tts_provider_custom_audio_data() {
        let custom_data = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        let provider = MockTtsProvider::success_with_data(custom_data.clone());

        let result = provider.synthesize("Test", "en").await;
        assert!(result.is_ok());

        let tts_result = result.unwrap();
        assert_eq!(tts_result.audio_bytes, custom_data);
        assert_eq!(tts_result.metrics.audio_size_bytes, 10);
    }

    #[tokio::test]
    async fn test_tts_result_audio_bytes() {
        let audio_data = vec![0xFF, 0xFE, 0xFD];
        let metrics = TtsMetrics::new(10, "en".to_string(), "mp3".to_string());

        let result = TtsResult {
            audio_bytes: audio_data.clone(),
            metrics,
        };

        assert_eq!(result.audio_bytes, audio_data);
    }
}
