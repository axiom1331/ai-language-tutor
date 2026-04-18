use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Instant;
use tracing::{debug, error, info, instrument};

use crate::error::AssistantError;
use super::intent::Intent;
use super::{Message, Role};
use super::intent_classifier::IntentClassifier;

/// Intent classifier backed by the OpenAI Chat Completions API.
/// Uses a lightweight model (e.g. gpt-5.4-nano) for fast, cheap classification.
pub struct OpenAiIntentClassifier {
    client: Client,
    api_key: String,
    model_id: String,
}

impl OpenAiIntentClassifier {
    pub fn new(api_key: impl Into<String>, model_id: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            api_key: api_key.into(),
            model_id: model_id.into(),
        }
    }
}

// ── Request / response shapes ─────────────────────────────────────────────────

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    max_completion_tokens: u32,
    response_format: ResponseFormat,
}

#[derive(Serialize)]
struct ResponseFormat {
    #[serde(rename = "type")]
    format_type: &'static str,
}

#[derive(Serialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: AssistantMessage,
}

#[derive(Deserialize)]
struct AssistantMessage {
    content: String,
}

#[derive(Deserialize)]
struct ClassificationOutput {
    intent: Intent,
}

// ── Implementation ────────────────────────────────────────────────────────────

#[async_trait]
impl IntentClassifier for OpenAiIntentClassifier {
    #[instrument(skip(self, history), fields(model_id = %self.model_id, message_count = history.len()))]
    async fn classify(&self, history: &[Message]) -> Result<Intent, AssistantError> {
        let start_time = Instant::now();

        info!("Starting intent classification via OpenAI");

        let mut messages = vec![ChatMessage {
            role: "system".to_string(),
            content: include_str!("prompts/intent_classifier_prompt.txt").to_string(),
        }];

        messages.extend(history.iter().map(|m| ChatMessage {
            role: match m.role {
                Role::User => "user",
                Role::Assistant => "assistant",
            }
            .to_string(),
            content: m.content.clone(),
        }));

        let request_body = ChatRequest {
            model: self.model_id.clone(),
            messages,
            max_completion_tokens: 50,
            response_format: ResponseFormat {
                format_type: "json_object",
            },
        };

        debug!("Sending intent classification request to OpenAI");

        let response = self
            .client
            .post("https://api.openai.com/v1/chat/completions")
            .bearer_auth(&self.api_key)
            .json(&request_body)
            .send()
            .await
            .map_err(|e| {
                error!(error = %e, "OpenAI API request failed");
                AssistantError::OpenAi(e.to_string())
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            error!(status = %status, body = %body, "OpenAI API error during intent classification");
            return Err(AssistantError::OpenAi(format!("API error {}: {}", status, body)));
        }

        let chat_response: ChatResponse = response.json().await.map_err(|e| {
            error!(error = %e, "Failed to deserialize OpenAI response");
            AssistantError::OpenAi(e.to_string())
        })?;

        let text = chat_response
            .choices
            .into_iter()
            .next()
            .map(|c| c.message.content)
            .ok_or_else(|| {
                error!("OpenAI response contained no choices");
                AssistantError::EmptyResponse
            })?;

        debug!(response_text = %text, "Parsing classification output JSON");

        let output: ClassificationOutput = serde_json::from_str(&text).map_err(|e| {
            error!(
                error = %e,
                model_text = %text,
                "Failed to parse classification output JSON"
            );
            AssistantError::ParseResponse(e.to_string())
        })?;

        let elapsed = start_time.elapsed();
        info!(
            intent = %output.intent.as_str(),
            duration_ms = elapsed.as_millis(),
            "Intent classification completed"
        );

        Ok(output.intent)
    }
}
