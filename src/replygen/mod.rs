mod interface;
mod bedrock_impl;

pub use interface::{ReplyGenerator, Message, Role, GenerationResponse};
pub use bedrock_impl::BedrockReplyGenerator;
