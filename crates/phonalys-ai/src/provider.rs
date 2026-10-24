//! Types communs aux moteurs d'inférence locaux.

use serde::{Deserialize, Serialize};

/// Familles d'API prises en charge. Toutes doivent pointer vers un hôte validé par
/// [`crate::EndpointPolicy`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    /// API native d'Ollama (`/api/chat`).
    Ollama,
    /// API compatible OpenAI servie localement : llama.cpp, LM Studio, vLLM, LocalAI, Jan…
    OpenAiCompatibleLocal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: Role,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(default)]
    pub temperature: Option<f32>,
}
