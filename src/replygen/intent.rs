use serde::{Deserialize, Serialize};

/// Represents the user's intent for the current message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Intent {
    /// General conversation practice in the target language
    Conversation,
    /// Specific grammar-related questions
    GrammarQuestion,
    /// Request for explanation of language concepts
    ConceptExplanation,
    /// Request to translate something
    TranslationRequest,
}

impl Intent {
    /// Returns the name of the intent as a string.
    pub fn as_str(&self) -> &'static str {
        match self {
            Intent::Conversation => "conversation",
            Intent::GrammarQuestion => "grammar_question",
            Intent::ConceptExplanation => "concept_explanation",
            Intent::TranslationRequest => "translation_request",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_intent_serialization() {
        let intent = Intent::Conversation;
        let json = serde_json::to_string(&intent).unwrap();
        assert_eq!(json, "\"conversation\"");
    }

    #[test]
    fn test_intent_deserialization() {
        let json = "\"grammar_question\"";
        let intent: Intent = serde_json::from_str(json).unwrap();
        assert_eq!(intent, Intent::GrammarQuestion);
    }

    #[test]
    fn test_intent_as_str() {
        assert_eq!(Intent::Conversation.as_str(), "conversation");
        assert_eq!(Intent::GrammarQuestion.as_str(), "grammar_question");
        assert_eq!(Intent::ConceptExplanation.as_str(), "concept_explanation");
        assert_eq!(Intent::TranslationRequest.as_str(), "translation_request");
    }
}
