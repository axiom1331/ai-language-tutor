use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, error, info, instrument};

use crate::{
    error::AssistantError,
    metrics::AnalysisMetrics,
    replygen::{GenerationResponse, Message, ReplyGenerator, Role},
};
use super::intent::Intent;
use super::intent_classifier::IntentClassifier;
use super::openai_intent_classifier::OpenAiIntentClassifier;

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
    usage: Option<Usage>,
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
struct Usage {
    prompt_tokens: Option<u32>,
    completion_tokens: Option<u32>,
}

#[derive(Deserialize)]
struct ModelOutput {
    reply: String,
    original_language_translated_reply: String,
    corrections: Option<String>,
    tip: Option<String>,
}

// ── Implementation ────────────────────────────────────────────────────────────

/// Language-learning reply generator backed by the OpenAI Chat Completions API.
/// Uses a two-stage approach:
/// 1. Fast intent classification with a lightweight model (e.g. gpt-5.4-nano)
/// 2. Intent-specific response generation with a mid-tier model (e.g. gpt-5.4-mini)
pub struct OpenAiReplyGenerator {
    client: Client,
    api_key: String,
    model_id: String,
    intent_classifier: Arc<dyn IntentClassifier>,
}

impl OpenAiReplyGenerator {
    /// Create a new reply generator.
    /// `model_id` — model used for generation (e.g. "gpt-5.4-mini").
    /// `classifier_model_id` — model used for intent classification (e.g. "gpt-5.4-nano").
    pub fn new(
        api_key: impl Into<String>,
        model_id: impl Into<String>,
        classifier_model_id: impl Into<String>,
    ) -> Self {
        let api_key = api_key.into();
        let intent_classifier = Arc::new(OpenAiIntentClassifier::new(
            api_key.clone(),
            classifier_model_id,
        ));

        Self {
            client: Client::new(),
            api_key,
            model_id: model_id.into(),
            intent_classifier,
        }
    }

    fn build_system_prompt(target_language: &str, intent: Intent) -> String {
        let template = match intent {
            Intent::Conversation => include_str!("prompts/conversation_prompt.txt"),
            Intent::GrammarQuestion => include_str!("prompts/grammar_question_prompt.txt"),
            Intent::ConceptExplanation => include_str!("prompts/concept_explanation_prompt.txt"),
            Intent::TranslationRequest => include_str!("prompts/translation_request_prompt.txt"),
        };
        template.replace("{target_language}", target_language)
    }
}

#[async_trait]
impl ReplyGenerator for OpenAiReplyGenerator {
    #[instrument(skip(self, history), fields(model_id = %self.model_id, target_language, message_count = history.len()))]
    async fn generate(
        &self,
        target_language: &str,
        history: &[Message],
    ) -> Result<GenerationResponse, AssistantError> {
        let start_time = Instant::now();
        let mut metrics = AnalysisMetrics::new(target_language.to_string(), history.len());

        // Step 1: Classify intent using lightweight model
        info!("Starting intent classification");
        let intent = self.intent_classifier.classify(history).await?;
        info!(intent = %intent.as_str(), "Intent classified");

        // Step 2: Build messages with intent-specific system prompt
        info!("Starting generation request with intent-specific prompt");

        let mut messages = vec![ChatMessage {
            role: "system".to_string(),
            content: Self::build_system_prompt(target_language, intent),
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
            max_completion_tokens: 1024,
            response_format: ResponseFormat {
                format_type: "json_object",
            },
        };

        info!("Invoking OpenAI model");
        let api_start = Instant::now();

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

        metrics.api_call_duration = api_start.elapsed();

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            error!(status = %status, body = %body, "OpenAI API error");
            return Err(AssistantError::OpenAi(format!("API error {}: {}", status, body)));
        }

        debug!("Parsing OpenAI response");
        let parse_start = Instant::now();

        let chat_response: ChatResponse = response.json().await.map_err(|e| {
            error!(error = %e, "Failed to deserialize OpenAI response");
            AssistantError::OpenAi(e.to_string())
        })?;

        if let Some(usage) = chat_response.usage {
            metrics.input_tokens = usage.prompt_tokens;
            metrics.output_tokens = usage.completion_tokens;
        }

        let text = chat_response
            .choices
            .into_iter()
            .next()
            .map(|c| c.message.content)
            .ok_or_else(|| {
                error!("OpenAI response contained no choices");
                AssistantError::EmptyResponse
            })?;

        debug!(response_text = %text, "Parsing model output JSON");

        let output: ModelOutput = serde_json::from_str(&text).map_err(|e| {
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
