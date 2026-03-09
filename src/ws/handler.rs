use crate::replygen::{Message, ReplyGenerator, Role};
use crate::stt::SttProvider;
use crate::tutor::LanguageTutor;
use crate::ws::protocol::{
    AudioResponse, ClientMessage, ErrorCode, ErrorResponse, MessageRole,
    ServerMessage, TextResponse,
};
use axum::extract::ws::{Message as WsMessage, WebSocket};
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use tracing::{debug, error, info, warn};

/// Shared state for WebSocket handlers
pub struct WsState<S, R, T>
where
    S: SttProvider,
    R: ReplyGenerator,
    T: crate::tts::TtsProvider,
{
    pub stt_provider: Arc<S>,
    pub tutor: Arc<LanguageTutor<R, T>>,
}

impl<S, R, T> Clone for WsState<S, R, T>
where
    S: SttProvider,
    R: ReplyGenerator,
    T: crate::tts::TtsProvider,
{
    fn clone(&self) -> Self {
        Self {
            stt_provider: Arc::clone(&self.stt_provider),
            tutor: Arc::clone(&self.tutor),
        }
    }
}

/// Main WebSocket handler for client connections
pub async fn handle_websocket<S, R, T>(
    socket: WebSocket,
    state: WsState<S, R, T>,
) where
    S: SttProvider + 'static,
    R: ReplyGenerator + 'static,
    T: crate::tts::TtsProvider + 'static,
{
    let (mut sender, mut receiver) = socket.split();
    let client_id = uuid::Uuid::new_v4();

    info!("WebSocket client connected: {}", client_id);

    // Process incoming messages
    while let Some(msg_result) = receiver.next().await {
        match msg_result {
            Ok(WsMessage::Text(text)) => {
                debug!("Received text message from {}: {} bytes", client_id, text.len());

                // Parse client message
                let client_msg = match serde_json::from_str::<ClientMessage>(&text) {
                    Ok(msg) => msg,
                    Err(e) => {
                        error!("Failed to parse client message: {}", e);
                        let error_response = ServerMessage::Error(ErrorResponse {
                            request_id: None,
                            message: format!("Invalid message format: {}", e),
                            code: ErrorCode::InvalidMessage,
                        });

                        if let Ok(json) = serde_json::to_string(&error_response) {
                            let _ = sender.send(WsMessage::Text(json)).await;
                        }
                        continue;
                    }
                };

                // Handle the message
                match client_msg {
                    ClientMessage::Reply(req) => {
                        info!(
                            "Processing reply request {} for language: {}",
                            req.request_id, req.target_language
                        );

                        // Handle the reply request
                        match handle_reply_request(req.clone(), &state).await {
                            Ok((text_response, audio_response)) => {
                                // Send text response
                                if let Ok(json) = serde_json::to_string(&ServerMessage::Text(text_response)) {
                                    if let Err(e) = sender.send(WsMessage::Text(json)).await {
                                        error!("Failed to send text response: {}", e);
                                        break;
                                    }
                                }

                                // Send audio response
                                if let Ok(json) = serde_json::to_string(&ServerMessage::Audio(audio_response)) {
                                    if let Err(e) = sender.send(WsMessage::Text(json)).await {
                                        error!("Failed to send audio response: {}", e);
                                        break;
                                    }
                                }

                                info!("Successfully processed request {}", req.request_id);
                            }
                            Err(error_response) => {
                                error!("Request {} failed: {:?}", req.request_id, error_response);
                                if let Ok(json) = serde_json::to_string(&ServerMessage::Error(error_response)) {
                                    let _ = sender.send(WsMessage::Text(json)).await;
                                }
                            }
                        }
                    }
                }
            }
            Ok(WsMessage::Binary(_)) => {
                warn!("Received unexpected binary message from {}", client_id);
            }
            Ok(WsMessage::Ping(data)) => {
                debug!("Received ping from {}", client_id);
                let _ = sender.send(WsMessage::Pong(data)).await;
            }
            Ok(WsMessage::Pong(_)) => {
                debug!("Received pong from {}", client_id);
            }
            Ok(WsMessage::Close(_)) => {
                info!("Client {} closed connection", client_id);
                break;
            }
            Err(e) => {
                error!("WebSocket error for client {}: {}", client_id, e);
                break;
            }
        }
    }

    info!("WebSocket client disconnected: {}", client_id);
}

