use crate::error::AppError;
use crate::models::{
    request::{ModelEndpointConfig, ProviderKind, VerificationMode, VerificationRequest},
    response::ApiResponse,
};
use crate::services::{
    image_processor::{ImageProcessor, ProcessedImage},
    verification_engine::VerificationEngine,
};
use actix_multipart::Multipart;
use actix_web::{HttpResponse, web};
use futures_util::{StreamExt, TryStreamExt};
use std::sync::Arc;
use tracing::{debug, info};

const MAX_IMAGE_FILE_BYTES: usize = 20 * 1024 * 1024; // 20 MB max per file
const MAX_TEXT_FIELD_BYTES: usize = 64 * 1024; // 64 KB max for standard form fields
const MAX_PROMPT_BYTES: usize = 1024 * 1024; // 1 MB max for prompt text

pub async fn verify_json(
    engine: web::Data<Arc<VerificationEngine>>,
    body: web::Json<VerificationRequest>,
) -> Result<HttpResponse, AppError> {
    info!(
        mode = ?body.mode,
        primary = %body.primary_model.model_name,
        "Received JSON verification request"
    );

    if body.images.is_empty() {
        return Err(AppError::Validation(
            "At least one image must be provided for verification".to_string(),
        ));
    }

    let max_images = engine.max_images();
    if body.images.len() > max_images {
        return Err(AppError::Validation(format!(
            "Too many images attached: maximum allowed is {} per verification",
            max_images
        )));
    }

    if body.prompt.trim().is_empty() {
        return Err(AppError::Validation(
            "Verification prompt cannot be empty".to_string(),
        ));
    }

    let response: ApiResponse = engine.verify(body.into_inner(), None).await?;
    Ok(HttpResponse::Ok().json(response))
}

