use crate::error::AppError;
use crate::models::{
    request::{
        ImageSource, ModelEndpointConfig, ProviderKind, VerificationMode, VerificationRequest,
    },
    response::{ApiResponse, VerdictStatus, VerificationVerdict},
};
use crate::providers::{
    MultimodalProvider, anthropic::AnthropicProvider, gemini::GeminiProvider,
    ollama::OllamaNativeProvider, openai_compatible::OpenAiCompatibleProvider,
};
use crate::services::{
    image_processor::{ImageProcessor, ProcessedImage},
    prompt_builder::PromptBuilder,
};
use reqwest::Client;
use std::sync::Arc;
use std::time::Instant;
use tracing::{info, warn};
use uuid::Uuid;

pub struct VerificationEngine {
    download_client: Client,
    openai_provider: Arc<OpenAiCompatibleProvider>,
    gemini_provider: Arc<GeminiProvider>,
    anthropic_provider: Arc<AnthropicProvider>,
    ollama_provider: Arc<OllamaNativeProvider>,
    app_config: Arc<crate::config::AppConfig>,
}

impl VerificationEngine {
    pub fn new() -> Self {
        Self::with_config(Arc::new(crate::config::AppConfig::from_env()))
    }

    pub fn with_config(app_config: Arc<crate::config::AppConfig>) -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(90))
            .build()
            .unwrap_or_else(|_| Client::new());

        let download_client = Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap_or_else(|_| Client::new());

        Self {
            openai_provider: Arc::new(OpenAiCompatibleProvider::new(client.clone())),
            gemini_provider: Arc::new(GeminiProvider::new(client.clone())),
            anthropic_provider: Arc::new(AnthropicProvider::new(client.clone())),
            ollama_provider: Arc::new(OllamaNativeProvider::new(client)),
            download_client,
            app_config,
        }
    }

    pub fn max_images(&self) -> usize {
        self.app_config.max_images_per_request
    }

    fn select_provider(&self, kind: &ProviderKind) -> Arc<dyn MultimodalProvider> {
        match kind {
            ProviderKind::OpenaiCompatible => self.openai_provider.clone(),
            ProviderKind::Gemini => self.gemini_provider.clone(),
            ProviderKind::Anthropic => self.anthropic_provider.clone(),
            ProviderKind::Ollama => self.ollama_provider.clone(),
        }
    }

    pub async fn process_images(
        &self,
        image_sources: &[ImageSource],
    ) -> Result<Vec<ProcessedImage>, AppError> {
        let mut processed = Vec::with_capacity(image_sources.len());

        for (idx, source) in image_sources.iter().enumerate() {
            let img = match source {
                ImageSource::DataUrl(data_url) => {
                    let _permit = ImageProcessor::acquire_permit().await?;
                    ImageProcessor::process_base64(data_url, idx + 1)?
                }
                ImageSource::Base64 { data, .. } => {
                    let _permit = ImageProcessor::acquire_permit().await?;
                    ImageProcessor::process_base64(data, idx + 1)?
                }
                ImageSource::Url { url } => {
                    ImageProcessor::fetch_and_process_url(&self.download_client, url, idx + 1)
                        .await?
                }
            };
            processed.push(img);
        }

        Ok(processed)
    }

    pub async fn verify(
        &self,
        request: VerificationRequest,
        in_memory_images: Option<Vec<ProcessedImage>>,
    ) -> Result<ApiResponse, AppError> {
        let start_time = Instant::now();
        let request_id = Uuid::new_v4().to_string();

        // 1. Process images in memory if not already processed (e.g. from multipart)
        let images = if let Some(imgs) = in_memory_images {
            imgs
        } else {
            self.process_images(&request.images).await?
        };

        if images.is_empty() {
            return Err(AppError::Validation(
                "At least one image must be provided for visual verification".to_string(),
            ));
        }

        let images_metadata = images.iter().map(|img| img.to_metadata()).collect();
        let system_prompt = PromptBuilder::build_system_prompt(request.system_prompt.as_deref());
        let user_prompt = PromptBuilder::build_user_prompt(&request.prompt, images.len());

        let mut failover_triggered = false;
        let mut escalated = false;
        let mut secondary_verdict = None;

        let final_verdict = match request.mode {
            VerificationMode::Consensus => {
                let secondary_config = request.secondary_model.as_ref().ok_or_else(|| {
                    AppError::Validation(
                        "Consensus mode requires both primary_model and secondary_model to be configured"
                            .to_string(),
                    )
                })?;

                let primary_fut = self.execute_single_model(
                    &request.primary_model,
                    &system_prompt,
                    &user_prompt,
                    &images,
                );
                let secondary_fut = self.execute_single_model(
                    secondary_config,
                    &system_prompt,
                    &user_prompt,
                    &images,
                );

                let (res_a, res_b) = tokio::join!(primary_fut, secondary_fut);

                match (res_a, res_b) {
                    (Ok(a), Ok(b)) => {
                        secondary_verdict = Some(b.clone());
                        if a.verdict == b.verdict && a.verified == b.verified {
                            let avg_confidence = (a.confidence_score + b.confidence_score) / 2.0;
                            VerificationVerdict {
                                verdict: a.verdict,
                                verified: a.verified,
                                confidence_score: avg_confidence,
                                summary: format!(
                                    "Consensus PASS: Both {} and {} agreed. {}",
                                    a.model_used, b.model_used, a.summary
                                ),
                                detailed_reasoning: format!(
                                    "Model A ({}):\n{}\n\nModel B ({}):\n{}",
                                    a.model_used,
                                    a.detailed_reasoning,
                                    b.model_used,
                                    b.detailed_reasoning
                                ),
                                checks: a.checks,
                                detected_entities: a.detected_entities,
                                anomalies: a.anomalies,
                                model_used: format!("{}+{}", a.model_used, b.model_used),
                            }
                        } else {
                            VerificationVerdict {
                                verdict: VerdictStatus::Disputed,
                                verified: false,
                                confidence_score: 0.5,
                                summary: format!(
                                    "DISPUTE DETECTED: Primary model ({}) returned {:?} while Secondary model ({}) returned {:?}.",
                                    a.model_used, a.verdict, b.model_used, b.verdict
                                ),
                                detailed_reasoning: format!(
                                    "Primary ({}) Verdict: {:?}\nReasoning: {}\n\nSecondary ({}) Verdict: {:?}\nReasoning: {}",
                                    a.model_used, a.verdict, a.detailed_reasoning,
                                    b.model_used, b.verdict, b.detailed_reasoning
                                ),
                                checks: [a.checks, b.checks].concat(),
                                detected_entities: [a.detected_entities, b.detected_entities].concat(),
                                anomalies: vec![
                                    "Model consensus failure: conflicting verdicts require human review".to_string(),
                                ],
                                model_used: format!("{}+{}", a.model_used, b.model_used),
                            }
                        }
                    }
                    (Ok(a), Err(err_b)) => {
                        warn!(error = %err_b, "Secondary model failed during consensus, falling back to primary");
                        a
                    }
                    (Err(err_a), Ok(b)) => {
                        warn!(error = %err_a, "Primary model failed during consensus, falling back to secondary");
                        failover_triggered = true;
                        b
                    }
                    (Err(err_a), Err(err_b)) => {
                        return Err(AppError::AllProvidersFailed {
                            primary_err: err_a.to_string(),
                            secondary_err: err_b.to_string(),
                        });
                    }
                }
            }

            VerificationMode::SmartEscalation => {
                let threshold = request.escalation_threshold.unwrap_or(0.85);
                let primary_res = self
                    .execute_single_model(
                        &request.primary_model,
                        &system_prompt,
                        &user_prompt,
                        &images,
                    )
                    .await;

                match primary_res {
                    Ok(verdict) => {
                        let is_borderline = verdict.confidence_score < threshold
                            || verdict.verdict == VerdictStatus::Inconclusive;

                        if is_borderline {
                            if let Some(sec_config) = &request.secondary_model {
                                info!(
                                    primary_conf = verdict.confidence_score,
                                    threshold = threshold,
                                    "Confidence below threshold or inconclusive. Escalating to secondary model"
                                );

                                match self
                                    .execute_single_model(
                                        sec_config,
                                        &system_prompt,
                                        &user_prompt,
                                        &images,
                                    )
                                    .await
                                {
                                    Ok(sec_verdict) => {
                                        escalated = true;
                                        secondary_verdict = Some(sec_verdict.clone());

                                        // Secondary model acts as referee
                                        VerificationVerdict {
                                            verdict: sec_verdict.verdict,
                                            verified: sec_verdict.verified,
                                            confidence_score: (verdict.confidence_score
                                                + sec_verdict.confidence_score)
                                                / 2.0,
                                            summary: format!(
                                                "Escalated verdict: {}. Arbiter ({}): {}",
                                                verdict.summary,
                                                sec_verdict.model_used,
                                                sec_verdict.summary
                                            ),
                                            detailed_reasoning: format!(
                                                "Initial evaluation ({}):\n{}\n\nArbiter evaluation ({}):\n{}",
                                                verdict.model_used,
                                                verdict.detailed_reasoning,
                                                sec_verdict.model_used,
                                                sec_verdict.detailed_reasoning
                                            ),
                                            checks: sec_verdict.checks,
                                            detected_entities: sec_verdict.detected_entities,
                                            anomalies: sec_verdict.anomalies,
                                            model_used: format!(
                                                "{}->{}",
                                                verdict.model_used, sec_verdict.model_used
                                            ),
                                        }
                                    }
                                    Err(e) => {
                                        warn!(error = %e, "Escalation to secondary model failed, keeping primary verdict");
                                        verdict
                                    }
                                }
                            } else {
                                verdict
                            }
                        } else {
                            verdict
                        }
                    }
                    Err(primary_err) => {
                        if let Some(sec_config) = &request.secondary_model {
                            warn!(
                                error = %primary_err,
                                "Primary model failed in SmartEscalation. Failing over to secondary model"
                            );
                            failover_triggered = true;
                            self.execute_single_model(
                                sec_config,
                                &system_prompt,
                                &user_prompt,
                                &images,
                            )
                            .await
                            .map_err(|secondary_err| {
                                AppError::AllProvidersFailed {
                                    primary_err: primary_err.to_string(),
                                    secondary_err: secondary_err.to_string(),
                                }
                            })?
                        } else {
                            return Err(primary_err);
                        }
                    }
                }
            }

            VerificationMode::Standard => {
                let primary_res = self
                    .execute_single_model(
                        &request.primary_model,
                        &system_prompt,
                        &user_prompt,
                        &images,
                    )
                    .await;

                match primary_res {
                    Ok(verdict) => verdict,
                    Err(primary_err) => {
                        if let Some(sec_config) = &request.secondary_model {
                            warn!(
                                error = %primary_err,
                                "Primary model failed. Triggering failover to secondary model"
                            );
                            failover_triggered = true;
                            self.execute_single_model(
                                sec_config,
                                &system_prompt,
                                &user_prompt,
                                &images,
                            )
                            .await
                            .map_err(|secondary_err| {
                                AppError::AllProvidersFailed {
                                    primary_err: primary_err.to_string(),
                                    secondary_err: secondary_err.to_string(),
                                }
                            })?
                        } else {
                            return Err(primary_err);
                        }
                    }
                }
            }
        };

        let elapsed = start_time.elapsed().as_millis();

        Ok(ApiResponse {
            success: true,
            request_id,
            mode: request.mode,
            verdict: final_verdict,
            secondary_verdict,
            failover_triggered,
            escalated,
            execution_time_ms: elapsed,
            images_inspected: images.len(),
            images_metadata,
        })
    }

    async fn execute_single_model(
        &self,
        config: &ModelEndpointConfig,
        system_prompt: &str,
        user_prompt: &str,
        images: &[ProcessedImage],
    ) -> Result<VerificationVerdict, AppError> {
        let mut resolved_config = config.clone();
        if resolved_config
            .api_key
            .as_deref()
            .unwrap_or("")
            .trim()
            .is_empty()
        {
            if let Some(base) = resolved_config.base_url.as_deref()
                && (base.contains("googleapis.com")
                    || resolved_config.model_name.contains("gemini"))
                && let Some(gemini_key) = &self.app_config.default_gemini_key
            {
                resolved_config.api_key = Some(gemini_key.clone());
            } else if let Some(server_key) = self.app_config.get_key_for_provider(&config.provider)
            {
                resolved_config.api_key = Some(server_key.to_string());
            }
        }

        if resolved_config
            .base_url
            .as_deref()
            .unwrap_or("")
            .trim()
            .is_empty()
            && let Some(server_base_url) =
                self.app_config.get_base_url_for_provider(&config.provider)
        {
            resolved_config.base_url = Some(server_base_url.to_string());
        }

        let provider = self.select_provider(&resolved_config.provider);
        provider
            .verify(&resolved_config, system_prompt, user_prompt, images)
            .await
    }
}

impl Default for VerificationEngine {
    fn default() -> Self {
        Self::new()
    }
}
