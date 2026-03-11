use async_trait::async_trait;
use aws_sdk_bedrockruntime::{primitives::Blob, Client};
use serde::{Deserialize, Serialize};
use std::time::Instant;
use tracing::{debug, error, info, instrument};

use crate::error::AssistantError;
use super::intent::Intent;
use super::{Message, Role};

/// Trait for intent classification from user messages.
#[async_trait]
pub trait IntentClassifier: Send + Sync {
    /// Classify the user's intent based on the conversation history.
    async fn classify(&self, history: &[Message]) -> Result<Intent, AssistantError>;
}

/// Intent classifier backed by Amazon Bedrock using a lightweight model.
pub struct BedrockIntentClassifier {
    client: Client,
    model_id: String,
}

impl BedrockIntentClassifier {
    /// Create a new intent classifier using the supplied Bedrock client.
    /// Uses a lightweight model (nova-micro) for fast, cheap classification.
    pub fn new(client: Client) -> Self {
        Self {
            client,
            model_id: "eu.amazon.nova-micro-v1:0".to_string(),
        }
    }

    fn build_system_prompt() -> &'static str {
        include_str!("prompts/intent_classifier_prompt.txt")
    }
}

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
struct ClassificationOutput {
    intent: Intent,
}

#[async_trait]
impl IntentClassifier for BedrockIntentClassifier {
    #[instrument(skip(self, history), fields(model_id = %self.model_id, message_count = history.len()))]
    async fn classify(&self, history: &[Message]) -> Result<Intent, AssistantError> {
        let start_time = Instant::now();

        info!("Starting intent classification");
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
                text: Self::build_system_prompt().to_string(),
            }],
            messages,
            inference_config: InferenceConfig {
                max_tokens: 50,
            },
        };

        debug!("Serializing request body");
        let body_bytes = serde_json::to_vec(&request_body).map_err(|e| {
            error!(error = %e, "Failed to serialize request body");
            e
        })?;

        info!("Invoking Bedrock model for intent classification");
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
                error!(error = ?e, "Bedrock API error during intent classification");
                AssistantError::Bedrock(e.to_string())
            })?;

        debug!("Parsing Bedrock response");
        let raw = response.body().as_ref().to_vec();
        let bedrock_resp: BedrockResponse = serde_json::from_slice(&raw).map_err(|e| {
            error!(
                error = %e,
                raw_response = %String::from_utf8_lossy(&raw),
                "Failed to deserialize Bedrock response"
            );
            e
        })?;

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

        debug!(response_text = %text, "Parsing classification output JSON");
        let output: ClassificationOutput = serde_json::from_str(&text)
            .map_err(|e| {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classification_output_deserialization() {
        let json = r#"{"intent": "conversation"}"#;
        let output: ClassificationOutput = serde_json::from_str(json).unwrap();
        assert_eq!(output.intent, Intent::Conversation);
    }

    #[test]
    fn test_classification_output_all_intents() {
        let test_cases = vec![
            (r#"{"intent": "conversation"}"#, Intent::Conversation),
            (r#"{"intent": "grammar_question"}"#, Intent::GrammarQuestion),
            (r#"{"intent": "concept_explanation"}"#, Intent::ConceptExplanation),
            (r#"{"intent": "translation_request"}"#, Intent::TranslationRequest),
        ];

        for (json, expected) in test_cases {
            let output: ClassificationOutput = serde_json::from_str(json).unwrap();
            assert_eq!(output.intent, expected);
        }
    }
}
