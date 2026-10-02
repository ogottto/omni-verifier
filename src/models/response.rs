use super::request::VerificationMode;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum VerdictStatus {
    Pass,
    Fail,
    Inconclusive,
    Disputed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckItem {
    pub name: String,
    pub passed: bool,
    pub observation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationVerdict {
    pub verdict: VerdictStatus,
    pub verified: bool,
    pub confidence_score: f32,
    pub summary: String,
    pub detailed_reasoning: String,
    #[serde(default)]
    pub checks: Vec<CheckItem>,
    #[serde(default)]
    pub detected_entities: Vec<String>,
    #[serde(default)]
    pub anomalies: Vec<String>,
    #[serde(default)]
    pub model_used: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExifSummary {
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub date_time: Option<String>,
    pub orientation: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageMetadata {
    pub index: usize,
    pub width: u32,
    pub height: u32,
    pub mime_type: String,
    pub size_bytes: usize,
    pub exif: Option<ExifSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse {
    pub success: bool,
    pub request_id: String,
    pub mode: VerificationMode,
    pub verdict: VerificationVerdict,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secondary_verdict: Option<VerificationVerdict>,
    pub failover_triggered: bool,
    pub escalated: bool,
    pub execution_time_ms: u128,
    pub images_inspected: usize,
    pub images_metadata: Vec<ImageMetadata>,
}
