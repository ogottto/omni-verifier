use super::{MultimodalProvider, parse_verdict_json};
use crate::error::AppError;
use crate::models::{request::ModelEndpointConfig, response::VerificationVerdict};
use crate::services::image_processor::ProcessedImage;
use async_trait::async_trait;
use reqwest::Client;
use serde_json::{Value, json};
use tracing::{debug, error, info};

pub struct AnthropicProvider {
    client: Client,
}

impl AnthropicProvider {
    pub fn new(client: Client) -> Self {
        Self { client }
    }
}

#[async_trait]
impl MultimodalProvider for AnthropicProvider {
    fn name(&self) -> &'static str {
        "anthropic"
    }

    async fn verify(
        &self,
        config: &ModelEndpointConfig,
        system_prompt: &str,
        user_prompt: &str,
        images: &[ProcessedImage],
    ) -> Result<VerificationVerdict, AppError> {
        let base_url = config.resolved_base_url().trim_end_matches('/');
        let url = format!("{}/v1/messages", base_url);
        let api_key = config.api_key.as_deref().unwrap_or("").trim();

        if api_key.is_empty() {
            return Err(AppError::Provider {
                provider: self.name().to_string(),
                message: "Anthropic API key is not configured. Set ANTHROPIC_API_KEY in .env or pass api_key in request".to_string(),
                status_code: Some(401),
            });
        }

        debug!(
            provider = self.name(),
            model = %config.model_name,
            images_count = images.len(),
            "Sending verification request to Anthropic API"
        );

        let mut content = vec![json!({ "type": "text", "text": user_prompt })];

        for (idx, img) in images.iter().enumerate() {
            content.push(json!({
                "type": "image",
                "source": {
                    "type": "base64",
                    "media_type": img.mime_type,
                    "data": img.raw_base64
                }
            }));
            content.push(json!({
                "type": "text",
                "text": format!("[Image {} resolution: {}x{} px]", idx + 1, img.width, img.height)
            }));
        }

        let body = json!({
            "model": config.model_name,
            "max_tokens": 4096,
            "system": system_prompt,
            "messages": [
                {
                    "role": "user",
                    "content": content
                }
            ],
            "temperature": 0.1
        });

        let resp = self
            .client
            .post(&url)
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::Provider {
                provider: config.model_name.clone(),
                message: format!("Anthropic connection error: {}", e),
                status_code: None,
            })?;

        let status = resp.status();
        let resp_json: Value = resp.json().await.map_err(|e| AppError::Provider {
            provider: config.model_name.clone(),
            message: format!("Failed to parse Anthropic response JSON: {}", e),
            status_code: Some(status.as_u16()),
        })?;

        if !status.is_success() {
            let err_msg = resp_json["error"]["message"]
                .as_str()
                .unwrap_or("Unknown Anthropic API error");

            error!(
                provider = self.name(),
                model = %config.model_name,
                status = ?status,
                error = %err_msg,
                "Anthropic API returned error"
            );

            return Err(AppError::Provider {
                provider: config.model_name.clone(),
                message: format!("Anthropic error ({}): {}", status, err_msg),
                status_code: Some(status.as_u16()),
            });
        }

        let text = resp_json["content"][0]["text"]
            .as_str()
            .ok_or_else(|| AppError::Provider {
                provider: config.model_name.clone(),
                message: "Missing content[0].text in Anthropic response".to_string(),
                status_code: Some(status.as_u16()),
            })?;

        info!(
            provider = self.name(),
            model = %config.model_name,
            "Successfully received response from Anthropic API"
        );

        parse_verdict_json(text, &config.model_name)
    }
}
