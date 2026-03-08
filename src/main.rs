mod assistant;
mod bedrock;
mod cartesia;
mod error;
mod metrics;
mod tts;

use aws_config::BehaviorVersion;
use assistant::{LearningAssistant, Message, Role};
use bedrock::BedrockLearningAssistant;
use cartesia::{CartesiaTtsProvider, CartesiaConfig};
use dotenv::dotenv;
use metrics::SessionMetrics;
use std::env;
use std::fs;
use std::path::Path;
use std::time::Instant;
use tracing::{info, error};
use tracing_subscriber;
use tts::TtsProvider;

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
    info!("Initializing AWS config for region: eu-central-1");
    let config = aws_config::defaults(BehaviorVersion::latest()).region("eu-central-1").load().await;
    let client = aws_sdk_bedrockruntime::Client::new(&config);

    info!("Creating Bedrock assistant with model: eu.amazon.nova-lite-v1:0");
    let assistant = BedrockLearningAssistant::new(
        client,
        "eu.amazon.nova-lite-v1:0",
    );

    info!("Setting up Cartesia TTS provider");
    let tts_config = CartesiaConfig {
        api_key: env::var("CARTESIA_API_KEY").expect("CARTESIA_API_KEY not set"),
        voice_id: env::var("CARTESIA_VOICE_ID").unwrap_or_else(|_| "default-voice".to_string()),
        model_id: env::var("CARTESIA_MODEL_ID").unwrap_or_else(|_| "sonic-multilingual".to_string()),
        speed: env::var("CARTESIA_SPEED")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1.0),
        output_format: env::var("CARTESIA_OUTPUT_FORMAT").unwrap_or_else(|_| "wav".to_string()),
    };
    let tts_provider = CartesiaTtsProvider::new(tts_config);

    // Create output directory
    let output_dir = "tts_output";
    if !Path::new(output_dir).exists() {
        info!("Creating output directory: {}", output_dir);
        fs::create_dir_all(output_dir).expect("Failed to create output directory");
    }

    let history = vec![Message {
        role: Role::User,
        content: "Hola! Tu eres muy interesante. Yo estoy cansado hoy.".to_string(),
    }];

    let target_language = "es";
    let session_start = Instant::now();

    info!("Analyzing user message for language: {}", target_language);
    match assistant.analyze(target_language, &history).await {
        Ok(response) => {
            info!("Successfully received response from assistant");
            println!("\n=== Assistant Response ===");
            println!("Reply:       {}", response.reply);
            println!("Original Language Reply: {}", response.original_language_translated_reply);

            if let Some(c) = &response.corrections {
                println!("Corrections: {c}");
            }
            if let Some(t) = &response.tip {
                println!("Tip:         {t}");
            }

            let analysis_metrics = response.metrics;
            let has_corrections = response.corrections.is_some();
            let has_tip = response.tip.is_some();

            // Synthesize speech from the reply
            info!("Synthesizing speech for reply");
            match tts_provider.synthesize(&response.reply, target_language).await {
                Ok(tts_result) => {
                    // Generate filename with timestamp
                    let timestamp = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs();
                    let filename = format!("{}/reply_{}.wav", output_dir, timestamp);

                    info!("Saving audio to: {}", filename);
                    match fs::write(&filename, &tts_result.audio_bytes) {
                        Ok(_) => {
                            println!("Audio saved to: {}", filename);
                            info!("Audio file saved successfully");
                        }
                        Err(e) => {
                            error!("Failed to save audio file: {}", e);
                            eprintln!("Failed to save audio: {}", e);
                        }
                    }

                    // Display session metrics
                    let session_metrics = SessionMetrics {
                        analysis: analysis_metrics,
                        tts: Some(tts_result.metrics),
                        total_duration: session_start.elapsed(),
                        has_corrections,
                        has_tip,
                    };
                    session_metrics.display();
                }
                Err(e) => {
                    error!("Failed to synthesize speech: {}", e);
                    eprintln!("TTS Error: {}", e);

                    // Display metrics even if TTS failed
                    let session_metrics = SessionMetrics {
                        analysis: analysis_metrics,
                        tts: None,
                        total_duration: session_start.elapsed(),
                        has_corrections,
                        has_tip,
                    };
                    session_metrics.display();
                }
            }
        }
        Err(e) => {
            error!("Failed to analyze message: {}", e);
            eprintln!("Error: {e}");
        }
    }
}