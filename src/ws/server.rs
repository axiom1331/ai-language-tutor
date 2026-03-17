use crate::auth::{create_jwt, verify_credentials, verify_jwt};
use crate::db::ConversationRepository;
use crate::pipeline::Pipeline;
use crate::replygen::ReplyGenerator;
use crate::stt::SttProvider;
use crate::tts::TtsProvider;
use crate::ws::handler::{handle_websocket, WsState};
use std::sync::Arc;
use axum::{
    extract::{Query, State, WebSocketUpgrade},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer};
use tracing::Level;

/// Create the Axum application with WebSocket routes
pub fn create_app<S, R, T>(
    pipeline: Pipeline,
    conversation_repo: Arc<ConversationRepository>,
) -> Router
where
    S: SttProvider + 'static,
    R: ReplyGenerator + 'static,
    T: TtsProvider + 'static,
{
    let state = WsState {
        pipeline,
        conversation_repo,
    };

    // Configure CORS to allow WebSocket connections from any origin
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // Configure request tracing
    let trace = TraceLayer::new_for_http()
        .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
        .on_response(DefaultOnResponse::new().level(Level::INFO));

    Router::new()
        .route("/ws", get(ws_handler))
        .route("/health", get(health_handler))
        .route("/api/login", post(login_handler))
        .layer(cors)
        .layer(trace)
        .with_state(state)
}

#[derive(Debug, Deserialize)]
struct WsQuery {
    token: String,
}

/// WebSocket upgrade handler with JWT verification
async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<WsState>,
    Query(query): Query<WsQuery>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    // Verify JWT token
    verify_jwt(&query.token).map_err(|e| {
        (StatusCode::UNAUTHORIZED, format!("Authentication failed: {}", e))
    })?;

    Ok(ws.on_upgrade(move |socket| handle_websocket(socket, state)))
}

/// Health check endpoint
async fn health_handler() -> impl IntoResponse {
    "OK"
}

#[derive(Debug, Deserialize)]
struct LoginRequest {
    username: String,
    password: String,
}

#[derive(Debug, Serialize)]
struct LoginResponse {
    token: String,
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
}

/// Login endpoint
async fn login_handler(
    Json(credentials): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Verify credentials
    verify_credentials(&credentials.username, &credentials.password).map_err(|e| {
        (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
    })?;

    // Create JWT token
    let token = create_jwt(&credentials.username).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
    })?;

    Ok(Json(LoginResponse { token }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::{AssistantError, SttError, TtsError};
    use crate::metrics::{AnalysisMetrics, SttMetrics, TtsMetrics};
    use crate::replygen::{GenerationResponse, Message, ReplyGenerator};
    use crate::stt::{SttProvider, SttResult};
    use crate::tts::{TtsProvider, TtsResult};
    use async_trait::async_trait;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use std::sync::Arc;
    use std::time::Duration;
    use tower::ServiceExt;

    struct MockSttProvider;

    #[async_trait]
    impl SttProvider for MockSttProvider {
        async fn transcribe(
            &self,
            audio_bytes: &[u8],
            language: &str,
        ) -> Result<SttResult, SttError> {
            Ok(SttResult {
                text: "test".to_string(),
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
                reply: "test reply".to_string(),
                original_language_translated_reply: "test reply".to_string(),
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
                audio_bytes: vec![],
                metrics: TtsMetrics {
                    total_duration: Duration::from_millis(150),
                    api_call_duration: Duration::from_millis(130),
                    audio_size_bytes: 0,
                    text_length: text.len(),
                    target_language: target_language.to_string(),
                    output_format: "wav".to_string(),
                },
            })
        }
    }

    #[tokio::test]
    #[ignore] // Requires database connection
    async fn test_health_endpoint() {
        let stt = Arc::new(MockSttProvider);
        let reply_gen = Arc::new(MockReplyGenerator);
        let tts = Arc::new(MockTtsProvider);
        let pipeline = crate::pipeline::Pipeline::new(stt, reply_gen, tts);

        // Create a mock database pool for testing
        let db_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://postgres:postgres@localhost/test".to_string());
        let db_pool = crate::db::pool::create_pool(&db_url).await.unwrap();
        let conversation_repo = Arc::new(crate::db::ConversationRepository::new(db_pool));

        let app = create_app::<MockSttProvider, MockReplyGenerator, MockTtsProvider>(
            pipeline,
            conversation_repo,
        );

        let request = Request::builder()
            .uri("/health")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    #[ignore] // Requires database connection
    async fn test_ws_endpoint_exists() {
        let stt = Arc::new(MockSttProvider);
        let reply_gen = Arc::new(MockReplyGenerator);
        let tts = Arc::new(MockTtsProvider);
        let pipeline = crate::pipeline::Pipeline::new(stt, reply_gen, tts);

        // Create a mock database pool for testing
        let db_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://postgres:postgres@localhost/test".to_string());
        let db_pool = crate::db::pool::create_pool(&db_url).await.unwrap();
        let conversation_repo = Arc::new(crate::db::ConversationRepository::new(db_pool));

        let app = create_app::<MockSttProvider, MockReplyGenerator, MockTtsProvider>(
            pipeline,
            conversation_repo,
        );

        // Test that the /ws route exists (without proper headers it will fail, but not 404)
        let request = Request::builder()
            .uri("/ws")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        // Should not be 404 (route exists)
        assert_ne!(response.status(), StatusCode::NOT_FOUND);
    }
}
