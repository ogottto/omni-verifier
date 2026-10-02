pub mod anthropic;
pub mod gemini;
pub mod ollama;
pub mod openai_compatible;

use crate::error::AppError;
use crate::models::{
    request::ModelEndpointConfig,
    response::{VerdictStatus, VerificationVerdict},
};
use crate::services::image_processor::ProcessedImage;
use async_trait::async_trait;
use serde_json::Value;
use tracing::{debug, error};

#[async_trait]
pub trait MultimodalProvider: Send + Sync {
    fn name(&self) -> &'static str;

    async fn verify(
        &self,
        config: &ModelEndpointConfig,
        system_prompt: &str,
        user_prompt: &str,
        images: &[ProcessedImage],
    ) -> Result<VerificationVerdict, AppError>;
}

/// Helper function to parse model response into strongly typed VerificationVerdict,
/// gracefully cleaning markdown ```json fences if the LLM wrapped it.
pub fn parse_verdict_json(
    raw_text: &str,
    model_name: &str,
) -> Result<VerificationVerdict, AppError> {
    let mut cleaned = raw_text.trim();
    if let Some(start_idx) = cleaned.find("```json") {
        let sub = &cleaned[start_idx + 7..];
        if let Some(end_idx) = sub.find("```") {
            cleaned = sub[..end_idx].trim();
        }
    } else if let Some(start_idx) = cleaned.find("```") {
        let sub = &cleaned[start_idx + 3..];
        if let Some(end_idx) = sub.find("```") {
            cleaned = sub[..end_idx].trim();
        }
    } else if let Some(start_idx) = cleaned.find('{')
        && let Some(end_idx) = cleaned.rfind('}')
        && end_idx > start_idx
    {
        cleaned = &cleaned[start_idx..=end_idx];
    }

    debug!(raw = %cleaned, "Parsing cleaned LLM response JSON");

    match serde_json::from_str::<VerificationVerdict>(cleaned) {
        Ok(mut verdict) => {
            verdict.model_used = model_name.to_string();
            Ok(verdict)
        }
        Err(e) => {
            // Attempt fallback parsing from generic Value
            if let Ok(val) = serde_json::from_str::<Value>(cleaned) {
                let verified = val["verified"].as_bool().unwrap_or(false);
                let verdict_str = val["verdict"].as_str().unwrap_or("INCONCLUSIVE");
                let confidence = val["confidence_score"]
                    .as_f64()
                    .map(|f| f as f32)
                    .unwrap_or(0.5);
                let summary = val["summary"]
                    .as_str()
                    .unwrap_or("No summary provided by model")
                    .to_string();
                let reasoning = val["detailed_reasoning"]
                    .as_str()
                    .unwrap_or(cleaned)
                    .to_string();

                let verdict_status = match verdict_str.to_uppercase().as_str() {
                    "PASS" => VerdictStatus::Pass,
                    "FAIL" => VerdictStatus::Fail,
                    _ => VerdictStatus::Inconclusive,
                };

                return Ok(VerificationVerdict {
                    verdict: verdict_status,
                    verified,
                    confidence_score: confidence,
                    summary,
                    detailed_reasoning: reasoning,
                    checks: vec![],
                    detected_entities: vec![],
                    anomalies: vec![],
                    model_used: model_name.to_string(),
                });
            }

            error!(error = %e, raw = %raw_text, "Failed to parse structured JSON from provider");
            Err(AppError::Provider {
                provider: model_name.to_string(),
                message: format!("AI returned invalid JSON: {} (Raw: {})", e, raw_text),
                status_code: None,
            })
        }
    }
}
