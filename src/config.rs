use crate::models::request::ProviderKind;
use std::env;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub host: String,
    pub port: u16,
    pub max_payload_size_mb: usize,
    pub max_images_per_request: usize,
    pub default_deepseek_key: Option<String>,
    pub default_gemini_key: Option<String>,
    pub default_openai_key: Option<String>,
    pub default_anthropic_key: Option<String>,
    pub default_ollama_base_url: Option<String>,
    pub default_deepseek_base_url: Option<String>,
    pub default_openai_base_url: Option<String>,
    pub default_vllm_base_url: Option<String>,
    pub default_gemini_base_url: Option<String>,
    pub default_anthropic_base_url: Option<String>,
}

impl AppConfig {
    pub fn from_env() -> Self {
        dotenvy::dotenv().ok();

        let host = env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
        let port = env::var("PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(8099);
        let max_payload_size_mb = env::var("MAX_PAYLOAD_SIZE_MB")
            .ok()
            .and_then(|m| m.parse().ok())
            .unwrap_or(50);
        let max_images_per_request = env::var("MAX_IMAGES_PER_REQUEST")
            .ok()
            .and_then(|m| m.parse().ok())
            .unwrap_or(10);

        let default_deepseek_key = env::var("DEEPSEEK_API_KEY").ok().filter(|s| !s.is_empty());
        let default_gemini_key = env::var("GEMINI_API_KEY").ok().filter(|s| !s.is_empty());
        let default_openai_key = env::var("OPENAI_API_KEY").ok().filter(|s| !s.is_empty());
        let default_anthropic_key = env::var("ANTHROPIC_API_KEY").ok().filter(|s| !s.is_empty());

        let default_ollama_base_url = env::var("OLLAMA_BASE_URL").ok().filter(|s| !s.is_empty());
        let default_deepseek_base_url =
            env::var("DEEPSEEK_BASE_URL").ok().filter(|s| !s.is_empty());
        let default_openai_base_url = env::var("OPENAI_BASE_URL").ok().filter(|s| !s.is_empty());
        let default_vllm_base_url = env::var("VLLM_BASE_URL").ok().filter(|s| !s.is_empty());
        let default_gemini_base_url = env::var("GEMINI_BASE_URL").ok().filter(|s| !s.is_empty());
        let default_anthropic_base_url = env::var("ANTHROPIC_BASE_URL")
            .ok()
            .filter(|s| !s.is_empty());

        Self {
            host,
            port,
            max_payload_size_mb,
            max_images_per_request,
            default_deepseek_key,
            default_gemini_key,
            default_openai_key,
            default_anthropic_key,
            default_ollama_base_url,
            default_deepseek_base_url,
            default_openai_base_url,
            default_vllm_base_url,
            default_gemini_base_url,
            default_anthropic_base_url,
        }
    }

    pub fn get_key_for_provider(&self, provider: &ProviderKind) -> Option<&str> {
        match provider {
            ProviderKind::OpenaiCompatible => self
                .default_deepseek_key
                .as_deref()
                .or(self.default_openai_key.as_deref()),
            ProviderKind::Gemini => self.default_gemini_key.as_deref(),
            ProviderKind::Anthropic => self.default_anthropic_key.as_deref(),
            ProviderKind::Ollama => None,
        }
    }

    pub fn get_base_url_for_provider(&self, provider: &ProviderKind) -> Option<&str> {
        match provider {
            ProviderKind::Ollama => self.default_ollama_base_url.as_deref(),
            ProviderKind::OpenaiCompatible => self
                .default_deepseek_base_url
                .as_deref()
                .or(self.default_openai_base_url.as_deref())
                .or(self.default_vllm_base_url.as_deref()),
            ProviderKind::Gemini => self.default_gemini_base_url.as_deref(),
            ProviderKind::Anthropic => self.default_anthropic_base_url.as_deref(),
        }
    }
}
