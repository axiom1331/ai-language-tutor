mod auth;
mod db;
mod error;
mod metrics;
mod pipeline;
mod replygen;
mod stt;
mod tts;
mod ws;

use aws_config::{BehaviorVersion, Region};
use db::pool::{create_pool, run_migrations};
use db::ConversationRepository;
use dotenv::dotenv;
use pipeline::Pipeline;
use replygen::{BedrockReplyGenerator, OpenAiReplyGenerator};
use stt::{CartesiaConfig as SttCartesiaConfig, CartesiaSttProvider};
use std::env;
use std::sync::Arc;
use tracing::info;
use tracing_subscriber;
use tts::{CartesiaConfig as TtsCartesiaConfig, CartesiaTtsProvider};
use ws::create_app;

#[tokio::main]
async fn main() {
    dotenv().ok();

    // Initialize tracing subscriber with default log level INFO
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"))
        )
        .init();

    // Initialize database connection pool
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL not set");
    let db_pool = create_pool(&database_url)
        .await
        .expect("Failed to create database pool");

    // Run migrations
    run_migrations(&db_pool)
        .await
        .expect("Failed to run database migrations");

    // Create conversation repository
    let conversation_repo = Arc::new(ConversationRepository::new(db_pool));

    // Create Cartesia STT provider
    info!("Setting up Cartesia STT provider");
    let stt_config = SttCartesiaConfig {
        api_key: env::var("CARTESIA_API_KEY").expect("CARTESIA_API_KEY not set"),
        model_id: env::var("CARTESIA_STT_MODEL_ID")
            .unwrap_or_else(|_| "ink-whisper".to_string()),
        language: env::var("DEFAULT_LANGUAGE").unwrap_or_else(|_| "en".to_string()),
        encoding: env::var("AUDIO_ENCODING").unwrap_or_else(|_| "pcm_s16le".to_string()),
        sample_rate: env::var("SAMPLE_RATE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(16000),
        min_volume: env::var("MIN_VOLUME")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.1),
        max_silence_duration_secs: env::var("MAX_SILENCE_DURATION")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.5),
    };
    let stt_provider = Arc::new(CartesiaSttProvider::new(stt_config));

    // Create Cartesia TTS provider
    info!("Setting up Cartesia TTS provider");
    let tts_config = TtsCartesiaConfig {
        api_key: env::var("CARTESIA_API_KEY").expect("CARTESIA_API_KEY not set"),
        version: env::var("CARTESIA_VERSION").expect("CARTESIA_VERSION not set"),
        voice_id: env::var("CARTESIA_VOICE_ID")
            .unwrap_or_else(|_| "default-voice".to_string()),
        model_id: env::var("CARTESIA_MODEL_ID")
            .unwrap_or_else(|_| "sonic-multilingual".to_string()),
        speed: env::var("CARTESIA_SPEED")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1.0),
        output_format: env::var("CARTESIA_OUTPUT_FORMAT")
            .unwrap_or_else(|_| "wav".to_string()),
    };
    let tts_provider = Arc::new(CartesiaTtsProvider::new(tts_config));

    // Select LLM provider via LLM_PROVIDER env var ("openai" or "bedrock", defaults to "bedrock")
    let llm_provider = env::var("LLM_PROVIDER").unwrap_or_else(|_| "bedrock".to_string());
    info!("LLM provider: {}", llm_provider);

    let app = match llm_provider.as_str() {
        "openai" => {
            let openai_api_key = env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY not set");
            let reply_model = env::var("OPENAI_REPLY_MODEL")
                .unwrap_or_else(|_| "gpt-5.4-mini".to_string());
            let classifier_model = env::var("OPENAI_CLASSIFIER_MODEL")
                .unwrap_or_else(|_| "gpt-5.4-nano".to_string());
            info!(
                "Creating OpenAI reply generator (reply: {}, classifier: {})",
                reply_model, classifier_model
            );
            let reply_generator = Arc::new(OpenAiReplyGenerator::new(
                openai_api_key,
                reply_model,
                classifier_model,
            ));
            let pipeline = Pipeline::new(stt_provider, reply_generator, tts_provider);
            info!("Creating WebSocket server");
            create_app::<CartesiaSttProvider, OpenAiReplyGenerator, CartesiaTtsProvider>(
                pipeline,
                conversation_repo,
            )
        }
        _ => {
            let aws_region = env::var("AWS_REGION").expect("AWS_REGION not set");
            info!("Initializing AWS config for region: {}", aws_region);
            let config = aws_config::defaults(BehaviorVersion::latest())
                .region(Region::new(aws_region))
                .load()
                .await;
            let bedrock_client = aws_sdk_bedrockruntime::Client::new(&config);
            info!("Creating Bedrock reply generator with model: eu.amazon.nova-lite-v1:0");
            let reply_generator =
                Arc::new(BedrockReplyGenerator::new(bedrock_client, "eu.amazon.nova-lite-v1:0"));
            let pipeline = Pipeline::new(stt_provider, reply_generator, tts_provider);
            info!("Creating WebSocket server");
            create_app::<CartesiaSttProvider, BedrockReplyGenerator, CartesiaTtsProvider>(
                pipeline,
                conversation_repo,
            )
        }
    };

    // Start the server
    let addr = env::var("LISTEN_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string());
    info!("Starting WebSocket server on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("Failed to bind to address");

    info!("WebSocket server listening on {}", addr);
    info!("WebSocket endpoint: ws://{}/ws", addr);
    info!("Health check endpoint: http://{}/health", addr);

    axum::serve(listener, app)
        .await
        .expect("Failed to start server");
}
