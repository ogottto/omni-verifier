use super::{MultimodalProvider, parse_verdict_json};
use crate::error::AppError;
use crate::models::{request::ModelEndpointConfig, response::VerificationVerdict};
use crate::services::image_processor::ProcessedImage;
use async_trait::async_trait;
use reqwest::Client;
use serde_json::{Value, json};
use tracing::{debug, error, info};

pub struct OllamaNativeProvider {
    client: Client,
}

impl OllamaNativeProvider {
    pub fn new(client: Client) -> Self {
        Self { client }
    }
}

#[async_trait]
impl MultimodalProvider for OllamaNativeProvider {
    fn name(&self) -> &'static str {
        "ollama_native"
    }

    async fn verify(
        &self,
        config: &ModelEndpointConfig,
        system_prompt: &str,
        user_prompt: &str,
        images: &[ProcessedImage],
    ) -> Result<VerificationVerdict, AppError> {
        let base_url = config.resolved_base_url().trim_end_matches('/');
        let url = format!("{}/api/chat", base_url);

        debug!(
            provider = self.name(),
            model = %config.model_name,
            endpoint = %url,
            images_count = images.len(),
            "Sending verification request to native Ollama endpoint"
        );

        let image_payloads: Vec<String> = images.iter().map(|img| img.raw_base64.clone()).collect();

        let body = json!({
            "model": config.model_name,
            "format": "json",
            "stream": false,
            "messages": [
                {
                    "role": "system",
                    "content": system_prompt
                },
                {
                    "role": "user",
                    "content": user_prompt,
                    "images": image_payloads
                }
            ]
        });

        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::Provider {
                provider: config.model_name.clone(),
                message: format!("Ollama connection error: {}", e),
                status_code: None,
            })?;

        let status = resp.status();
        let resp_json: Value = resp.json().await.map_err(|e| AppError::Provider {
            provider: config.model_name.clone(),
            message: format!("Failed to parse Ollama response: {}", e),
            status_code: Some(status.as_u16()),
        })?;

        if !status.is_success() {
            let err_msg = resp_json["error"]
                .as_str()
                .unwrap_or("Unknown Ollama error");

            error!(
                provider = self.name(),
                model = %config.model_name,
                status = ?status,
                error = %err_msg,
                "Ollama returned error"
            );

            return Err(AppError::Provider {
                provider: config.model_name.clone(),
                message: format!("Ollama error ({}): {}", status, err_msg),
                status_code: Some(status.as_u16()),
            });
        }

        let content =
            resp_json["message"]["content"]
                .as_str()
                .ok_or_else(|| AppError::Provider {
                    provider: config.model_name.clone(),
                    message: "Missing message.content in Ollama response".to_string(),
                    status_code: Some(status.as_u16()),
                })?;

        info!(
            provider = self.name(),
            model = %config.model_name,
            "Successfully received response from Ollama"
        );

        parse_verdict_json(content, &config.model_name)
    }
}
