use crate::pipeline::{HistoryMessage, Pipeline, PipelineResponse, SttRequest};
use crate::ws::protocol::{ClientMessage, ErrorCode, ErrorResponse, ServerMessage};
use axum::extract::ws::{Message as WsMessage, WebSocket};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

/// Shared state for WebSocket handlers
pub struct WsState {
    pub pipeline: Pipeline,
}

impl Clone for WsState {
    fn clone(&self) -> Self {
        Self {
            pipeline: self.pipeline.clone(),
        }
    }
}

/// Main WebSocket handler for client connections
pub async fn handle_websocket(socket: WebSocket, state: WsState) {
    let (sender, mut receiver) = socket.split();
    let client_id = uuid::Uuid::new_v4();

    info!("WebSocket client connected: {}", client_id);

    // Create a channel to receive responses from the pipeline
    let (response_tx, mut response_rx) = mpsc::unbounded_channel();

    // Create a channel for sending messages to the WebSocket
    let (ws_tx, mut ws_rx) = mpsc::unbounded_channel::<WsMessage>();

    // Spawn a task to handle sending messages to WebSocket
    tokio::spawn(async move {
        let mut sender = sender;
        while let Some(msg) = ws_rx.recv().await {
            if let Err(e) = sender.send(msg).await {
                error!("Failed to send WebSocket message: {}", e);
                break;
            }
        }
    });

    // Spawn a task to handle pipeline responses
    let ws_tx_clone = ws_tx.clone();
    tokio::spawn(async move {
        while let Some(response) = response_rx.recv().await {
            match response {
                PipelineResponse::Text(text_response) => {
                    if let Ok(json) = serde_json::to_string(&ServerMessage::Text(text_response)) {
                        let _ = ws_tx_clone.send(WsMessage::Text(json));
                    }
                }
                PipelineResponse::Audio(audio_response) => {
                    if let Ok(json) = serde_json::to_string(&ServerMessage::Audio(audio_response))
                    {
                        let _ = ws_tx_clone.send(WsMessage::Text(json));
                    }
                }
                PipelineResponse::Error {
                    request_id,
                    message,
                    stage,
                } => {
                    error!("Pipeline error at {}: {}", stage, message);
                    let error_response = ErrorResponse {
                        request_id: Some(request_id),
                        message,
                        code: match stage.as_str() {
                            "STT" => ErrorCode::TranscriptionFailed,
                            "Reply" => ErrorCode::ReplyGenerationFailed,
                            "TTS" => ErrorCode::TtsFailed,
                            _ => ErrorCode::InternalError,
                        },
                    };
                    if let Ok(json) = serde_json::to_string(&ServerMessage::Error(error_response))
                    {
                        let _ = ws_tx_clone.send(WsMessage::Text(json));
                    }
                }
            }
        }
    });

    // Process incoming messages
    while let Some(msg_result) = receiver.next().await {
        match msg_result {
            Ok(WsMessage::Text(text)) => {
                debug!(
                    "Received text message from {}: {} bytes",
                    client_id,
                    text.len()
                );

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
                            let _ = ws_tx.send(WsMessage::Text(json));
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

                        // Convert history to pipeline format
                        let history = req
                            .history
                            .iter()
                            .map(|h| HistoryMessage {
                                role: h.role,
                                content: h.content.clone(),
                            })
                            .collect();

                        // Send request to STT worker via pipeline
                        let stt_request = SttRequest {
                            request_id: req.request_id,
                            audio_bytes: req.audio_bytes,
                            target_language: req.target_language,
                            history,
                            response_tx: response_tx.clone(),
                        };

                        if let Err(e) = state.pipeline.stt_tx.send(stt_request) {
                            error!("Failed to send to STT pipeline: {}", e);
                            let error_response = ErrorResponse {
                                request_id: Some(req.request_id),
                                message: "Internal pipeline error".to_string(),
                                code: ErrorCode::InternalError,
                            };
                            if let Ok(json) =
                                serde_json::to_string(&ServerMessage::Error(error_response))
                            {
                                let _ = ws_tx.send(WsMessage::Text(json));
                            }
                        } else {
                            info!("Request {} sent to pipeline", req.request_id);
                        }
                    }
                }
            }
            Ok(WsMessage::Binary(_)) => {
                warn!("Received unexpected binary message from {}", client_id);
            }
            Ok(WsMessage::Ping(data)) => {
                debug!("Received ping from {}", client_id);
                let _ = ws_tx.send(WsMessage::Pong(data));
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

