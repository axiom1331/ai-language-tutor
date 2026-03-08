use std::time::Duration;

/// Metrics for LLM analysis operations.
#[derive(Debug, Clone)]
pub struct AnalysisMetrics {
    /// Total time from request to response
    pub total_duration: Duration,
    /// Time spent on network/API call
    pub api_call_duration: Duration,
    /// Time spent parsing and processing response
    pub parse_duration: Duration,
    /// Number of tokens in input (if available)
    pub input_tokens: Option<u32>,
    /// Number of tokens in output (if available)
    pub output_tokens: Option<u32>,
    /// Number of messages in conversation history
    pub message_count: usize,
    /// Target language being taught
    pub target_language: String,
}

impl AnalysisMetrics {
    pub fn new(target_language: String, message_count: usize) -> Self {
        Self {
            total_duration: Duration::default(),
            api_call_duration: Duration::default(),
            parse_duration: Duration::default(),
            input_tokens: None,
            output_tokens: None,
            message_count,
            target_language,
        }
    }
}

/// Metrics for TTS synthesis operations.
#[derive(Debug, Clone)]
pub struct TtsMetrics {
    /// Total time from request to receiving audio bytes
    pub total_duration: Duration,
    /// Time spent on network/API call
    pub api_call_duration: Duration,
    /// Size of generated audio in bytes
    pub audio_size_bytes: usize,
    /// Length of input text in characters
    pub text_length: usize,
    /// Target language for synthesis
    pub target_language: String,
    /// Audio format
    pub output_format: String,
}

impl TtsMetrics {
    pub fn new(text_length: usize, target_language: String, output_format: String) -> Self {
        Self {
            total_duration: Duration::default(),
            api_call_duration: Duration::default(),
            audio_size_bytes: 0,
            text_length,
            target_language,
            output_format,
        }
    }
}

/// Metrics for STT transcription operations.
#[derive(Debug, Clone)]
pub struct SttMetrics {
    /// Total time from request to receiving transcription
    pub total_duration: Duration,
    /// Time spent on network/API call
    pub api_call_duration: Duration,
    /// Size of input audio in bytes
    pub audio_size_bytes: usize,
    /// Length of transcribed text in characters
    pub transcript_length: usize,
    /// Language of the audio
    pub language: String,
}

impl SttMetrics {
    pub fn new(audio_size_bytes: usize, language: String) -> Self {
        Self {
            total_duration: Duration::default(),
            api_call_duration: Duration::default(),
            audio_size_bytes,
            transcript_length: 0,
            language,
        }
    }
}

/// Aggregated metrics for a complete learning interaction.
#[derive(Debug, Clone)]
pub struct SessionMetrics {
    /// Metrics from the LLM analysis
    pub analysis: AnalysisMetrics,
    /// Metrics from TTS synthesis (if performed)
    pub tts: Option<TtsMetrics>,
    /// Total end-to-end duration
    pub total_duration: Duration,
    /// Whether corrections were provided
    pub has_corrections: bool,
    /// Whether a tip was provided
    pub has_tip: bool,
}

impl SessionMetrics {
    /// Calculate throughput metrics for analysis.
    pub fn analysis_tokens_per_second(&self) -> Option<f64> {
        self.analysis.output_tokens.map(|tokens| {
            tokens as f64 / self.analysis.api_call_duration.as_secs_f64()
        })
    }

    /// Calculate audio generation rate (bytes per second).
    pub fn tts_bytes_per_second(&self) -> Option<f64> {
        self.tts.as_ref().map(|tts| {
            tts.audio_size_bytes as f64 / tts.api_call_duration.as_secs_f64()
        })
    }

    /// Calculate characters per second of TTS processing.
    pub fn tts_chars_per_second(&self) -> Option<f64> {
        self.tts.as_ref().map(|tts| {
            tts.text_length as f64 / tts.total_duration.as_secs_f64()
        })
    }

    /// Pretty print the metrics.
    pub fn display(&self) {
        println!("\n=== Session Metrics ===");
        println!("Total Duration: {:.2}s", self.total_duration.as_secs_f64());

        println!("\n--- LLM Analysis ---");
        println!("  Language: {}", self.analysis.target_language);
        println!("  Message Count: {}", self.analysis.message_count);
        println!("  Total: {:.2}s", self.analysis.total_duration.as_secs_f64());
        println!("  API Call: {:.2}s", self.analysis.api_call_duration.as_secs_f64());
        println!("  Parsing: {:.2}s", self.analysis.parse_duration.as_secs_f64());

        if let Some(input_tokens) = self.analysis.input_tokens {
            println!("  Input Tokens: {}", input_tokens);
        }
        if let Some(output_tokens) = self.analysis.output_tokens {
            println!("  Output Tokens: {}", output_tokens);
        }
        if let Some(tps) = self.analysis_tokens_per_second() {
            println!("  Tokens/sec: {:.2}", tps);
        }

        println!("  Has Corrections: {}", self.has_corrections);
        println!("  Has Tip: {}", self.has_tip);

        if let Some(tts) = &self.tts {
            println!("\n--- TTS Synthesis ---");
            println!("  Language: {}", tts.target_language);
            println!("  Format: {}", tts.output_format);
            println!("  Total: {:.2}s", tts.total_duration.as_secs_f64());
            println!("  API Call: {:.2}s", tts.api_call_duration.as_secs_f64());
            println!("  Text Length: {} chars", tts.text_length);
            println!("  Audio Size: {} bytes ({:.2} KB)", tts.audio_size_bytes, tts.audio_size_bytes as f64 / 1024.0);

            if let Some(bps) = self.tts_bytes_per_second() {
                println!("  Throughput: {:.2} KB/s", bps / 1024.0);
            }
            if let Some(cps) = self.tts_chars_per_second() {
                println!("  Processing: {:.2} chars/s", cps);
            }
        }

        println!("======================\n");
    }
}
