# Integration Tests for STT Module

## Overview

This directory contains integration tests for the Speech-to-Text (STT) module, specifically testing the Cartesia STT WebSocket streaming implementation.

## Test Files

- **`stt_integration_test.rs`** - Main integration tests for Cartesia STT
- **`testaudio_es_1.wav`** - Spanish test audio file (1.6 MB)

## Prerequisites

1. **Environment Variables**: You need to set `CARTESIA_API_KEY` in your `.env` file or environment:
   ```bash
   CARTESIA_API_KEY=your_api_key_here
   ```

2. **Test Audio**: The test audio file `testaudio_es_1.wav` should be present in the `tests/` directory.

## Running the Tests

### Run All Unit Tests (Non-ignored)
```bash
cargo test
```

### Run Helper Function Tests Only
```bash
cargo test --test stt_integration_test helpers
```

### Run Integration Tests (Requires API Key)
```bash
# Run all integration tests
cargo test --test stt_integration_test -- --ignored

# Run a specific integration test
cargo test --test stt_integration_test test_cartesia_stt_spanish_audio -- --ignored

# Run with output
cargo test --test stt_integration_test -- --ignored --nocapture
```

## Test Cases

### 1. `test_cartesia_stt_spanish_audio`
**Purpose**: End-to-end transcription test with expected text comparison.

**What it tests**:
- Loads the Spanish audio file
- Sends it to Cartesia STT API via WebSocket
- Compares transcription with expected text
- Verifies >80% similarity

**Expected Transcript**:
> "He leído su anuncio. Necesito ayuda urgente. Vivo solo desde que mi esposa me dejó. Y desde hace tres meses, cada mañana encuentro los muebles de mi casa en lugares diferentes. He cambiado las cerraduras, he puesto nuevos cerrojos, incluso he dormido en el sofá para vigilar... pero sigue pasando."

### 2. `test_cartesia_stt_with_word_timestamps`
**Purpose**: Verify word-level timestamp functionality.

**What it tests**:
- Word timestamps are returned
- Timestamps are in chronological order
- Each word's start time < end time
- No overlapping timestamps

### 3. `test_cartesia_stt_metrics`
**Purpose**: Validate metrics accuracy and consistency.

**What it tests**:
- Total duration > 0
- API call duration > 0
- Total duration >= API call duration
- Audio size matches input
- Transcript length matches output

## Helper Functions

The test suite includes utility functions:

- **`load_test_audio(filename)`** - Loads audio file from tests directory
- **`normalize_text(text)`** - Normalizes text for comparison (lowercase, trim whitespace)
- **`calculate_similarity(text1, text2)`** - Simple word overlap calculation

## Test Configuration

The tests use the following Cartesia STT configuration:

```rust
CartesiaConfig {
    api_key: env::var("CARTESIA_API_KEY"),
    model_id: "ink-whisper",
    language: "es",
    encoding: "pcm_s16le",
    sample_rate: 16000,
    min_volume: 0.1,
    max_silence_duration_secs: 0.5,
}
```

## Best Practices Implemented

✅ **Integration tests are ignored by default** - Won't run on `cargo test` without `--ignored`
✅ **Clear error messages** - Tests fail with descriptive messages
✅ **Comprehensive assertions** - Tests verify all aspects of the API response
✅ **Helper functions tested** - Unit tests for utility functions
✅ **Proper file organization** - Test files separate from source code
✅ **Environment configuration** - Uses dotenv for API keys
✅ **Detailed output** - Prints results for manual verification

## Troubleshooting

### Test Audio File Not Found
```
Error: Failed to read test audio file: tests/testaudio_es_1.wav
```
**Solution**: Ensure `testaudio_es_1.wav` is in the `tests/` directory.

### Missing API Key
```
Error: CARTESIA_API_KEY not set
```
**Solution**: Add `CARTESIA_API_KEY=your_key` to `.env` file in project root.

### Low Similarity Score
```
Assertion failed: Transcription similarity too low: 65%. Expected > 80%
```
**Solution**: This may indicate:
- Audio quality issues
- Wrong language setting
- API model changes
- Expected transcript mismatch

Review the printed output to see actual vs expected transcripts.

## Adding New Tests

To add new integration tests:

1. Add your test audio file to `tests/` directory
2. Create a new test function with `#[tokio::test]` and `#[ignore]`
3. Use helper functions for loading audio and comparing results
4. Follow the existing test patterns

Example:
```rust
#[tokio::test]
#[ignore]
async fn test_my_new_audio() {
    dotenv::dotenv().ok();
    let api_key = env::var("CARTESIA_API_KEY").expect("CARTESIA_API_KEY not set");
    let audio_bytes = load_test_audio("my_audio.wav");

    // ... test implementation
}
```
