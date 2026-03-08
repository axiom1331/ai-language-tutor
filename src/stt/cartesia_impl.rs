use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use std::time::Instant;
use tokio_tungstenite::{connect_async, tungstenite::{http::HeaderValue, Message}};
use tracing::{debug, error, info, instrument, warn};
use url::Url;
use tungstenite::client::IntoClientRequest;

use crate::{
    error::SttError,
    metrics::SttMetrics,
    stt::{SttProvider, SttResult, WordTimestamp},
};

/// Configuration for Cartesia STT service.
#[derive(Debug, Clone)]
pub struct CartesiaConfig {
    /// API key for authentication
    pub api_key: String,
    /// Model ID (e.g., "ink-whisper")
    pub model_id: String,
    /// Language of the input audio in ISO-639-1 format
    pub language: String,
    /// Audio encoding format (e.g., "pcm_s16le", "pcm_f32le")
    pub encoding: String,
    /// Sample rate of the audio in Hz
    pub sample_rate: u32,
    /// Volume threshold for voice activity detection (0.0-1.0)
    pub min_volume: f32,
    /// Maximum duration of silence before endpointing (in seconds)
    pub max_silence_duration_secs: f32,
}

impl Default for CartesiaConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            model_id: "ink-whisper".to_string(),
            language: "en".to_string(),
            encoding: "pcm_s16le".to_string(),
            sample_rate: 16000,
            min_volume: 0.1,
            max_silence_duration_secs: 0.5,
        }
    }
}

/// Cartesia STT provider implementation using WebSocket streaming.
pub struct CartesiaSttProvider {
    config: CartesiaConfig,
    base_url: String,
}

impl CartesiaSttProvider {
    /// Create a new Cartesia STT provider with the given configuration.
    pub fn new(config: CartesiaConfig) -> Self {
        Self {
            config,
            base_url: "wss://api.cartesia.ai/stt/websocket".to_string(),
        }
    }

    /// Create a new provider with custom base URL (useful for testing).
    pub fn with_base_url(config: CartesiaConfig, base_url: impl Into<String>) -> Self {
        Self {
            config,
            base_url: base_url.into(),
        }
    }

    /// Build the WebSocket URL with query parameters.
    fn build_url(&self) -> Result<String, SttError> {
        let mut url = Url::parse(&self.base_url)
            .map_err(|e| SttError::ConnectionError(format!("Invalid base URL: {}", e)))?;

        url.query_pairs_mut()
            .append_pair("model", &self.config.model_id)
            .append_pair("language", &self.config.language)
            .append_pair("encoding", &self.config.encoding)
            .append_pair("sample_rate", &self.config.sample_rate.to_string())
            .append_pair("min_volume", &self.config.min_volume.to_string())
            .append_pair(
                "max_silence_duration_secs",
                &self.config.max_silence_duration_secs.to_string(),
            );

        Ok(url.to_string())
    }
}

#[derive(Deserialize, Debug)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
enum CartesiaResponse {
    Transcript {
        is_final: bool,
        request_id: String,
        text: String,
        duration: f64,
        #[serde(default)]
        words: Vec<Word>,
    },
    FlushDone {
        request_id: String,
    },
    Done {
        request_id: String,
    },
    Error {
        error: String,
        request_id: String,
    },
}

#[derive(Deserialize, Debug)]
struct Word {
    word: String,
    start: f64,
    end: f64,
}

