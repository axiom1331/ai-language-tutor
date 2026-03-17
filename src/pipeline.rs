use crate::replygen::{Message, ReplyGenerator, Role};
use crate::stt::SttProvider;
use crate::tts::TtsProvider;
use crate::ws::{AudioResponse, MessageRole, TextResponse};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, error, info};
use uuid::Uuid;

/// Message sent from WebSocket to STT worker
#[derive(Debug, Clone)]
pub struct SttRequest {
    pub request_id: Uuid,
    pub audio_bytes: Vec<u8>,
    pub target_language: String,
    pub history: Vec<HistoryMessage>,
    pub response_tx: mpsc::UnboundedSender<PipelineResponse>,
}

/// History message for conversation context
#[derive(Debug, Clone)]
pub struct HistoryMessage {
    pub role: MessageRole,
    pub content: String,
}

/// Message sent from STT to Reply worker
#[derive(Debug, Clone)]
pub struct ReplyRequest {
    pub request_id: Uuid,
    pub transcription: String,
    pub target_language: String,
    pub history: Vec<HistoryMessage>,
    pub response_tx: mpsc::UnboundedSender<PipelineResponse>,
}

/// Message sent from Reply to TTS worker
#[derive(Debug, Clone)]
pub struct TtsRequest {
    pub request_id: Uuid,
    pub transcription: String,
    pub reply: String,
    pub original_language_reply: String,
    pub corrections: Option<String>,
    pub tip: Option<String>,
    pub target_language: String,
    pub response_tx: mpsc::UnboundedSender<PipelineResponse>,
}

/// Response messages sent back to WebSocket
#[derive(Debug, Clone)]
pub enum PipelineResponse {
    Text(TextResponse),
    Audio(AudioResponse),
    Transcription {
        request_id: Uuid,
        transcription: String,
    },
    Error {
        request_id: Uuid,
        message: String,
        stage: String,
    },
}

/// STT worker that processes audio transcription
pub async fn stt_worker<S>(
    stt_provider: Arc<S>,
    mut rx: mpsc::UnboundedReceiver<SttRequest>,
    reply_tx: mpsc::UnboundedSender<ReplyRequest>,
) where
    S: SttProvider + 'static,
{
    info!("STT worker started");

    while let Some(request) = rx.recv().await {
        debug!("STT processing request {}", request.request_id);

        let result = stt_provider
            .transcribe(&request.audio_bytes, &request.target_language)
            .await;

        match result {
            Ok(stt_result) => {
                info!(
                    "Request {}: Transcribed {} bytes to: {}",
                    request.request_id,
                    request.audio_bytes.len(),
                    stt_result.text
                );

                // Send transcription back to WebSocket for saving
                let _ = request.response_tx.send(PipelineResponse::Transcription {
                    request_id: request.request_id,
                    transcription: stt_result.text.clone(),
                });

                // Forward to Reply worker
                let reply_request = ReplyRequest {
                    request_id: request.request_id,
                    transcription: stt_result.text,
                    target_language: request.target_language,
                    history: request.history,
                    response_tx: request.response_tx,
                };

                if let Err(e) = reply_tx.send(reply_request) {
                    error!("Failed to send to Reply worker: {}", e);
                }
            }
            Err(e) => {
                error!("Request {}: STT failed: {}", request.request_id, e);
                let _ = request.response_tx.send(PipelineResponse::Error {
                    request_id: request.request_id,
                    message: format!("Transcription failed: {}", e),
                    stage: "STT".to_string(),
                });
            }
        }
    }

    info!("STT worker stopped");
}

