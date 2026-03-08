use async_trait::async_trait;
use reqwest::Client;
use serde::Serialize;
use std::time::Instant;
use tracing::{debug, error, info, instrument};

use crate::{error::TtsError, metrics::TtsMetrics, tts::{TtsProvider, TtsResult}};

/// Configuration for Cartesia TTS service.
#[derive(Debug, Clone)]
pub struct CartesiaConfig {
    /// API key for authentication
    pub api_key: String,
    /// Voice ID to use for synthesis
    pub voice_id: String,
    /// Model ID (e.g., "sonic-english", "sonic-multilingual")
    pub model_id: String,
    /// Speech speed multiplier (e.g., 1.0 for normal, 0.5 for half speed, 2.0 for double)
    pub speed: f32,
    /// Output audio format (e.g., "wav", "mp3", "pcm")
    pub output_format: String,
}

impl Default for CartesiaConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            voice_id: String::new(),
            model_id: "sonic-multilingual".to_string(),
            speed: 1.0,
            output_format: "wav".to_string(),
        }
    }
}

/// Cartesia TTS provider implementation.
pub struct CartesiaTtsProvider {
    client: Client,
    config: CartesiaConfig,
    base_url: String,
}

impl CartesiaTtsProvider {
    /// Create a new Cartesia TTS provider with the given configuration.
    pub fn new(config: CartesiaConfig) -> Self {
        Self {
            client: Client::new(),
            config,
            base_url: "https://api.cartesia.ai/tts/bytes".to_string(),
        }
    }

    /// Create a new provider with custom base URL (useful for testing).
    pub fn with_base_url(config: CartesiaConfig, base_url: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            config,
            base_url: base_url.into(),
        }
    }
}

#[derive(Serialize)]
struct CartesiaRequest<'a> {
    model_id: &'a str,
    transcript: &'a str,
    voice: VoiceConfig<'a>,
    generation_config: GenerationConfig<'a>,
    output_format: OutputFormat<'a>,
    language: Option<&'a str>,
}

#[derive(Serialize)]
struct GenerationConfig<'a> {
    volume: Option<&'a f32>,
    speed: Option<&'a f32>,
    emotion: Option<&'a String>
}

#[derive(Serialize)]
struct VoiceConfig<'a> {
    id: &'a str,
}

#[derive(Serialize)]
struct OutputFormat<'a> {
    container: &'a str,
    encoding: &'a str,
    sample_rate: u32,
}

#[async_trait]
impl TtsProvider for CartesiaTtsProvider {
    #[instrument(skip(self, text), fields(
        voice_id = %self.config.voice_id,
        model_id = %self.config.model_id,
        target_language,
        text_length = text.len()
    ))]
    async fn synthesize(
        &self,
        text: &str,
        target_language: &str,
    ) -> Result<TtsResult, TtsError> {
        let start_time = Instant::now();
        let mut metrics = TtsMetrics::new(
            text.len(),
            target_language.to_string(),
            self.config.output_format.clone(),
        );

        info!("Starting TTS synthesis");

        let request_body = CartesiaRequest {
            model_id: &self.config.model_id,
            transcript: text,
            voice: VoiceConfig {
                id: &self.config.voice_id,
            },
            generation_config: GenerationConfig {
                volume: Some(&1.0),
                speed: Some(&self.config.speed),
                emotion: Some(&"neutral".to_string()),
            },
            output_format: OutputFormat {
                container: &self.config.output_format,
                encoding: "pcm_f32le",
                sample_rate: 44100,
            },
            language: if target_language.is_empty() {
                None
            } else {
                Some(target_language)
            },
        };

        debug!("Sending request to Cartesia API");
        let api_start = Instant::now();
        let response = self
            .client
            .post(&self.base_url)
            .header("X-API-Key", &self.config.api_key)
            .header("Cartesia-Version", "2024-06-10")
            .json(&request_body)
            .send()
            .await
            .map_err(|e| {
                error!(error = ?e, "Cartesia API request failed");
                e
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            error!(status = %status, error = %error_text, "Cartesia API returned error");
            return Err(TtsError::ApiError(format!("HTTP {}: {}", status, error_text)));
        }

        debug!("Receiving audio data from Cartesia");
        let audio_bytes = response.bytes().await.map_err(|e| {
            error!(error = ?e, "Failed to read audio response");
            e
        })?;

        metrics.api_call_duration = api_start.elapsed();
        metrics.audio_size_bytes = audio_bytes.len();

        if audio_bytes.is_empty() {
            error!("Received empty audio data");
            return Err(TtsError::NoAudioData);
        }

        metrics.total_duration = start_time.elapsed();

        info!(
            audio_size = audio_bytes.len(),
            total_duration_ms = metrics.total_duration.as_millis(),
            api_duration_ms = metrics.api_call_duration.as_millis(),
            "Successfully synthesized audio"
        );

        Ok(TtsResult {
            audio_bytes: audio_bytes.to_vec(),
            metrics,
        })
    }
}
