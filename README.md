# OmniVerifier: Autonomous Multimodal Visual Verification Engine

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust: 2024 Edition](https://img.shields.io/badge/Rust-2024%20Edition-orange.svg)](https://www.rust-lang.org)
[![Docker](https://img.shields.io/badge/Docker-Ready-2496ED.svg)](https://www.docker.com)

A high-performance visual verification service built with **Rust (Edition 2024)** and **Actix Web**. It provides a single unified API to verify vehicles, documents, identities, or arbitrary visual tasks using any multimodal AI model (DeepSeek v4 Flash, Google Gemini, OpenAI GPT-4o, Anthropic Claude, Groq, Ollama, vLLM).

The server runs on **port `8099`** by default.

---

## Quickstart (Run in 30 Seconds)

### Prerequisites
* [Docker & Docker Compose](https://docs.docker.com/get-docker/) (or [Rust 1.85+](https://rustup.rs/) for native compilation).

### 1. Clone & Configure
```bash
git clone https://github.com/ogottto/omni-verifier.git
cd omni-verifier
cp .env.example .env
```
*(Add whatever API key(s) you use in `.env` — e.g. `GEMINI_API_KEY`, `DEEPSEEK_API_KEY`, or leave blank for local Ollama).*

### 2. Start the Engine
```bash
docker compose up -d --build
```

### 3. Verify Health & Capabilities
```bash
curl http://localhost:8099/health
curl http://localhost:8099/api/v1/capabilities
```

---

## Core Features

- **Strictly In-Memory Processing**: Zero disk persistence. Images are decoded in RAM, inspected for EXIF metadata (camera make, model, timestamp, orientation), downscaled to $\le 1920\text{px}$ preserving aspect ratio, and immediately deallocated after verification.
- **Universal Provider Adapter (`openai_compatible`)**: Supports **DeepSeek v4 Flash**, **OpenAI** (`gpt-4o`, `gpt-4o-mini`), **Groq** (`llama-3.2-vision`), and self-hosted models (**vLLM**, **LocalAI**, **Ollama**).
- **Dedicated Cloud Adapters**: Native support for **Google Gemini** (1.5 / 2.0 Flash / 3.x Preview) and **Anthropic Claude** (3.5 Sonnet / Haiku).
- **Zero-Exposure Key Management**: Provider API keys are injected securely on the server via `.env`. Frontend apps (web or mobile) never need to expose keys in client bundles or network requests.
- **Three Verification Strategies**:
  1. `standard`: Fast execution with automatic failover to a secondary model on timeout, 429 rate limit, or provider error.
  2. `smart_escalation` *(Default)*: Primary model executes. If confidence is below $0.85$ or the verdict is `INCONCLUSIVE`, the secondary model is automatically triggered as an arbiter.
  3. `consensus`: Executes two models in parallel via `tokio::join!`. Conflicting verdicts are marked as `DISPUTED` for human review.
- **Built-in Security Guardrails**:
  - SSRF defense with HTTP redirect rejection on remote image URLs.
  - Global memory concurrency semaphore bounding simultaneous image decompressions.
  - Anti-visual prompt injection defenses and screen rebroadcast/printout spoofing detection.
  - Zero `unsafe` blocks and zero unhandled `.unwrap()` / `.expect()` calls in runtime code paths.

---

## Estimated Verification Costs & Speed

Each verification typically submits **2 images** (resized to 1080p, costing $\sim 1,600$ image tokens) plus $\sim 500$ prompt tokens and receives $\sim 400$ structured output tokens:

| Model | Provider | Input Cost / 1M | Output Cost / 1M | Est. Cost per Verification (2 Images) | Response Time |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **DeepSeek v4 Flash** | DeepSeek | **$0.14** | **$0.28** | **~$0.0004** (400 checks per $1) | **Ultra Fast (~1.2s)** |
| **Gemini 2.0 Flash** | Google | **$0.10** | **$0.40** | **~$0.00035** (300+ checks per $1) | **Ultra Fast (~1.1s)** |
| **GPT-4o-mini** | OpenAI | **$0.15** | **$0.60** | **~$0.0005** (200 checks per $1) | Fast (~1.5s) |
| **GPT-4o** | OpenAI | $2.50 | $10.00 | ~$0.008 (120 checks per $1) | Moderate (~2.5s) |
| **Claude 3.5 Sonnet** | Anthropic | $3.00 | $15.00 | ~$0.012 (80 checks per $1) | High Accuracy (~3.0s) |
| **Qwen2-VL / LLaVA** | Self-Hosted (Ollama/vLLM) | **$0.00** | **$0.00** | **$0.00 (Hardware cost only)** | Varies by GPU |

---

## Running with Cargo (Directly on Host)

If you prefer running without Docker:

```bash
# 1. Run tests and linting
cargo test
cargo fmt --all && cargo clippy --workspace --all-targets --all-features -- -D warnings

# 2. Run the server (default port 8099)
cargo run --release
```

### Generating Local Test Images
Generate clean sample test images locally for testing vehicle matching:
```bash
cargo run --bin generate_test_images
```
This generates:
- `test_assets/car_front.jpg`
- `test_assets/car_rear.jpg`
- `test_assets/animal_cat.jpg`

*(Note: `test_assets/` is automatically ignored by Git).*

---

## API Usage & Examples (POST /api/v1/verify)

OmniVerifier provides a single polymorphic endpoint: `POST /api/v1/verify`. It automatically inspects the `Content-Type` header and supports both direct file uploads (`multipart/form-data`) and JSON payloads (`application/json`).

### 1. Direct File Upload (Multipart Form)
```bash
curl -X POST http://localhost:8099/api/v1/verify \
  -F "prompt=Determine if both images depict the exact same car from different angles." \
  -F "primary_provider=gemini" \
  -F "primary_model_name=gemini-3.1-flash-lite-preview" \
  -F "images=@test_assets/car_front.jpg" \
  -F "images=@test_assets/car_rear.jpg"
```

### 2. JSON Request (Remote Image URLs or Base64)
```bash
curl -X POST http://localhost:8099/api/v1/verify \
  -H "Content-Type: application/json" \
  -d '{
    "prompt": "Determine if both photos depict the exact same vehicle from different angles, and check for any body inconsistencies.",
    "primary_model": {
      "provider": "gemini",
      "model_name": "gemini-3.1-flash-lite-preview"
    },
    "mode": "standard",
    "images": [
      { "url": "https://images.unsplash.com/photo-1503376780353-7e6692767b70?w=1200" },
      { "url": "https://images.unsplash.com/photo-1617788138017-80ad40651399?w=1200" }
    ]
  }'
```

### 3. Dual-Model Smart Escalation (DeepSeek Flash + Gemini Arbiter)
```bash
curl -X POST http://localhost:8099/api/v1/verify \
  -F "prompt=Determine if both images depict the exact same car." \
  -F "primary_provider=openai_compatible" \
  -F "primary_model_name=deepseek-flash" \
  -F "secondary_provider=gemini" \
  -F "secondary_model_name=gemini-3.1-flash-lite-preview" \
  -F "mode=smart_escalation" \
  -F "images=@test_assets/car_front.jpg" \
  -F "images=@test_assets/car_rear.jpg"
```

### 4. Cross-Provider Mode (Gemini via OpenAI-Compatible Endpoint)
```bash
curl -X POST http://localhost:8099/api/v1/verify \
  -F "prompt=Determine if both images depict the exact same vehicle from different angles." \
  -F "primary_provider=openai_compatible" \
  -F "primary_base_url=https://generativelanguage.googleapis.com/v1beta/openai" \
  -F "primary_model_name=gemini-3.1-flash-lite-preview" \
  -F "images=@test_assets/car_front.jpg" \
  -F "images=@test_assets/car_rear.jpg"
```

### 5. Dual-Model Consensus Mode (Parallel Multi-Model Execution)
```bash
curl -X POST http://localhost:8099/api/v1/verify \
  -F "prompt=Determine if both images depict the exact same vehicle from different angles." \
  -F "mode=consensus" \
  -F "primary_provider=gemini" \
  -F "primary_model_name=gemini-3.1-flash-lite-preview" \
  -F "secondary_provider=openai_compatible" \
  -F "secondary_model_name=deepseek-flash" \
  -F "images=@test_assets/car_front.jpg" \
  -F "images=@test_assets/car_rear.jpg"
```

---

## Sample JSON Response

```json
{
  "success": true,
  "request_id": "76974db3-26bb-4934-8c8f-3da86c3ebbfd",
  "mode": "smart_escalation",
  "verdict": {
    "verdict": "PASS",
    "verified": true,
    "confidence_score": 0.96,
    "summary": "Both images depict the same silver 2021 Toyota Corolla sedan with matching alloy wheels.",
    "detailed_reasoning": "Image 1 shows the front-left quarter view and Image 2 shows the rear-right quarter view. Both vehicles exhibit identical paint reflection, trim, wheel rim design, and matching registration sticker location.",
    "checks": [
      {
        "name": "Vehicle Presence",
        "passed": true,
        "observation": "Real-world sedan confirmed in both frames"
      },
      {
        "name": "Identity Match",
        "passed": true,
        "observation": "Identical trim, alloy wheels, and side mirror geometry"
      },
      {
        "name": "Anti-Spoofing",
        "passed": true,
        "observation": "No screen moire, bezel edges, or print artifacts detected"
      }
    ],
    "detected_entities": [
      "Toyota Corolla 2021",
      "Alloy Rims",
      "Silver Paint"
    ],
    "anomalies": [],
    "model_used": "deepseek-flash"
  },
  "failover_triggered": false,
  "escalated": false,
  "execution_time_ms": 1180,
  "images_inspected": 2,
  "images_metadata": [
    {
      "index": 1,
      "width": 1920,
      "height": 1080,
      "mime_type": "image/jpeg",
      "size_bytes": 842010,
      "exif": {
        "camera_make": "Apple",
        "camera_model": "iPhone 15 Pro",
        "date_time": "2026:09:28 16:32:01",
        "orientation": 1
      }
    }
  ]
}
```

---

## Available Verification Modes

| Mode | Behavior | Best Used For |
| :--- | :--- | :--- |
| `standard` | Runs the primary model. If it encounters a rate limit (429) or connection error, it automatically fails over to the secondary model. | Low latency, minimum cost. |
| `smart_escalation` *(Default)* | Runs the primary model. If confidence is $\ge 0.85$, returns immediately. If confidence is $< 0.85$ or verdict is `INCONCLUSIVE`, the secondary model is triggered as a referee. | Balanced cost & high accuracy. |
| `consensus` | Queries both models concurrently. If both agree, combines confidence; if they disagree, returns `verdict: "DISPUTED"` for human escalation. | High-stakes claims or financial authorization. |

---

## Health, Capabilities & CORS

- `GET /health`: Health status probe.
- `GET /api/v1/capabilities`: Inspect supported model providers, verification strategies, and image specs (also aliased at `/health/capabilities`).
- **CORS**: Configured with permissive Actix CORS headers (`allow_any_origin`, `allow_any_method`, `allow_any_header`) to support direct browser uploads from React, Vue, Next.js, and mobile clients with zero CORS errors.

---

## License

This project is licensed under the [MIT License](LICENSE).