#[async_trait]
impl SttProvider for CartesiaSttProvider {
    #[instrument(skip(self, audio_bytes), fields(
        model_id = %self.config.model_id,
        language = %self.config.language,
        audio_size = audio_bytes.len()
    ))]
    async fn transcribe(
        &self,
        audio_bytes: &[u8],
        language: &str,
    ) -> Result<SttResult, SttError> {
        let start_time = Instant::now();
        let mut metrics = SttMetrics::new(audio_bytes.len(), language.to_string());

        info!("Starting STT transcription via WebSocket");

        // Build WebSocket URL with custom headers
        let url = self.build_url()?;
        debug!("Connecting to WebSocket: {}", url);

        // Create request with API key header
        let mut request = url
            .into_client_request()
            .map_err(|e| SttError::ConnectionError(format!("Failed to create request: {}", e)))?;

        request.headers_mut().insert(
            "X-API-Key",
            HeaderValue::from_str(&self.config.api_key)
                .map_err(|e| SttError::ConnectionError(format!("Invalid API key: {}", e)))?
        );
        request.headers_mut().insert(
            "Cartesia-Version",
            HeaderValue::from_static("2024-06-10")
        );

        // Connect to WebSocket
        let api_start = Instant::now();
        let (ws_stream, _) = connect_async(request)
            .await
            .map_err(|e| SttError::ConnectionError(format!("WebSocket connection failed: {}", e)))?;

        info!("WebSocket connection established");

        let (mut write, mut read) = ws_stream.split();

        // Send audio data in chunks to avoid size limits (32KB limit on WebSocket frames)
        const CHUNK_SIZE: usize = 30000; // Slightly below 32KB limit for safety
        let total_size = audio_bytes.len();
        let num_chunks = (total_size + CHUNK_SIZE - 1) / CHUNK_SIZE;

        debug!("Sending {} bytes of audio data in {} chunks", total_size, num_chunks);

        for (i, chunk) in audio_bytes.chunks(CHUNK_SIZE).enumerate() {
            debug!("Sending chunk {}/{} ({} bytes)", i + 1, num_chunks, chunk.len());
            write
                .send(Message::Binary(chunk.to_vec()))
                .await
                .map_err(|e| SttError::WebSocketError(format!("Failed to send audio chunk {}: {}", i + 1, e)))?;
        }

        info!("All audio chunks sent successfully");

        // Send "finalize" command to flush remaining audio
        debug!("Sending finalize command");
        write
            .send(Message::Text("finalize".to_string()))
            .await
            .map_err(|e| SttError::WebSocketError(format!("Failed to send finalize: {}", e)))?;

        // Send "done" command to close session
        debug!("Sending done command");
        write
            .send(Message::Text("done".to_string()))
            .await
            .map_err(|e| SttError::WebSocketError(format!("Failed to send done: {}", e)))?;

        // Collect transcription results and merge them
        let mut transcripts: Vec<String> = Vec::new();
        let mut all_words: Vec<WordTimestamp> = Vec::new();
        let mut final_result: Option<SttResult> = None;
        let mut message_count = 0;

        while let Some(msg) = read.next().await {
            message_count += 1;
            let msg = msg.map_err(|e| SttError::WebSocketError(format!("WebSocket error: {}", e)))?;

            match msg {
                Message::Text(text) => {
                    info!("Received WebSocket message #{}: {}", message_count, text);

                    let response: CartesiaResponse = serde_json::from_str(&text)
                        .map_err(|e| {
                            error!("Failed to parse response: {}", e);
                            SttError::InvalidResponse
                        })?;

                    match response {
                        CartesiaResponse::Transcript {
                            is_final,
                            request_id,
                            text,
                            duration,
                            words,
                        } => {
                            info!(
                                is_final = is_final,
                                text = %text,
                                "Received transcript"
                            );

                            metrics.api_call_duration = api_start.elapsed();

                            // Accumulate transcripts
                            if !text.is_empty() {
                                transcripts.push(text.clone());
                            }

                            // Accumulate word timestamps
                            if !words.is_empty() {
                                let word_timestamps: Vec<WordTimestamp> = words
                                    .into_iter()
                                    .map(|w| WordTimestamp {
                                        word: w.word,
                                        start: w.start,
                                        end: w.end,
                                    })
                                    .collect();
                                all_words.extend(word_timestamps);
                            }

                            // Keep track of the final result metadata
                            let result = SttResult {
                                text: text.clone(),
                                is_final,
                                request_id,
                                duration,
                                language: self.config.language.clone(),
                                words: None, // Will be set later with all accumulated words
                                metrics: metrics.clone(),
                            };

                            // Keep the final result or update with latest
                            if is_final || final_result.is_none() {
                                final_result = Some(result);
                            }
                        }
                        CartesiaResponse::FlushDone { request_id } => {
                            debug!("Flush done: {}", request_id);
                        }
                        CartesiaResponse::Done { request_id } => {
                            info!("Session closed: {}", request_id);
                            break;
                        }
                        CartesiaResponse::Error { error, request_id } => {
                            error!(
                                error = %error,
                                request_id = %request_id,
                                "Cartesia STT API error"
                            );
                            return Err(SttError::ApiError(error));
                        }
                    }
                }
                Message::Close(frame) => {
                    info!("WebSocket connection closed: {:?}", frame);
                    break;
                }
                Message::Binary(data) => {
                    warn!("Received unexpected binary message: {} bytes", data.len());
                }
                Message::Ping(data) => {
                    debug!("Received ping: {} bytes", data.len());
                }
                Message::Pong(data) => {
                    debug!("Received pong: {} bytes", data.len());
                }
                Message::Frame(_) => {
                    warn!("Received raw frame (unexpected)");
                }
            }
        }

        metrics.total_duration = start_time.elapsed();

        info!(
            "WebSocket session ended. Total messages received: {}, Final result: {}",
            message_count,
            if final_result.is_some() { "Yes" } else { "No" }
        );

        match final_result {
            Some(mut result) => {
                // Merge all transcripts into one
                let merged_text = transcripts.join(" ");
                result.text = merged_text.clone();

                // Set accumulated words if any
                if !all_words.is_empty() {
                    result.words = Some(all_words);
                }

                result.metrics.total_duration = metrics.total_duration;
                result.metrics.transcript_length = merged_text.len();

                info!(
                    transcript = %result.text,
                    total_duration_ms = metrics.total_duration.as_millis(),
                    api_duration_ms = metrics.api_call_duration.as_millis(),
                    num_transcripts_merged = transcripts.len(),
                    "Successfully transcribed audio"
                );
                Ok(result)
            }
            None => {
                error!(
                    "No transcription received from Cartesia API. Total messages: {}, Audio size: {} bytes",
                    message_count,
                    audio_bytes.len()
                );
                Err(SttError::NoTranscription)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = CartesiaConfig::default();
        assert_eq!(config.model_id, "ink-whisper");
        assert_eq!(config.language, "en");
        assert_eq!(config.encoding, "pcm_s16le");
        assert_eq!(config.sample_rate, 16000);
        assert_eq!(config.min_volume, 0.1);
        assert_eq!(config.max_silence_duration_secs, 0.5);
    }

    #[test]
    fn test_build_url() {
        let config = CartesiaConfig {
            api_key: "test-key".to_string(),
            model_id: "ink-whisper".to_string(),
            language: "es".to_string(),
            encoding: "pcm_s16le".to_string(),
            sample_rate: 16000,
            min_volume: 0.2,
            max_silence_duration_secs: 1.0,
        };

        let provider = CartesiaSttProvider::new(config);
        let url = provider.build_url().unwrap();

        assert!(url.contains("model=ink-whisper"));
        assert!(url.contains("language=es"));
        assert!(url.contains("encoding=pcm_s16le"));
        assert!(url.contains("sample_rate=16000"));
        // API key should NOT be in URL (sent via header instead)
        assert!(!url.contains("api_key"));
    }

    #[test]
    fn test_custom_base_url() {
        let config = CartesiaConfig::default();
        let custom_url = "wss://custom.example.com/stt";
        let provider = CartesiaSttProvider::with_base_url(config, custom_url);

        assert_eq!(provider.base_url, custom_url);
    }
}