pub async fn verify_multipart(
    engine: web::Data<Arc<VerificationEngine>>,
    mut payload: Multipart,
) -> Result<HttpResponse, AppError> {
    info!("Received multipart/form-data verification request");

    let mut prompt = String::new();
    let mut primary_provider_str = "openai_compatible".to_string();
    let mut primary_model_name = String::new();
    let mut primary_base_url = None;
    let mut primary_api_key = None;
    let mut primary_max_tokens = None;
    let mut primary_reasoning_effort = None;

    let mut secondary_provider_str = None;
    let mut secondary_model_name = None;
    let mut secondary_base_url = None;
    let mut secondary_api_key = None;
    let mut secondary_max_tokens = None;
    let mut secondary_reasoning_effort = None;

    let mut mode_str = "standard".to_string();
    let mut system_prompt = None;
    let mut in_memory_images: Vec<ProcessedImage> = Vec::new();
    let mut image_counter = 0;

    while let Ok(Some(mut field)) = payload.try_next().await {
        let name = field
            .content_disposition()
            .and_then(|cd| cd.get_name())
            .unwrap_or_default()
            .to_string();

        if name == "images" || name == "image" || name == "file" {
            let max_images = engine.max_images();
            if in_memory_images.len() >= max_images {
                return Err(AppError::Validation(format!(
                    "Too many images uploaded: maximum allowed is {} per verification",
                    max_images
                )));
            }

            // Read image bytes directly into memory with size guard
            let mut bytes_buf = Vec::new();
            while let Some(chunk) = field.next().await {
                let data = chunk.map_err(|e| {
                    AppError::BadRequest(format!("Failed reading multipart image chunk: {}", e))
                })?;
                if bytes_buf.len() + data.len() > MAX_IMAGE_FILE_BYTES {
                    return Err(AppError::Validation(
                        "Uploaded image exceeds maximum 20MB limit per file".to_string(),
                    ));
                }
                bytes_buf.extend_from_slice(&data);
            }

            if !bytes_buf.is_empty() {
                image_counter += 1;
                let _permit = ImageProcessor::acquire_permit().await?;
                let processed = ImageProcessor::process_bytes(&bytes_buf, image_counter)?;
                in_memory_images.push(processed);
            }
        } else {
            // Read text form fields with length guard
            let max_allowed = if name == "prompt" || name == "system_prompt" {
                MAX_PROMPT_BYTES
            } else {
                MAX_TEXT_FIELD_BYTES
            };

            let mut field_bytes = Vec::new();
            while let Some(chunk) = field.next().await {
                let data = chunk.map_err(|e| {
                    AppError::BadRequest(format!("Failed reading field chunk: {}", e))
                })?;
                if field_bytes.len() + data.len() > max_allowed {
                    return Err(AppError::Validation(format!(
                        "Field '{}' exceeds maximum allowed text size",
                        name
                    )));
                }
                field_bytes.extend_from_slice(&data);
            }

            let value = String::from_utf8(field_bytes)
                .map_err(|e| {
                    AppError::BadRequest(format!("Field '{}' must be valid UTF-8: {}", name, e))
                })?
                .trim()
                .to_string();

            match name.as_str() {
                "prompt" => prompt = value,
                "provider" | "primary_provider" => primary_provider_str = value,
                "model_name" | "primary_model_name" => primary_model_name = value,
                "base_url" | "primary_base_url" => {
                    if !value.is_empty() {
                        primary_base_url = Some(value);
                    }
                }
                "api_key" | "primary_api_key" => {
                    if !value.is_empty() {
                        primary_api_key = Some(value);
                    }
                }
                "secondary_provider" => {
                    if !value.is_empty() {
                        secondary_provider_str = Some(value);
                    }
                }
                "secondary_model_name" => {
                    if !value.is_empty() {
                        secondary_model_name = Some(value);
                    }
                }
                "secondary_base_url" => {
                    if !value.is_empty() {
                        secondary_base_url = Some(value);
                    }
                }
                "secondary_api_key" => {
                    if !value.is_empty() {
                        secondary_api_key = Some(value);
                    }
                }
                "max_tokens" | "primary_max_tokens" => {
                    primary_max_tokens = value.parse::<u32>().ok();
                }
                "reasoning_effort" | "primary_reasoning_effort" => {
                    if !value.is_empty() {
                        primary_reasoning_effort = Some(value);
                    }
                }
                "secondary_max_tokens" => {
                    secondary_max_tokens = value.parse::<u32>().ok();
                }
                "secondary_reasoning_effort" => {
                    if !value.is_empty() {
                        secondary_reasoning_effort = Some(value);
                    }
                }
                "mode" => mode_str = value,
                "system_prompt" => {
                    if !value.is_empty() {
                        system_prompt = Some(value);
                    }
                }
                _ => {
                    debug!(field_name = %name, "Ignored unexpected multipart field");
                }
            }
        }
    }

    if prompt.is_empty() {
        return Err(AppError::Validation(
            "Missing 'prompt' in multipart request".to_string(),
        ));
    }

    if primary_model_name.is_empty() {
        return Err(AppError::Validation(
            "Missing 'model_name' (or 'primary_model_name') in multipart request".to_string(),
        ));
    }

    if in_memory_images.is_empty() {
        return Err(AppError::Validation(
            "No images uploaded in 'images' multipart field".to_string(),
        ));
    }

    let primary_provider: ProviderKind =
        serde_json::from_value(serde_json::Value::String(primary_provider_str.clone())).map_err(
            |_| {
                AppError::Validation(format!(
                    "Invalid primary provider '{}'. Supported: openai_compatible, deepseek, gemini, anthropic, ollama",
                    primary_provider_str
                ))
            },
        )?;

    let primary_model = ModelEndpointConfig {
        provider: primary_provider,
        model_name: primary_model_name,
        base_url: primary_base_url,
        api_key: primary_api_key,
        detail: None,
        max_tokens: primary_max_tokens,
        reasoning_effort: primary_reasoning_effort,
    };

    let secondary_model = if let (Some(sec_prov_str), Some(sec_model)) =
        (secondary_provider_str, secondary_model_name)
    {
        let sec_prov: ProviderKind =
                serde_json::from_value(serde_json::Value::String(sec_prov_str.clone())).map_err(
                    |_| {
                        AppError::Validation(format!(
                            "Invalid secondary provider '{}'. Supported: openai_compatible, deepseek, gemini, anthropic, ollama",
                            sec_prov_str
                        ))
                    },
                )?;
        Some(ModelEndpointConfig {
            provider: sec_prov,
            model_name: sec_model,
            base_url: secondary_base_url,
            api_key: secondary_api_key,
            detail: None,
            max_tokens: secondary_max_tokens,
            reasoning_effort: secondary_reasoning_effort,
        })
    } else {
        None
    };

    let mode: VerificationMode =
        serde_json::from_value(serde_json::Value::String(mode_str.clone())).map_err(|_| {
            AppError::Validation(format!(
                "Invalid verification mode '{}'. Supported: standard, smart_escalation, consensus",
                mode_str
            ))
        })?;

    let request = VerificationRequest {
        prompt,
        primary_model,
        secondary_model,
        mode,
        images: vec![],
        system_prompt,
        escalation_threshold: None,
    };

    let response: ApiResponse = engine.verify(request, Some(in_memory_images)).await?;
    Ok(HttpResponse::Ok().json(response))
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::resource("/api/v1/verify")
            .route(
                web::post()
                    .guard(actix_web::guard::fn_guard(|ctx| {
                        ctx.head()
                            .headers()
                            .get(actix_web::http::header::CONTENT_TYPE)
                            .and_then(|v| v.to_str().ok())
                            .map(|ct| ct.starts_with("multipart/form-data"))
                            .unwrap_or(false)
                    }))
                    .to(verify_multipart),
            )
            .route(web::post().to(verify_json)),
    );

    // Backward-compatible alias
    cfg.route("/api/v1/verify/multipart", web::post().to(verify_multipart));
}
