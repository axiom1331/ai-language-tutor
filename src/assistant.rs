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

/// The response produced by the assistant after analyzing user input.
#[derive(Debug)]
pub struct LearningResponse {
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

/// Core interface for a language-learning AI assistant.
///
/// Implementations are responsible for sending conversation history to a
/// backing model and returning a structured [`LearningResponse`].
#[async_trait]
pub trait LearningAssistant: Send + Sync {
    /// Analyze the latest user message given the full conversation history
    /// and return a pedagogically enriched response.
    async fn analyze(
        &self,
        target_language: &str,
        history: &[Message],
    ) -> Result<LearningResponse, AssistantError>;
}