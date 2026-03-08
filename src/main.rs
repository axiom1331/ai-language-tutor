mod error;
mod metrics;
mod replygen;
mod tts;
mod tutor;

use aws_config::{BehaviorVersion, Region};
use dotenv::dotenv;
use replygen::{BedrockReplyGenerator, Message, Role};
use std::env;
use std::fs;
use std::path::Path;
use tracing::{error, info};
use tracing_subscriber;
use tts::{CartesiaConfig, CartesiaTtsProvider};
use tutor::LanguageTutor;

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

    let aws_region = env::var("AWS_REGION").expect("AWS_REGION not set");
    info!("Initializing AWS config for region: {}", aws_region);
    let config = aws_config::defaults(BehaviorVersion::latest())
        .region(Region::new(aws_region))
        .load()
        .await;
    let client = aws_sdk_bedrockruntime::Client::new(&config);

    info!("Creating Bedrock reply generator with model: eu.amazon.nova-lite-v1:0");
    let reply_generator = BedrockReplyGenerator::new(client, "eu.amazon.nova-lite-v1:0");

    info!("Setting up Cartesia TTS provider");
    let tts_config = CartesiaConfig {
        api_key: env::var("CARTESIA_API_KEY").expect("CARTESIA_API_KEY not set"),
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
    let tts_provider = CartesiaTtsProvider::new(tts_config);

    // Create the language tutor
    info!("Creating language tutor");
    let tutor = LanguageTutor::new(reply_generator, tts_provider);

    // Create output directory
    let output_dir = "tts_output";
    if !Path::new(output_dir).exists() {
        info!("Creating output directory: {}", output_dir);
        fs::create_dir_all(output_dir).expect("Failed to create output directory");
    }

    let history = vec![Message {
        role: Role::User,
        content: "Hola! Como estas hoy?".to_string(),
    }];

    let target_language = "es";

    info!("Processing user message");
    match tutor.process(target_language, &history).await {
        Ok(response) => {
            info!("Successfully processed user message");
            println!("\n=== Tutor Response ===");
            println!("Reply:       {}", response.reply);
            println!(
                "Original Language Reply: {}",
                response.original_language_translated_reply
            );

            if let Some(c) = &response.corrections {
                println!("Corrections: {c}");
            }
            if let Some(t) = &response.tip {
                println!("Tip:         {t}");
            }

            // Generate filename with timestamp
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            let filename = format!("{}/reply_{}.wav", output_dir, timestamp);

            info!("Saving audio to: {}", filename);
            match fs::write(&filename, &response.audio_bytes) {
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
            response.metrics.display();
        }
        Err(e) => {
            error!("Failed to process message: {}", e);
            eprintln!("Error: {e}");
        }
    }
}
