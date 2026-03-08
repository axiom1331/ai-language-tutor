use async_trait::async_trait;
use crate::error::AssistantError;
use crate::metrics::AnalysisMetrics;

/// A message exchanged in a learning session.
#[derive(Debug, Clone)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

/// The role of a conversation participant.
#[derive(Debug, Clone, PartialEq)]
pub enum Role {
    User,
    Assistant,
}

/// The response produced by the reply generator after analyzing user input.
#[derive(Debug)]
pub struct GenerationResponse {
    /// The assistant's reply in the target language and/or explanation.
    pub reply: String,
    /// The assistant's reply in the original language of the user.
    pub original_language_translated_reply: String,
    /// Optional corrections to the user's input.
    pub corrections: Option<String>,
    /// Optional vocabulary or grammar tip surfaced from the input.
    pub tip: Option<String>,
    /// Metrics about the analysis operation.
    pub metrics: AnalysisMetrics,
}

/// Core interface for a language-learning reply generator.
///
/// Implementations are responsible for sending conversation history to a
/// backing model and returning a structured [`GenerationResponse`].
#[async_trait]
pub trait ReplyGenerator: Send + Sync {
    /// Analyze the latest user message given the full conversation history
    /// and return a pedagogically enriched response.
    async fn generate(
        &self,
        target_language: &str,
        history: &[Message],
    ) -> Result<GenerationResponse, AssistantError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// Mock implementation of ReplyGenerator for testing.
    struct MockReplyGenerator {
        should_fail: bool,
        reply: String,
        original_reply: String,
        corrections: Option<String>,
        tip: Option<String>,
    }

    impl MockReplyGenerator {
        fn success() -> Self {
            Self {
                should_fail: false,
                reply: "Hola! Estoy bien, gracias.".to_string(),
                original_reply: "Hello! I'm fine, thank you.".to_string(),
                corrections: Some("Use '¿Cómo estás?' instead of 'Como estas'".to_string()),
                tip: Some("Remember to use question marks at the beginning and end".to_string()),
            }
        }

        fn failure() -> Self {
            Self {
                should_fail: true,
                reply: String::new(),
                original_reply: String::new(),
                corrections: None,
                tip: None,
            }
        }
    }

    #[async_trait]
    impl ReplyGenerator for MockReplyGenerator {
        async fn generate(
            &self,
            target_language: &str,
            history: &[Message],
        ) -> Result<GenerationResponse, AssistantError> {
            if self.should_fail {
                return Err(AssistantError::EmptyResponse);
            }

            let metrics = AnalysisMetrics {
                total_duration: Duration::from_millis(150),
                api_call_duration: Duration::from_millis(100),
                parse_duration: Duration::from_millis(50),
                input_tokens: Some(10),
                output_tokens: Some(50),
                message_count: history.len(),
                target_language: target_language.to_string(),
            };

            Ok(GenerationResponse {
                reply: self.reply.clone(),
                original_language_translated_reply: self.original_reply.clone(),
                corrections: self.corrections.clone(),
                tip: self.tip.clone(),
                metrics,
            })
        }
    }

    #[tokio::test]
    async fn test_mock_reply_generator_success() {
        let generator = MockReplyGenerator::success();
        let history = vec![Message {
            role: Role::User,
            content: "Hola! Como estas?".to_string(),
        }];

        let result = generator.generate("es", &history).await;
        assert!(result.is_ok());

        let response = result.unwrap();
        assert_eq!(response.reply, "Hola! Estoy bien, gracias.");
        assert_eq!(response.original_language_translated_reply, "Hello! I'm fine, thank you.");
        assert!(response.corrections.is_some());
        assert!(response.tip.is_some());
        assert_eq!(response.metrics.message_count, 1);
        assert_eq!(response.metrics.target_language, "es");
    }

    #[tokio::test]
    async fn test_mock_reply_generator_failure() {
        let generator = MockReplyGenerator::failure();
        let history = vec![Message {
            role: Role::User,
            content: "Hello".to_string(),
        }];

        let result = generator.generate("fr", &history).await;
        assert!(result.is_err());

        match result {
            Err(AssistantError::EmptyResponse) => {},
            _ => panic!("Expected EmptyResponse error"),
        }
    }

    #[test]
    fn test_message_creation() {
        let message = Message {
            role: Role::User,
            content: "Test message".to_string(),
        };

        assert_eq!(message.role, Role::User);
        assert_eq!(message.content, "Test message");
    }

    #[test]
    fn test_role_equality() {
        assert_eq!(Role::User, Role::User);
        assert_eq!(Role::Assistant, Role::Assistant);
        assert_ne!(Role::User, Role::Assistant);
    }

    #[test]
    fn test_message_clone() {
        let original = Message {
            role: Role::Assistant,
            content: "Original".to_string(),
        };

        let cloned = original.clone();
        assert_eq!(cloned.role, original.role);
        assert_eq!(cloned.content, original.content);
    }
}
