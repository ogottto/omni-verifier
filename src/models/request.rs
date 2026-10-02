use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum VerificationMode {
    /// Fast: Run primary model. If error/timeout/429, failover to secondary model.
    #[default]
    Standard,
    /// Recommended: Run primary model. If confidence < 0.85 or verdict is INCONCLUSIVE, invoke secondary model.
    SmartEscalation,
    /// High-stakes: Run primary and secondary models concurrently. Disagreements are flagged as DISPUTED.
    Consensus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    /// Universal OpenAI-compatible format: DeepSeek v4 Flash, OpenAI (GPT-4o), Groq, vLLM, LocalAI, Ollama v1
    #[serde(alias = "deepseek", alias = "openai", alias = "groq", alias = "vllm")]
    OpenaiCompatible,
    /// Google Gemini 1.5 Flash / Pro, Gemini 2.0 Flash
    Gemini,
    /// Anthropic Claude 3.5 Sonnet / Haiku
    Anthropic,
    /// Native Ollama /api/chat vision protocol
    Ollama,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelEndpointConfig {
    pub provider: ProviderKind,
    pub model_name: String,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub detail: Option<String>,
    #[serde(default)]
    pub max_tokens: Option<u32>,
    #[serde(default)]
    pub reasoning_effort: Option<String>,
}

impl ModelEndpointConfig {
    pub fn resolved_base_url(&self) -> &str {
        if let Some(url) = &self.base_url {
            url.as_str()
        } else {
            match self.provider {
                ProviderKind::OpenaiCompatible => "https://api.deepseek.com",
                ProviderKind::Gemini => "https://generativelanguage.googleapis.com",
                ProviderKind::Anthropic => "https://api.anthropic.com",
                ProviderKind::Ollama => "http://localhost:11434",
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ImageSource {
    DataUrl(String),
    Base64 {
        data: String,
        mime_type: Option<String>,
    },
    Url {
        url: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationRequest {
    /// Verification prompt or task description (e.g. "Verify if both pictures depict the exact same car")
    pub prompt: String,
    /// Primary model to run
    pub primary_model: ModelEndpointConfig,
    /// Optional secondary model for failover, smart escalation, or dual consensus
    #[serde(default)]
    pub secondary_model: Option<ModelEndpointConfig>,
    /// Verification policy mode: standard (default), smart_escalation, or consensus
    #[serde(default)]
    pub mode: VerificationMode,
    /// List of image sources (Base64, data URI, or URL)
    #[serde(default)]
    pub images: Vec<ImageSource>,
    /// Optional custom system prompt overriding default verification guidelines
    #[serde(default)]
    pub system_prompt: Option<String>,
    /// Optional confidence threshold for smart escalation (default 0.85)
    #[serde(default)]
    pub escalation_threshold: Option<f32>,
}
