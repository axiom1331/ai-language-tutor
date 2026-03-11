mod error;
mod metrics;
mod pipeline;
mod replygen;
mod stt;
mod tts;
mod ws;

use aws_config::{BehaviorVersion, Region};
use dotenv::dotenv;
use pipeline::Pipeline;
use replygen::BedrockReplyGenerator;
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

    // Initialize AWS Bedrock client
    let aws_region = env::var("AWS_REGION").expect("AWS_REGION not set");
    info!("Initializing AWS config for region: {}", aws_region);
    let config = aws_config::defaults(BehaviorVersion::latest())
        .region(Region::new(aws_region))
        .load()
        .await;
    let bedrock_client = aws_sdk_bedrockruntime::Client::new(&config);

    // Create Bedrock reply generator
    info!("Creating Bedrock reply generator with model: eu.amazon.nova-lite-v1:0");
    let reply_generator = BedrockReplyGenerator::new(bedrock_client, "eu.amazon.nova-lite-v1:0");

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
    let stt_provider = CartesiaSttProvider::new(stt_config);

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

    // Wrap providers in Arc for sharing across threads
    let stt_provider = Arc::new(stt_provider);
    let reply_generator = Arc::new(reply_generator);

    // Create and start the processing pipeline with worker threads
    info!("Initializing parallel processing pipeline (STT -> Reply -> TTS)");
    let pipeline = Pipeline::new(stt_provider, reply_generator, tts_provider);

    // Create the WebSocket application
    info!("Creating WebSocket server");
    let app = create_app::<CartesiaSttProvider, BedrockReplyGenerator, CartesiaTtsProvider>(pipeline);

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