/// Reply generation worker
pub async fn reply_worker<R>(
    reply_generator: Arc<R>,
    mut rx: mpsc::UnboundedReceiver<ReplyRequest>,
    tts_tx: mpsc::UnboundedSender<TtsRequest>,
) where
    R: ReplyGenerator + 'static,
{
    info!("Reply worker started");

    while let Some(request) = rx.recv().await {
        debug!("Reply processing request {}", request.request_id);

        // Build conversation history
        let mut history = request
            .history
            .iter()
            .map(|h| Message {
                role: match h.role {
                    MessageRole::User => Role::User,
                    MessageRole::Assistant => Role::Assistant,
                },
                content: h.content.clone(),
            })
            .collect::<Vec<_>>();

        // Add the transcribed user message
        history.push(Message {
            role: Role::User,
            content: request.transcription.clone(),
        });

        // Generate reply
        let result = reply_generator
            .generate(&request.target_language, &history)
            .await;

        match result {
            Ok(generation_response) => {
                info!(
                    "Request {}: Generated reply: {}",
                    request.request_id, generation_response.reply
                );

                // Forward to TTS worker
                let tts_request = TtsRequest {
                    request_id: request.request_id,
                    transcription: request.transcription,
                    reply: generation_response.reply,
                    original_language_reply: generation_response.original_language_translated_reply,
                    corrections: generation_response.corrections,
                    tip: generation_response.tip,
                    target_language: request.target_language,
                    response_tx: request.response_tx.clone(),
                };

                // Send text response immediately
                let text_response = TextResponse {
                    request_id: request.request_id,
                    transcription: tts_request.transcription.clone(),
                    reply: tts_request.reply.clone(),
                    original_language_reply: tts_request.original_language_reply.clone(),
                    corrections: tts_request.corrections.clone(),
                    tip: tts_request.tip.clone(),
                };

                let _ = request
                    .response_tx
                    .send(PipelineResponse::Text(text_response));

                // Forward to TTS for audio generation
                if let Err(e) = tts_tx.send(tts_request) {
                    error!("Failed to send to TTS worker: {}", e);
                }
            }
            Err(e) => {
                error!("Request {}: Reply failed: {}", request.request_id, e);
                let _ = request.response_tx.send(PipelineResponse::Error {
                    request_id: request.request_id,
                    message: format!("Reply generation failed: {}", e),
                    stage: "Reply".to_string(),
                });
            }
        }
    }

    info!("Reply worker stopped");
}

/// TTS worker that processes text-to-speech
pub async fn tts_worker<T>(
    tts_provider: Arc<T>,
    mut rx: mpsc::UnboundedReceiver<TtsRequest>,
) where
    T: TtsProvider + 'static,
{
    info!("TTS worker started");

    while let Some(request) = rx.recv().await {
        debug!("TTS processing request {}", request.request_id);

        let result = tts_provider
            .synthesize(&request.reply, &request.target_language)
            .await;

        match result {
            Ok(tts_result) => {
                info!(
                    "Request {}: Synthesized {} bytes of audio",
                    request.request_id,
                    tts_result.audio_bytes.len()
                );

                let audio_response = AudioResponse {
                    request_id: request.request_id,
                    audio_bytes: tts_result.audio_bytes,
                    format: "wav".to_string(),
                };

                let _ = request
                    .response_tx
                    .send(PipelineResponse::Audio(audio_response));
            }
            Err(e) => {
                error!("Request {}: TTS failed: {}", request.request_id, e);
                let _ = request.response_tx.send(PipelineResponse::Error {
                    request_id: request.request_id,
                    message: format!("TTS synthesis failed: {}", e),
                    stage: "TTS".to_string(),
                });
            }
        }
    }

    info!("TTS worker stopped");
}

/// Pipeline handles for communication with workers
#[derive(Clone)]
pub struct Pipeline {
    pub stt_tx: mpsc::UnboundedSender<SttRequest>,
}

impl Pipeline {
    pub fn new<S, R, T>(
        stt_provider: Arc<S>,
        reply_generator: Arc<R>,
        tts_provider: Arc<T>,
    ) -> Self
    where
        S: SttProvider + Send + 'static,
        R: ReplyGenerator + Send + 'static,
        T: TtsProvider + Send + 'static,
    {
        // Create channels
        let (stt_tx, stt_rx) = mpsc::unbounded_channel();
        let (reply_tx, reply_rx) = mpsc::unbounded_channel();
        let (tts_tx, tts_rx) = mpsc::unbounded_channel();

        // Spawn workers
        tokio::spawn(stt_worker(stt_provider, stt_rx, reply_tx));
        tokio::spawn(reply_worker(reply_generator, reply_rx, tts_tx));
        tokio::spawn(tts_worker(tts_provider, tts_rx));

        info!("Pipeline workers spawned");

        Self { stt_tx }
    }
}
