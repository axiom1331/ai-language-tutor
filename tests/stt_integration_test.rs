use language_learning_ai_assistant::stt::{CartesiaConfig, CartesiaSttProvider, SttProvider};
use std::env;
use std::fs;
use std::path::PathBuf;
use tracing_subscriber;

/// Helper function to load test audio file
fn load_test_audio(filename: &str) -> Vec<u8> {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests");
    path.push(filename);

    fs::read(&path).unwrap_or_else(|e| {
        panic!(
            "Failed to read test audio file: {}. Error: {}",
            path.display(),
            e
        )
    })
}

/// Helper function to normalize text for comparison
/// Removes extra whitespace and converts to lowercase
fn normalize_text(text: &str) -> String {
    text.to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Helper function to calculate similarity between two strings (simple word overlap)
fn calculate_similarity(text1: &str, text2: &str) -> f64 {
    let words1: Vec<_> = text1.split_whitespace().collect();
    let words2: Vec<_> = text2.split_whitespace().collect();

    if words1.is_empty() || words2.is_empty() {
        return 0.0;
    }

    let matches = words1
        .iter()
        .filter(|w| words2.contains(w))
        .count();

    matches as f64 / words1.len().max(words2.len()) as f64
}

#[tokio::test]
#[ignore] // Run with: cargo test --test stt_integration_test -- --ignored
async fn test_cartesia_stt_spanish_audio() {
    // Initialize tracing for debugging
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_test_writer()
        .try_init();

    // Load environment variables
    dotenv::dotenv().ok();

    // Get API key from environment
    let api_key = env::var("CARTESIA_API_KEY").expect(
        "CARTESIA_API_KEY not set. Please set it in .env file or environment variables.",
    );

    // Load test audio file
    let audio_bytes = load_test_audio("test_audio.raw");
    println!("Loaded audio file: {} bytes", audio_bytes.len());

    // Expected transcription
    let expected_transcript = "He leído su anuncio. Necesito ayuda urgente. Vivo solo desde que mi esposa me dejó. Y desde hace tres meses, cada mañana encuentro los muebles de mi casa en lugares diferentes. He cambiado las cerraduras, he puesto nuevos cerrojos, incluso he dormido en el sofá para vigilar... pero sigue pasando.";

    // Configure Cartesia STT provider
    let config = CartesiaConfig {
        api_key,
        model_id: "ink-whisper".to_string(),
        language: "es".to_string(),
        encoding: "pcm_s16le".to_string(),
        sample_rate: 16000,
        min_volume: 0.1,
        max_silence_duration_secs: 1.0,
    };

    let provider = CartesiaSttProvider::new(config);

    // Perform transcription
    println!("Starting transcription...");
    let result = provider
        .transcribe(&audio_bytes, "es")
        .await
        .expect("Transcription failed");

    // Print results
    println!("\n=== Transcription Results ===");
    println!("Transcript: {}", result.text);
    println!("Is Final: {}", result.is_final);
    println!("Duration: {:.2}s", result.duration);
    println!("Language: {}", result.language);
    println!("Request ID: {}", result.request_id);

    if let Some(words) = &result.words {
        println!("\nWord-level timestamps ({} words):", words.len());
        for (i, word) in words.iter().take(10).enumerate() {
            println!(
                "  {}: '{}' [{:.2}s - {:.2}s]",
                i + 1,
                word.word,
                word.start,
                word.end
            );
        }
        if words.len() > 10 {
            println!("  ... and {} more words", words.len() - 10);
        }
    }

    println!("\n=== Metrics ===");
    println!(
        "Total Duration: {:.2}s",
        result.metrics.total_duration.as_secs_f64()
    );
    println!(
        "API Call Duration: {:.2}s",
        result.metrics.api_call_duration.as_secs_f64()
    );
    println!("Audio Size: {} bytes", result.metrics.audio_size_bytes);
    println!("Transcript Length: {} chars", result.metrics.transcript_length);
    println!("Language: {}", result.metrics.language);

    // Verify basic properties
    assert!(!result.text.is_empty(), "Transcript should not be empty");
    assert_eq!(result.language, "es", "Language should be Spanish");
    assert!(
        result.metrics.audio_size_bytes > 0,
        "Audio size should be greater than 0"
    );
    assert!(
        result.metrics.transcript_length > 0,
        "Transcript length should be greater than 0"
    );

    // Normalize both texts for comparison
    let normalized_result = normalize_text(&result.text);
    let normalized_expected = normalize_text(expected_transcript);

    println!("\n=== Text Comparison ===");
    println!("Expected (normalized): {}", normalized_expected);
    println!("Received (normalized): {}", normalized_result);

    // Calculate similarity
    let similarity = calculate_similarity(&normalized_result, &normalized_expected);
    println!("\nSimilarity: {:.2}%", similarity * 100.0);

    // Assert high similarity (>80%)
    assert!(
        similarity > 0.8,
        "Transcription similarity too low: {:.2}%. Expected > 80%",
        similarity * 100.0
    );

    println!("\n✅ Test passed! Transcription matches expected text with {:.2}% similarity", similarity * 100.0);
}

#[tokio::test]
#[ignore] // Run with: cargo test --test stt_integration_test -- --ignored
async fn test_cartesia_stt_with_word_timestamps() {
    dotenv::dotenv().ok();

    let api_key = env::var("CARTESIA_API_KEY").expect("CARTESIA_API_KEY not set");
    let audio_bytes = load_test_audio("testaudio_es_1.wav");

    let config = CartesiaConfig {
        api_key,
        model_id: "ink-whisper".to_string(),
        language: "es".to_string(),
        encoding: "pcm_s16le".to_string(),
        sample_rate: 16000,
        min_volume: 0.1,
        max_silence_duration_secs: 0.5,
    };

    let provider = CartesiaSttProvider::new(config);
    let result = provider.transcribe(&audio_bytes, "es").await.unwrap();

    // Verify word timestamps are present
    assert!(
        result.words.is_some(),
        "Word timestamps should be available"
    );

    let words = result.words.unwrap();
    assert!(!words.is_empty(), "Word list should not be empty");

    // Verify timestamps are in order and make sense
    for i in 0..words.len() - 1 {
        assert!(
            words[i].end <= words[i + 1].start || words[i].end <= words[i + 1].end,
            "Word timestamps should be in chronological order"
        );
        assert!(
            words[i].start < words[i].end,
            "Word start time should be before end time"
        );
    }

    println!("✅ Word timestamps are properly structured and ordered");
}

#[tokio::test]
#[ignore] // Run with: cargo test --test stt_integration_test -- --ignored
async fn test_cartesia_stt_metrics() {
    dotenv::dotenv().ok();

    let api_key = env::var("CARTESIA_API_KEY").expect("CARTESIA_API_KEY not set");
    let audio_bytes = load_test_audio("testaudio_es_1.wav");

    let config = CartesiaConfig {
        api_key,
        model_id: "ink-whisper".to_string(),
        language: "es".to_string(),
        encoding: "pcm_s16le".to_string(),
        sample_rate: 16000,
        min_volume: 0.1,
        max_silence_duration_secs: 0.5,
    };

    let provider = CartesiaSttProvider::new(config);
    let start = std::time::Instant::now();
    let result = provider.transcribe(&audio_bytes, "es").await.unwrap();
    let elapsed = start.elapsed();

    // Verify metrics are reasonable
    assert!(
        result.metrics.total_duration.as_secs_f64() > 0.0,
        "Total duration should be greater than 0"
    );
    assert!(
        result.metrics.api_call_duration.as_secs_f64() > 0.0,
        "API call duration should be greater than 0"
    );
    assert!(
        result.metrics.total_duration >= result.metrics.api_call_duration,
        "Total duration should be >= API call duration"
    );

    // Verify metrics match actual values
    assert_eq!(
        result.metrics.audio_size_bytes,
        audio_bytes.len(),
        "Audio size in metrics should match input"
    );
    assert_eq!(
        result.metrics.transcript_length,
        result.text.len(),
        "Transcript length in metrics should match result text"
    );

    println!("✅ Metrics are accurate and consistent");
    println!(
        "   Total time: {:.2}s (measured: {:.2}s)",
        result.metrics.total_duration.as_secs_f64(),
        elapsed.as_secs_f64()
    );
}

#[cfg(test)]
mod helpers {
    use super::*;

    #[test]
    fn test_normalize_text() {
        let text = "  Hello   World!  This  is   a   TEST.  ";
        let normalized = normalize_text(text);
        assert_eq!(normalized, "hello world! this is a test.");
    }

    #[test]
    fn test_calculate_similarity_identical() {
        let text1 = "hello world";
        let text2 = "hello world";
        let similarity = calculate_similarity(text1, text2);
        assert_eq!(similarity, 1.0);
    }

    #[test]
    fn test_calculate_similarity_partial() {
        let text1 = "hello world";
        let text2 = "hello there";
        let similarity = calculate_similarity(text1, text2);
        assert!(similarity > 0.0 && similarity < 1.0);
    }

    #[test]
    fn test_calculate_similarity_no_match() {
        let text1 = "hello world";
        let text2 = "foo bar";
        let similarity = calculate_similarity(text1, text2);
        assert_eq!(similarity, 0.0);
    }
}
