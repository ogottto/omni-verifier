use actix_web::{App, test as actix_test, web};
use image::{ImageBuffer, ImageFormat, Rgb};
use omni_verifier::{
    models::{
        request::{
            ImageSource, ModelEndpointConfig, ProviderKind, VerificationMode, VerificationRequest,
        },
        response::VerdictStatus,
    },
    providers::parse_verdict_json,
    routes,
    services::{
        image_processor::ImageProcessor, prompt_builder::PromptBuilder,
        verification_engine::VerificationEngine,
    },
};
use std::io::Cursor;
use std::sync::Arc;

/// Helper to generate a valid test JPEG in memory
fn create_test_image_bytes(width: u32, height: u32) -> Vec<u8> {
    let img: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::from_fn(width, height, |x, y| {
        Rgb([(x % 255) as u8, (y % 255) as u8, 128])
    });

    let mut buf = Vec::new();
    img.write_to(&mut Cursor::new(&mut buf), ImageFormat::Jpeg)
        .expect("Failed to write test image to memory buffer");
    buf
}

#[tokio::test]
async fn test_image_processor_in_memory() {
    let raw_bytes = create_test_image_bytes(2400, 1800);
    let original_len = raw_bytes.len();

    // Process image in memory
    let processed = ImageProcessor::process_bytes(&raw_bytes, 1).expect("Failed to process bytes");

    assert_eq!(processed.index, 1);
    assert_eq!(processed.mime_type, "image/jpeg");
    assert_eq!(processed.original_size_bytes, original_len);
    assert!(processed.data_url.starts_with("data:image/jpeg;base64,"));
    // Since original width was 2400, it should be downscaled <= 1920
    assert!(processed.width <= 1920);
    assert!(processed.height <= 1920);
}

#[tokio::test]
async fn test_image_processor_base64_data_url() {
    let raw_bytes = create_test_image_bytes(200, 200);
    let raw_b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &raw_bytes);
    let data_url = format!("data:image/jpeg;base64,{}", raw_b64);

    let processed =
        ImageProcessor::process_base64(&data_url, 1).expect("Failed to decode data URL");
    assert_eq!(processed.width, 200);
    assert_eq!(processed.height, 200);
}

#[tokio::test]
async fn test_parse_verdict_json_with_fenced_markdown_and_preamble() {
    let response_with_preamble = r#"Here is the audit result for the car verification:
```json
{
  "verdict": "PASS",
  "verified": true,
  "confidence_score": 0.98,
  "summary": "Both images depict the exact same silver Honda Civic.",
  "detailed_reasoning": "Identical front bumper scratch and matched registration sticker.",
  "checks": [
    {
      "name": "Vehicle Consistency",
      "passed": true,
      "observation": "Both vehicles match in make, model and color"
    }
  ],
  "detected_entities": ["Honda Civic"],
  "anomalies": []
}
```
Please let me know if further review is needed."#;

    let verdict = parse_verdict_json(response_with_preamble, "deepseek-flash")
        .expect("Should parse JSON with preamble properly");

    assert_eq!(verdict.verdict, VerdictStatus::Pass);
    assert!(verdict.verified);
    assert_eq!(verdict.confidence_score, 0.98);
    assert_eq!(verdict.checks.len(), 1);
    assert_eq!(verdict.model_used, "deepseek-flash");
}

#[tokio::test]
async fn test_ssrf_protection_blocks_internal_ip() {
    let client = reqwest::Client::new();
    let result =
        ImageProcessor::fetch_and_process_url(&client, "http://127.0.0.1:8099/car.jpg", 1).await;
    assert!(result.is_err());
    let err_str = result.err().unwrap().to_string();
    assert!(err_str.contains("private/internal IP") || err_str.contains("Forbidden URL target"));
}

#[tokio::test]
async fn test_prompt_builder_fraud_and_schema_inclusion() {
    let system_prompt = PromptBuilder::build_system_prompt(None);
    assert!(system_prompt.contains("Anti-Visual Injection"));
    assert!(system_prompt.contains("Anti-Spoofing"));
    assert!(system_prompt.contains("JSON OUTPUT SCHEMA"));

    let user_prompt = PromptBuilder::build_user_prompt("Is this the same car?", 2);
    assert!(user_prompt.contains("NUMBER OF ATTACHED IMAGES: 2"));
}

#[actix_web::test]
async fn test_health_and_capabilities_routes() {
    let app = actix_test::init_service(App::new().configure(routes::health::configure)).await;

    // 1. Health check
    let req = actix_test::TestRequest::get().uri("/health").to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert!(resp.status().is_success());

    // 2. Capabilities & aliases
    let req = actix_test::TestRequest::get()
        .uri("/api/v1/capabilities")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert!(resp.status().is_success());

    let req = actix_test::TestRequest::get()
        .uri("/health/capabilities")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert!(resp.status().is_success());

    let req = actix_test::TestRequest::get()
        .uri("/capabilities")
        .to_request();
    let resp = actix_test::call_service(&app, req).await;
    assert!(resp.status().is_success());
}

#[actix_web::test]
async fn test_verify_validation_no_images() {
    let engine = Arc::new(VerificationEngine::new());
    let engine_data = web::Data::new(engine);

    let app = actix_test::init_service(
        App::new()
            .app_data(engine_data.clone())
            .configure(routes::verify::configure),
    )
    .await;

    let request_body = VerificationRequest {
        prompt: "Verify if this is a real car".to_string(),
        primary_model: ModelEndpointConfig {
            provider: ProviderKind::OpenaiCompatible,
            model_name: "deepseek-flash".to_string(),
            base_url: None,
            api_key: Some("test_key".to_string()),
            detail: None,
            max_tokens: None,
            reasoning_effort: None,
        },
        secondary_model: None,
        mode: VerificationMode::Standard,
        images: vec![], // Empty! Should trigger validation error
        system_prompt: None,
        escalation_threshold: None,
    };

    let req = actix_test::TestRequest::post()
        .uri("/api/v1/verify")
        .set_json(&request_body)
        .to_request();

    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status().as_u16(), 400);
}

#[actix_web::test]
async fn test_verify_validation_too_many_images() {
    let engine = Arc::new(VerificationEngine::new());
    let engine_data = web::Data::new(engine);

    let app = actix_test::init_service(
        App::new()
            .app_data(engine_data.clone())
            .configure(routes::verify::configure),
    )
    .await;

    let dummy_images = vec![
        ImageSource::DataUrl("data:image/jpeg;base64,1234".to_string());
        15 // Exceeds limit of 10
    ];

    let request_body = VerificationRequest {
        prompt: "Verify cars".to_string(),
        primary_model: ModelEndpointConfig {
            provider: ProviderKind::OpenaiCompatible,
            model_name: "deepseek-flash".to_string(),
            base_url: None,
            api_key: Some("test_key".to_string()),
            detail: None,
            max_tokens: None,
            reasoning_effort: None,
        },
        secondary_model: None,
        mode: VerificationMode::Standard,
        images: dummy_images,
        system_prompt: None,
        escalation_threshold: None,
    };

    let req = actix_test::TestRequest::post()
        .uri("/api/v1/verify")
        .set_json(&request_body)
        .to_request();

    let resp = actix_test::call_service(&app, req).await;
    assert_eq!(resp.status().as_u16(), 400);
}
