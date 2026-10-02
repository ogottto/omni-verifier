use super::{MultimodalProvider, parse_verdict_json};
use crate::error::AppError;
use crate::models::{request::ModelEndpointConfig, response::VerificationVerdict};
use crate::services::image_processor::ProcessedImage;
use async_trait::async_trait;
use reqwest::Client;
use serde_json::{Value, json};
use tracing::{debug, error, info};

pub struct GeminiProvider {
    client: Client,
}

impl GeminiProvider {
    pub fn new(client: Client) -> Self {
        Self { client }
    }
}

#[async_trait]
impl MultimodalProvider for GeminiProvider {
    fn name(&self) -> &'static str {
        "gemini"
    }

    async fn verify(
        &self,
        config: &ModelEndpointConfig,
        system_prompt: &str,
        user_prompt: &str,
        images: &[ProcessedImage],
    ) -> Result<VerificationVerdict, AppError> {
        let base_url = config.resolved_base_url().trim_end_matches('/');
        let api_key = config.api_key.as_deref().unwrap_or("").trim();

        if api_key.is_empty() {
            return Err(AppError::Provider {
                provider: self.name().to_string(),
                message: "Gemini API key is not configured. Set GEMINI_API_KEY in .env or pass api_key in request".to_string(),
                status_code: Some(401),
            });
        }

        let url = format!(
            "{}/v1beta/models/{}:generateContent?key={}",
            base_url, config.model_name, api_key
        );

        debug!(
            provider = self.name(),
            model = %config.model_name,
            images_count = images.len(),
            "Sending verification request to Gemini API"
        );

        let mut parts = vec![json!({ "text": user_prompt })];

        for (idx, img) in images.iter().enumerate() {
            parts.push(json!({
                "inline_data": {
                    "mime_type": img.mime_type,
                    "data": img.raw_base64
                }
            }));
            parts.push(json!({
                "text": format!("[Image {} resolution: {}x{} px]", idx + 1, img.width, img.height)
            }));
        }

        let body = json!({
            "contents": [
                {
                    "role": "user",
                    "parts": parts
                }
            ],
            "system_instruction": {
                "parts": [
                    { "text": system_prompt }
                ]
            },
            "generation_config": {
                "response_mime_type": "application/json",
                "temperature": 0.1
            }
        });

        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::Provider {
                provider: config.model_name.clone(),
                message: format!("Gemini connection error: {}", e),
                status_code: None,
            })?;

        let status = resp.status();
        let resp_json: Value = resp.json().await.map_err(|e| AppError::Provider {
            provider: config.model_name.clone(),
            message: format!("Failed to parse Gemini response JSON: {}", e),
            status_code: Some(status.as_u16()),
        })?;

        if !status.is_success() {
            let err_msg = resp_json["error"]["message"]
                .as_str()
                .unwrap_or("Unknown Gemini API error");

            error!(
                provider = self.name(),
                model = %config.model_name,
                status = ?status,
                error = %err_msg,
                "Gemini API returned error"
            );

            return Err(AppError::Provider {
                provider: config.model_name.clone(),
                message: format!("Gemini error ({}): {}", status, err_msg),
                status_code: Some(status.as_u16()),
            });
        }

        let content = resp_json["candidates"][0]["content"]["parts"][0]["text"]
            .as_str()
            .ok_or_else(|| AppError::Provider {
                provider: config.model_name.clone(),
                message: "Missing candidates[0].content.parts[0].text in Gemini response"
                    .to_string(),
                status_code: Some(status.as_u16()),
            })?;

        info!(
            provider = self.name(),
            model = %config.model_name,
            "Successfully received response from Gemini API"
        );

        parse_verdict_json(content, &config.model_name)
    }
}
