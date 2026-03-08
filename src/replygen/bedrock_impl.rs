use async_trait::async_trait;
use aws_sdk_bedrockruntime::{primitives::Blob, Client};
use serde::{Deserialize, Serialize};
use std::time::Instant;
use tracing::{error, debug, info, instrument};

use crate::{
    replygen::{ReplyGenerator, GenerationResponse, Message, Role},
    error::AssistantError,
    metrics::AnalysisMetrics,
};

#[derive(Serialize)]
struct BedrockRequest<'a> {
    system: Vec<SystemMessage>,
    messages: Vec<BedrockMessage<'a>>,
    #[serde(rename = "inferenceConfig")]
    inference_config: InferenceConfig,
}

#[derive(Serialize)]
struct SystemMessage {
    text: String,
}

#[derive(Serialize)]
struct InferenceConfig {
    #[serde(rename = "maxTokens")]
    max_tokens: u32,
}

#[derive(Serialize)]
struct BedrockMessage<'a> {
    role: &'a str,
    content: Vec<ContentItem<'a>>,
}

#[derive(Serialize)]
struct ContentItem<'a> {
    text: &'a str,
}

#[derive(Deserialize)]
struct BedrockResponse {
    output: OutputBlock,
    usage: Option<UsageInfo>,
}

#[derive(Deserialize)]
struct OutputBlock {
    message: MessageBlock,
}

#[derive(Deserialize)]
struct MessageBlock {
    content: Vec<ContentBlock>,
}

#[derive(Deserialize)]
struct ContentBlock {
    text: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsageInfo {
    input_tokens: Option<u32>,
    output_tokens: Option<u32>,
}

#[derive(Deserialize)]
struct ModelOutput {
    reply: String,
    original_language_translated_reply: String,
    corrections: Option<String>,
    tip: Option<String>,
}

// ── Implementation ───────────────────────────────────────────────────────────

/// Language-learning reply generator backed by Amazon Bedrock (Claude on Bedrock).
pub struct BedrockReplyGenerator {
    client: Client,
    model_id: String,
}

impl BedrockReplyGenerator {
    /// Create a new reply generator using the supplied Bedrock client and model ID.
    pub fn new(client: Client, model_id: impl Into<String>) -> Self {
        Self {
            client,
            model_id: model_id.into(),
        }
    }

    fn build_system_prompt(target_language: &str) -> String {
        let template = include_str!("prompts/system_prompt.txt");
        template.replace("{target_language}", target_language)
    }
}

#[async_trait]
impl ReplyGenerator for BedrockReplyGenerator {
    #[instrument(skip(self, history), fields(model_id = %self.model_id, target_language, message_count = history.len()))]
    async fn generate(
        &self,
        target_language: &str,
        history: &[Message],
    ) -> Result<GenerationResponse, AssistantError> {
        let start_time = Instant::now();
        let mut metrics = AnalysisMetrics::new(target_language.to_string(), history.len());

        info!("Starting generation request");
        let messages: Vec<BedrockMessage<'_>> = history
            .iter()
            .map(|m| BedrockMessage {
                role: match m.role {
                    Role::User => "user",
                    Role::Assistant => "assistant",
                },
                content: vec![ContentItem {
                    text: &m.content,
                }],
            })
            .collect();

        let request_body = BedrockRequest {
            system: vec![SystemMessage {
                text: Self::build_system_prompt(target_language),
            }],
            messages,
            inference_config: InferenceConfig {
                max_tokens: 1024,
            },
        };

        debug!("Serializing request body");
        let body_bytes = serde_json::to_vec(&request_body).map_err(|e| {
            error!(error = %e, "Failed to serialize request body");
            e
        })?;

        info!("Invoking Bedrock model");
        let api_start = Instant::now();
        let response = self
            .client
            .invoke_model()
            .model_id(&self.model_id)
            .content_type("application/json")
            .accept("application/json")
            .body(Blob::new(body_bytes))
            .send()
            .await
            .map_err(|e| {
                error!(error = ?e, "Bedrock API error");
                AssistantError::Bedrock(e.to_string())
            })?;
        metrics.api_call_duration = api_start.elapsed();

        debug!("Parsing Bedrock response");
        let parse_start = Instant::now();
        let raw = response.body().as_ref().to_vec();
        let bedrock_resp: BedrockResponse = serde_json::from_slice(&raw).map_err(|e| {
            error!(
                error = %e,
                raw_response = %String::from_utf8_lossy(&raw),
                "Failed to deserialize Bedrock response"
            );
            e
        })?;

        // Extract token usage if available
        if let Some(usage) = bedrock_resp.usage {
            metrics.input_tokens = usage.input_tokens;
            metrics.output_tokens = usage.output_tokens;
        }

        debug!("Extracting content from response");
        let text = bedrock_resp
            .output
            .message
            .content
            .into_iter()
            .next()
            .map(|b| b.text)
            .ok_or_else(|| {
                error!("Bedrock response contained no content blocks");
                AssistantError::EmptyResponse
            })?;

        debug!(response_text = %text, "Parsing model output JSON");
        let output: ModelOutput = serde_json::from_str(&text)
            .map_err(|e| {
                error!(
                    error = %e,
                    model_text = %text,
                    "Failed to parse model output JSON"
                );
                AssistantError::ParseResponse(e.to_string())
            })?;

        metrics.parse_duration = parse_start.elapsed();
        metrics.total_duration = start_time.elapsed();

        info!(
            has_corrections = output.corrections.is_some(),
            has_tip = output.tip.is_some(),
            total_duration_ms = metrics.total_duration.as_millis(),
            api_duration_ms = metrics.api_call_duration.as_millis(),
            "Successfully parsed generation response"
        );

        Ok(GenerationResponse {
            reply: output.reply,
            original_language_translated_reply: output.original_language_translated_reply,
            corrections: output.corrections,
            tip: output.tip,
            metrics,
        })
    }
}
