use super::{MultimodalProvider, parse_verdict_json};
use crate::error::AppError;
use crate::models::{request::ModelEndpointConfig, response::VerificationVerdict};
use crate::services::image_processor::ProcessedImage;
use async_trait::async_trait;
use reqwest::Client;
use serde_json::{Value, json};
use tracing::{debug, error, info};

pub struct OpenAiCompatibleProvider {
    client: Client,
}

impl OpenAiCompatibleProvider {
    pub fn new(client: Client) -> Self {
        Self { client }
    }
}

#[async_trait]
impl MultimodalProvider for OpenAiCompatibleProvider {
    fn name(&self) -> &'static str {
        "openai_compatible"
    }

    async fn verify(
        &self,
        config: &ModelEndpointConfig,
        system_prompt: &str,
        user_prompt: &str,
        images: &[ProcessedImage],
    ) -> Result<VerificationVerdict, AppError> {
        let base_url = config.resolved_base_url().trim_end_matches('/');
        let url = format!("{}/chat/completions", base_url);

        if (base_url.contains("deepseek.com") || base_url.contains("openai.com"))
            && config.api_key.as_deref().unwrap_or("").trim().is_empty()
        {
            return Err(AppError::Provider {
                provider: self.name().to_string(),
                message: format!(
                    "API key is required for cloud provider at '{}'. Configure in .env or pass api_key in request",
                    base_url
                ),
                status_code: Some(401),
            });
        }

        debug!(
            provider = self.name(),
            model = %config.model_name,
            endpoint = %url,
            images_count = images.len(),
            "Sending verification request to OpenAI-compatible provider"
        );

        let mut content_parts = vec![json!({
            "type": "text",
            "text": user_prompt
        })];

        let detail_level = config.detail.as_deref().unwrap_or("high");

        for (idx, img) in images.iter().enumerate() {
            content_parts.push(json!({
                "type": "image_url",
                "image_url": {
                    "url": img.data_url,
                    "detail": detail_level
                }
            }));
            content_parts.push(json!({
                "type": "text",
                "text": format!("[Image {} details: {}x{} px]", idx + 1, img.width, img.height)
            }));
        }

        let messages = vec![
            json!({
                "role": "system",
                "content": system_prompt
            }),
            json!({
                "role": "user",
                "content": content_parts
            }),
        ];

        let max_tokens = config.max_tokens.unwrap_or(16384);
        let mut body = json!({
            "model": config.model_name,
            "messages": messages,
            "response_format": { "type": "json_object" },
            "temperature": 0.1,
            "max_tokens": max_tokens
        });

        if let Some(effort) = &config.reasoning_effort {
            body["reasoning_effort"] = json!(effort);
        }

        let mut req = self.client.post(&url).json(&body);

        if let Some(key) = &config.api_key
            && !key.trim().is_empty()
        {
            req = req.header("Authorization", format!("Bearer {}", key.trim()));
        }

        let resp = req.send().await.map_err(|e| AppError::Provider {
            provider: config.model_name.clone(),
            message: format!("HTTP connection error: {}", e),
            status_code: None,
        })?;

        let status = resp.status();
        let resp_json: Value = resp.json().await.map_err(|e| AppError::Provider {
            provider: config.model_name.clone(),
            message: format!("Failed to parse response body as JSON: {}", e),
            status_code: Some(status.as_u16()),
        })?;

        if !status.is_success() {
            let err_msg = resp_json["error"]["message"]
                .as_str()
                .or_else(|| resp_json["message"].as_str())
                .unwrap_or("Unknown provider error");

            error!(
                provider = self.name(),
                model = %config.model_name,
                status = ?status,
                error = %err_msg,
                "Provider returned API error"
            );

            return Err(AppError::Provider {
                provider: config.model_name.clone(),
                message: format!("API error ({}): {}", status, err_msg),
                status_code: Some(status.as_u16()),
            });
        }

        let content_opt = resp_json["choices"][0]["message"]["content"].as_str();
        let finish_reason = resp_json["choices"][0]["finish_reason"]
            .as_str()
            .unwrap_or_default();

        let content = if let Some(c) = content_opt
            && !c.trim().is_empty()
        {
            c
        } else if let Some(reasoning) =
            resp_json["choices"][0]["message"]["reasoning_content"].as_str()
            && reasoning.contains('{')
            && reasoning.contains('}')
        {
            reasoning
        } else if finish_reason == "length" {
            return Err(AppError::Provider {
                provider: config.model_name.clone(),
                message: "Provider reached max token limit during reasoning without generating response body".to_string(),
                status_code: Some(status.as_u16()),
            });
        } else {
            content_opt.ok_or_else(|| AppError::Provider {
                provider: config.model_name.clone(),
                message: "Missing choices[0].message.content in provider response".to_string(),
                status_code: Some(status.as_u16()),
            })?
        };

        info!(
            provider = self.name(),
            model = %config.model_name,
            "Successfully received response from OpenAI-compatible provider"
        );

        parse_verdict_json(content, &config.model_name)
    }
}
