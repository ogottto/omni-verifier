# OmniVerifier: Autonomous Multimodal Verification Engine

A high-performance visual verification service built with **Rust (Edition 2024)** and **Actix Web**. It provides a single unified API to verify vehicles, documents, identities, or arbitrary visual tasks using any multimodal AI model (DeepSeek v4 Flash, Google Gemini, OpenAI GPT-4o, Anthropic Claude, Groq, Ollama, vLLM).

The server runs on **port `8099`** by default.

---

## Table of Contents
1. [Core Features](#core-features)
2. [Security: Why API Keys Should NOT Be Passed in Request Bodies](#security-why-api-keys-should-not-be-passed-in-request-bodies)
3. [Estimated Verification Costs per Model](#estimated-verification-costs-per-model)
4. [CORS Configuration (Zero CORS Errors on File Uploads)](#cors-configuration-zero-cors-errors-on-file-uploads)
5. [Prerequisites & Installation Guides](#prerequisites--installation-guides)
   - [Installing Docker (Ubuntu, macOS, Windows)](#how-to-install-docker)
   - [Installing Rust (For Running Without Docker)](#how-to-install-rust)
6. [Setup & Running the Service](#setup--running-the-service)
   - [Option A: Running with Docker Compose](#option-a-running-with-docker-compose-recommended)
   - [Option B: Running with Cargo (Native Rust)](#option-b-running-with-cargo)
7. [API Usage & Examples](#api-usage--examples)
   - [Multipart Form (Direct Image File Upload)](#1-multipart-form-upload-direct-file-upload)
   - [JSON Request (Base64 / Image URLs)](#2-json-request-base64-or-remote-urls)
   - [Sample Response](#3-sample-json-response)
8. [Available Verification Modes](#available-verification-modes)

---

## Core Features

- **Strictly In-Memory Processing**: Zero disk persistence. Images are decoded in RAM, inspected for EXIF metadata (camera make, model, timestamp, orientation), downscaled to $\le 1920\text{px}$ preserving aspect ratio, and immediately deallocated from memory after verification.
- **Universal Provider Adapter (`openai_compatible`)**: Supports **DeepSeek v4 Flash**, **OpenAI** (`gpt-4o`, `gpt-4o-mini`), **Groq** (`llama-3.2-vision`), and self-hosted models (**vLLM**, **LocalAI**, **Ollama v1**).
- **Dedicated Cloud Adapters**: Native support for **Google Gemini** (1.5 / 2.0 Flash) and **Anthropic Claude** (3.5 Sonnet / Haiku).
- **Three Verification Strategies**:
  1. `standard`: Fast execution with automatic failover to a secondary model on timeout, 429 rate limit, or provider error.
  2. `smart_escalation` *(Recommended)*: Primary model executes. If confidence is below $0.85$ or the verdict is `INCONCLUSIVE`, the secondary model is automatically triggered as an arbiter.
  3. `consensus`: Executes two models in parallel via `tokio::join!`. Conflicting verdicts are marked as `DISPUTED` for human review.
- **Production Guardrails**:
  - Injected anti-visual prompt injection defenses.
  - Screen rebroadcast / printout spoofing detection.
  - Structured output parsing that cleans markdown code fences.

---

## Security: Why API Keys Should NOT Be Passed in Request Bodies

> [!WARNING]
> **Security Advisory**: If client applications (e.g., mobile apps, web frontends, or single-page apps) send LLM provider API keys (`DEEPSEEK_API_KEY`, `GEMINI_API_KEY`) in the JSON body or form fields, those keys are **publicly exposed** in client-side bundles, browser network inspectors, proxy logs, and mobile APKs. Anyone can extract and steal your keys.

### How OmniVerifier Secures Keys:
1. **Server-Side Secret Injection (Default & Recommended)**:
   - Store your provider API keys securely on the server in `.env` (`DEEPSEEK_API_KEY`, `GEMINI_API_KEY`, etc.).
   - The client simply submits the verification prompt and images without passing any API keys.
   - OmniVerifier automatically matches the requested model provider and injects the server's private key.
2. **Microservice Overrides (Optional)**:
   - If internal backend microservices need to supply specific tenant keys, they can pass them via secure HTTP headers (e.g., `Authorization: Bearer <key>` or `X-Api-Key`), or via the request model only when communicating over private networks.

---

## Estimated Verification Costs per Model

Each verification typically submits **2 images** (resized to 1080p, costing $\sim 1,600$ image tokens) plus $\sim 500$ prompt tokens and receives $\sim 400$ structured output tokens:

| Model | Provider | Input Cost / 1M | Output Cost / 1M | Est. Cost per Verification (2 Images) | Verdict Speed |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **DeepSeek v4 Flash** | DeepSeek | **$0.14** | **$0.28** | **~$0.0004** (400 verifications per $1) | **Ultra Fast (~1.2s)** |
| **Gemini 2.0 Flash** | Google | **$0.10** | **$0.40** | **~$0.00035** (300+ verifications per $1) | **Ultra Fast (~1.1s)** |
| **GPT-4o-mini** | OpenAI | **$0.15** | **$0.60** | **~$0.0005** (200 verifications per $1) | Fast (~1.5s) |
| **GPT-4o** | OpenAI | $2.50 | $10.00 | ~$0.008 (120 verifications per $1) | Moderate (~2.5s) |
| **Claude 3.5 Sonnet** | Anthropic | $3.00 | $15.00 | ~$0.012 (80 verifications per $1) | High Accuracy (~3.0s) |
| **Qwen2-VL / LLaVA** | Self-Hosted (Ollama/vLLM) | **$0.00** | **$0.00** | **$0.00 (Hardware cost only)** | Varies by GPU |

> [!TIP]
> Combining **DeepSeek v4 Flash** (as primary) with **Gemini 2.0 Flash** (as secondary failover / escalation) costs **less than $0.0005 per vehicle check**, making it ideal for high-volume automotive platforms.

---

## CORS Configuration (Zero CORS Errors on File Uploads)

OmniVerifier is configured with permissive Actix CORS headers:
- `allow_any_origin()`
- `allow_any_method()` (`GET`, `POST`, `OPTIONS`, `PUT`, `DELETE`)
- `allow_any_header()` (including `Content-Type`, `Authorization`, `X-Api-Key`)
- `expose_any_header()`
- `max_age(3600)`

When uploading files via `multipart/form-data` from frontend browsers (Vue, React, Nuxt, Angular, Next.js, or vanilla JS `fetch`/`axios`), the preflight `OPTIONS` requests resolve cleanly with zero CORS errors.

---

## Prerequisites & Installation Guides

### How to Install Docker

#### On Ubuntu / Debian:
```bash
# 1. Update package index and install certificates
sudo apt update
sudo apt install -y ca-certificates curl gnupg

# 2. Add Docker official GPG key
sudo install -m 0755 -d /etc/apt/keyrings
curl -fsSL https://download.docker.com/linux/ubuntu/gpg | sudo gpg --dearmor -o /etc/apt/keyrings/docker.gpg
sudo chmod a+r /etc/apt/keyrings/docker.gpg

# 3. Set up repository
echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.gpg] https://download.docker.com/linux/ubuntu $(. /etc/os-release && echo "$VERSION_CODENAME") stable" | sudo tee /etc/apt/sources.list.d/docker.list > /dev/null

# 4. Install Docker Engine and Compose plugin
sudo apt update
sudo apt install -y docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin

# 5. Add your user to the docker group (optional, to run without sudo)
sudo usermod -aG docker $USER
newgrp docker
```

#### On macOS:
Install [Docker Desktop for Mac](https://docs.docker.com/desktop/setup/install/mac-install/) or use Homebrew:
```bash
brew install --cask docker
```

#### On Windows:
Install [Docker Desktop for Windows](https://docs.docker.com/desktop/setup/install/windows-install/) (requires WSL 2 enabled).

---

### How to Install Rust

If you prefer running OmniVerifier directly on your machine without Docker:

#### On Linux / macOS:
```bash
# 1. Install rustup toolchain installer
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y

# 2. Configure current shell
source "$HOME/.cargo/env"

# 3. Verify installation
rustc --version
cargo --version
```

#### On Windows:
Download and run the [rustup-init.exe installer](https://rustup.rs/). Ensure Microsoft C++ Build Tools are installed.

---

## Setup & Running the Service

### 1. Environment Configuration

Copy the template `.env.example` to `.env`:
```bash
cp .env.example .env
```

Edit `.env` with your desired configuration:
```env
# Server Binding
HOST=0.0.0.0
PORT=8099
RUST_LOG=info,omni_verifier=debug,actix_web=info
MAX_PAYLOAD_SIZE_MB=50

# Maximum images allowed per request (e.g., 5 or 10)
MAX_IMAGES_PER_REQUEST=5

# Server-Side Provider Keys (ALL KEYS ARE OPTIONAL!)
# You only need the key for the provider(s) you intend to use.
# Self-hosted models (Ollama, vLLM) require zero API keys.
DEEPSEEK_API_KEY=your_deepseek_api_key_here
GEMINI_API_KEY=your_gemini_api_key_here
OPENAI_API_KEY=your_openai_api_key_here
ANTHROPIC_API_KEY=your_anthropic_api_key_here

# Base URL Overrides for Local / Self-Hosted Models
# Default for Ollama is http://localhost:11434
OLLAMA_BASE_URL=http://localhost:11434

# If running OmniVerifier in Docker and Ollama on host machine, use:
# OLLAMA_BASE_URL=http://host.docker.internal:11434

# Optional Custom Endpoints (e.g. vLLM, LocalAI, Hugging Face)
# VLLM_BASE_URL=http://localhost:8000/v1
# DEEPSEEK_BASE_URL=https://api.deepseek.com
# OPENAI_BASE_URL=https://api.openai.com/v1

# Optional Custom Gateways / Reverse Proxies for Gemini & Anthropic
# (Defaults: https://generativelanguage.googleapis.com and https://api.anthropic.com)
# GEMINI_BASE_URL=https://generativelanguage.googleapis.com
# ANTHROPIC_BASE_URL=https://api.anthropic.com
```

> [!NOTE]
> **Is it a must to set up all API keys?**
> **No.** All keys are completely optional. If you only want to use DeepSeek, only fill in `DEEPSEEK_API_KEY`. If you only want Gemini, only fill in `GEMINI_API_KEY`. If you are using local models via Ollama or vLLM, you can leave all API keys empty.

### How Model Base URLs Are Detected & Configured
1. **Google Gemini**:
   - Default: `https://generativelanguage.googleapis.com`
   - Override via `.env`: `GEMINI_BASE_URL` (useful for Google Cloud Vertex AI reverse proxies or Cloudflare AI Gateway).
2. **Anthropic Claude**:
   - Default: `https://api.anthropic.com`
   - Override via `.env`: `ANTHROPIC_BASE_URL` (useful for enterprise proxies, AWS Bedrock gateway, or Cloudflare AI Gateway).
3. **Ollama (Automatic Zero-Config)**:
   - Default: `http://localhost:11434` (or `OLLAMA_BASE_URL` in `.env`).
   - In Docker: `docker-compose.yml` automatically routes to `http://host.docker.internal:11434` to communicate with the host's Ollama instance.
4. **Self-Hosted vLLM / LocalAI / LM Studio**:
   - Set `provider: "openai_compatible"` and configure `VLLM_BASE_URL=http://localhost:8000/v1` in `.env` (or pass `base_url` per request).
5. **Hugging Face Dedicated Endpoints**:
   - Set `provider: "openai_compatible"`, `model_name: "Qwen/Qwen2-VL-7B-Instruct"`, and pass your endpoint URL in `base_url` with your `hf_...` token.

> [!TIP]
> **Memory & Code Safety Guarantee**:
> The service contains **zero `unsafe` blocks** and **zero panicking `.unwrap()` / `.expect()` calls** in runtime code. All failures (invalid images, missing keys, timeouts, network hiccups) are gracefully converted to structured HTTP error responses without crashing the server.

---

### Option A: Running with Docker Compose (Recommended)

```bash
# Build and start the container in background
docker compose up -d --build

# View logs
docker compose logs -f

# Check health
curl http://localhost:8099/health

# Check capabilities (aliases supported: /health/capabilities and /api/v1/capabilities)
curl http://localhost:8099/health/capabilities
```

To stop:
```bash
docker compose down
```

---

### Option B: Running with Cargo (Directly on Host)

```bash
# 1. (Optional) Run tests and linter
cargo test
cargo fmt --all && cargo clippy --workspace --all-targets --all-features -- -D warnings

# 2. Start the server (default-run is configured, so simple 'cargo run' starts OmniVerifier)
cargo run

# Or run in optimized release mode:
cargo run --release
```

Server will start on `http://0.0.0.0:8099`.

---

## Generating Local Test Images

If you do not want to download images manually, you can generate clean sample test images using the built-in generator:

```bash
cargo run --bin generate_test_images
```

This creates:
- `test_assets/car_front.jpg` (Front vehicle angle)
- `test_assets/car_rear.jpg` (Rear vehicle angle)
- `test_assets/animal_cat.jpg` (Animal inspection image)

*(Note: The `test_assets/` folder is automatically excluded by `.gitignore` so test images are never pushed to GitHub).*

---

## API Testing Examples (Unified Endpoint: POST /api/v1/verify)

The service provides a single polymorphic endpoint: `POST /api/v1/verify`. It automatically handles both `multipart/form-data` and `application/json` payloads. *(Note: `/api/v1/verify/multipart` is also supported as an alias).*

### 1. Test with Google Gemini (`gemini-3.1-flash-lite-preview`)

#### Using Multipart File Upload (Direct to `/api/v1/verify`):
```bash
curl -X POST http://localhost:8099/api/v1/verify \
  -F "prompt=Determine if both images depict the exact same car from different angles." \
  -F "primary_provider=gemini" \
  -F "primary_model_name=gemini-3.1-flash-lite-preview" \
  -F "images=@test_assets/car_front.jpg" \
  -F "images=@test_assets/car_rear.jpg"
```

#### Using JSON with High-Res Unsplash Images:
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

---

### 2. Test with DeepSeek (`deepseek-flash`)

```bash
curl -X POST http://localhost:8099/api/v1/verify \
  -F "prompt=Determine if both images depict the exact same vehicle from different angles." \
  -F "primary_provider=openai_compatible" \
  -F "primary_model_name=deepseek-flash" \
  -F "images=@test_assets/car_front.jpg" \
  -F "images=@test_assets/car_rear.jpg"
```

---

### 3. Test Cross-Provider Mode (Google Gemini via OpenAI-Compatible Endpoint)

OmniVerifier's `openai_compatible` adapter can route directly through Google's official OpenAI `/chat/completions` API using your `GEMINI_API_KEY`:

```bash
curl -X POST http://localhost:8099/api/v1/verify \
  -F "prompt=Determine if both images depict the exact same vehicle from different angles." \
  -F "primary_provider=openai_compatible" \
  -F "primary_base_url=https://generativelanguage.googleapis.com/v1beta/openai" \
  -F "primary_model_name=gemini-3.1-flash-lite-preview" \
  -F "images=@test_assets/car_front.jpg" \
  -F "images=@test_assets/car_rear.jpg"
```

---

### 4. Test Dual Failover & Smart Escalation (DeepSeek + Gemini)

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

---

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

### 3. Sample JSON Response

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
| `smart_escalation` *(Default)* | Runs the primary model. If confidence is $\ge 0.85$, returns immediately. If confidence is $< 0.85$ or the verdict is `INCONCLUSIVE`, the secondary model is triggered as a referee. | Balanced cost & accuracy. |
| `consensus` | Queries both models concurrently. If both agree, combines confidence; if they disagree, returns `verdict: "DISPUTED"` for human escalation. | High-stakes claims or financial authorization. |

---

## Health and Capabilities Endpoints

- `GET /health` - Health check endpoint.
- `GET /api/v1/capabilities` - Inspect supported model providers, aliases, verification strategies, and image specs.