/// Handle a reply request by orchestrating STT, LLM, and TTS
async fn handle_reply_request<S, R, T>(
    req: crate::ws::protocol::ReplyRequest,
    state: &WsState<S, R, T>,
) -> Result<(TextResponse, AudioResponse), ErrorResponse>
where
    S: SttProvider,
    R: ReplyGenerator,
    T: crate::tts::TtsProvider,
{
    let request_id = req.request_id;

    // Step 1: Transcribe audio to text
    let stt_result = state
        .stt_provider
        .transcribe(&req.audio_bytes, &req.target_language)
        .await
        .map_err(|e| ErrorResponse {
            request_id: Some(request_id),
            message: format!("Transcription failed: {}", e),
            code: ErrorCode::TranscriptionFailed,
        })?;

    info!(
        "Transcribed {} bytes to: {}",
        req.audio_bytes.len(),
        stt_result.text
    );

    // Step 2: Build conversation history
    let mut history = req
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
        content: stt_result.text.clone(),
    });

    // Step 3: Generate reply with tutor
    let tutor_response = state
        .tutor
        .process(&req.target_language, &history)
        .await
        .map_err(|e| ErrorResponse {
            request_id: Some(request_id),
            message: format!("Reply generation failed: {}", e),
            code: ErrorCode::ReplyGenerationFailed,
        })?;

    info!("Generated reply: {}", tutor_response.reply);

    // Step 4: Prepare responses
    let text_response = TextResponse {
        request_id,
        transcription: stt_result.text,
        reply: tutor_response.reply,
        original_language_reply: tutor_response.original_language_translated_reply,
        corrections: tutor_response.corrections,
        tip: tutor_response.tip,
    };

    let audio_response = AudioResponse {
        request_id,
        audio_bytes: tutor_response.audio_bytes,
        format: "wav".to_string(),
    };

    Ok((text_response, audio_response))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::{AssistantError, SttError, TtsError};
    use crate::metrics::{AnalysisMetrics, SttMetrics, TtsMetrics};
    use crate::replygen::{GenerationResponse, Message, ReplyGenerator};
    use crate::stt::{SttProvider, SttResult};
    use crate::tts::{TtsProvider, TtsResult};
    use crate::tutor::LanguageTutor;
    use async_trait::async_trait;
    use std::time::Duration;

    struct MockSttProvider;

    #[async_trait]
    impl SttProvider for MockSttProvider {
        async fn transcribe(
            &self,
            audio_bytes: &[u8],
            language: &str,
        ) -> Result<SttResult, SttError> {
            Ok(SttResult {
                text: "Hola".to_string(),
                is_final: true,
                request_id: "test".to_string(),
                duration: 1.0,
                language: language.to_string(),
                words: None,
                metrics: SttMetrics {
                    total_duration: Duration::from_millis(100),
                    api_call_duration: Duration::from_millis(80),
                    audio_size_bytes: audio_bytes.len(),
                    transcript_length: 4,
                    language: language.to_string(),
                },
            })
        }
    }

    struct MockReplyGenerator;

    #[async_trait]
    impl ReplyGenerator for MockReplyGenerator {
        async fn generate(
            &self,
            target_language: &str,
            history: &[Message],
        ) -> Result<GenerationResponse, AssistantError> {
            Ok(GenerationResponse {
                reply: "¡Hola! ¿Cómo estás?".to_string(),
                original_language_translated_reply: "Hello! How are you?".to_string(),
                corrections: None,
                tip: None,
                metrics: AnalysisMetrics {
                    total_duration: Duration::from_millis(200),
                    api_call_duration: Duration::from_millis(150),
                    parse_duration: Duration::from_millis(50),
                    input_tokens: Some(10),
                    output_tokens: Some(20),
                    message_count: history.len(),
                    target_language: target_language.to_string(),
                },
            })
        }
    }

    struct MockTtsProvider;

    #[async_trait]
    impl TtsProvider for MockTtsProvider {
        async fn synthesize(
            &self,
            text: &str,
            target_language: &str,
        ) -> Result<TtsResult, TtsError> {
            Ok(TtsResult {
                audio_bytes: vec![1, 2, 3, 4],
                metrics: TtsMetrics {
                    total_duration: Duration::from_millis(150),
                    api_call_duration: Duration::from_millis(130),
                    audio_size_bytes: 4,
                    text_length: text.len(),
                    target_language: target_language.to_string(),
                    output_format: "wav".to_string(),
                },
            })
        }
    }

    #[tokio::test]
    async fn test_handle_reply_request_success() {
        let stt = Arc::new(MockSttProvider);
        let tutor = Arc::new(LanguageTutor::new(MockReplyGenerator, MockTtsProvider));
        let state = WsState {
            stt_provider: stt,
            tutor,
        };

        let request = crate::ws::protocol::ReplyRequest {
            audio_bytes: vec![0u8; 1024],
            target_language: "es".to_string(),
            history: vec![],
            request_id: uuid::Uuid::new_v4(),
        };

        let result = handle_reply_request(request.clone(), &state).await;
        assert!(result.is_ok());

        let (text_resp, audio_resp) = result.unwrap();
        assert_eq!(text_resp.request_id, request.request_id);
        assert_eq!(text_resp.transcription, "Hola");
        assert_eq!(text_resp.reply, "¡Hola! ¿Cómo estás?");
        assert_eq!(audio_resp.audio_bytes, vec![1, 2, 3, 4]);
    }
}
