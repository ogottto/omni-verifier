use actix_web::{HttpResponse, Responder, get, web};
use serde_json::json;

#[get("/health")]
pub async fn health_check() -> impl Responder {
    HttpResponse::Ok().json(json!({
        "status": "healthy",
        "service": "omni-verifier",
        "version": env!("CARGO_PKG_VERSION")
    }))
}

pub fn build_capabilities_json() -> serde_json::Value {
    json!({
        "service": "omni-verifier",
        "version": env!("CARGO_PKG_VERSION"),
        "supported_providers": [
            {
                "id": "openai_compatible",
                "description": "Universal OpenAI-compatible protocol for DeepSeek v4 Flash, OpenAI (GPT-4o), Groq, vLLM, LocalAI, Ollama v1",
                "default_base_url": "https://api.deepseek.com",
                "aliases": ["deepseek", "openai", "groq", "vllm"]
            },
            {
                "id": "gemini",
                "description": "Google Gemini 1.5 Flash / Pro, Gemini 2.0 Flash",
                "default_base_url": "https://generativelanguage.googleapis.com"
            },
            {
                "id": "anthropic",
                "description": "Anthropic Claude 3.5 Sonnet / Haiku",
                "default_base_url": "https://api.anthropic.com"
            },
            {
                "id": "ollama",
                "description": "Native Ollama /api/chat vision protocol",
                "default_base_url": "http://localhost:11434"
            }
        ],
        "verification_modes": [
            {
                "mode": "standard",
                "description": "Single-model execution with automatic failover to secondary provider on error/timeout"
            },
            {
                "mode": "smart_escalation",
                "description": "Primary model executes; if confidence < 0.85 or verdict is inconclusive, secondary model arbitrates"
            },
            {
                "mode": "consensus",
                "description": "Primary and secondary models evaluate simultaneously. Disagreements flag as DISPUTED"
            }
        ],
        "image_handling": {
            "storage": "Strictly in-memory, zero-disk persistence",
            "formats_supported": ["JPEG", "PNG", "WEBP", "Base64 Data URIs", "Remote Image URLs"],
            "max_dimension": 1920,
            "exif_inspection": true,
            "anti_spoofing_heuristics": true,
            "anti_visual_injection": true
        }
    })
}

#[get("/api/v1/capabilities")]
pub async fn capabilities() -> impl Responder {
    HttpResponse::Ok().json(build_capabilities_json())
}

#[get("/health/capabilities")]
pub async fn health_capabilities() -> impl Responder {
    HttpResponse::Ok().json(build_capabilities_json())
}

#[get("/capabilities")]
pub async fn root_capabilities() -> impl Responder {
    HttpResponse::Ok().json(build_capabilities_json())
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(health_check);
    cfg.service(capabilities);
    cfg.service(health_capabilities);
    cfg.service(root_capabilities);
}
