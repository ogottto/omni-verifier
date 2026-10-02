use crate::error::AppError;
use crate::models::response::{ExifSummary, ImageMetadata};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use exif::{In, Reader as ExifReader, Tag};
use image::{
    DynamicImage, GenericImageView, ImageFormat, ImageReader, Limits, imageops::FilterType,
};
use std::io::Cursor;
use std::net::IpAddr;
use std::sync::LazyLock;
use tokio::sync::Semaphore;
use tracing::{debug, warn};

const MAX_IMAGE_DIMENSION: u32 = 1920;
const MAX_IMAGE_DOWNLOAD_BYTES: usize = 20 * 1024 * 1024; // 20 MB max download
const MAX_DECODE_RAM_BYTES: u64 = 64 * 1024 * 1024; // 64 MB max decompression RAM
const MAX_DECODE_DIMENSION: u32 = 8192; // 8K max width/height

static DECOMPRESSION_SEMAPHORE: LazyLock<Semaphore> = LazyLock::new(|| Semaphore::new(16));

#[derive(Debug, Clone)]
pub struct ProcessedImage {
    pub index: usize,
    pub data_url: String,
    pub raw_base64: String,
    pub mime_type: String,
    pub width: u32,
    pub height: u32,
    pub original_size_bytes: usize,
    pub exif: Option<ExifSummary>,
}

impl ProcessedImage {
    pub fn to_metadata(&self) -> ImageMetadata {
        ImageMetadata {
            index: self.index,
            width: self.width,
            height: self.height,
            mime_type: self.mime_type.clone(),
            size_bytes: self.original_size_bytes,
            exif: self.exif.clone(),
        }
    }
}

pub struct ImageProcessor;

impl ImageProcessor {
    /// Acquires a permit from the global decompression concurrency semaphore
    pub async fn acquire_permit() -> Result<tokio::sync::SemaphorePermit<'static>, AppError> {
        DECOMPRESSION_SEMAPHORE.acquire().await.map_err(|e| {
            AppError::Internal(format!(
                "Failed to acquire image processing semaphore: {}",
                e
            ))
        })
    }

    /// In-memory processing of image bytes with decompression bomb protection
    pub fn process_bytes(raw_bytes: &[u8], index: usize) -> Result<ProcessedImage, AppError> {
        if raw_bytes.is_empty() {
            return Err(AppError::ImageProcessing(
                "Image byte buffer is empty".to_string(),
            ));
        }

        let original_size = raw_bytes.len();
        debug!(
            index = index,
            size = original_size,
            "Processing image in memory"
        );

        // 1. Extract EXIF metadata if present
        let exif = Self::extract_exif(raw_bytes);

        // 2. Decode image safely with strict decompression limits to prevent pixel flood DoS
        let mut reader = ImageReader::new(Cursor::new(raw_bytes))
            .with_guessed_format()
            .map_err(|e| {
                AppError::ImageProcessing(format!("Failed to determine image format: {}", e))
            })?;

        let mut limits = Limits::default();
        limits.max_image_width = Some(MAX_DECODE_DIMENSION);
        limits.max_image_height = Some(MAX_DECODE_DIMENSION);
        limits.max_alloc = Some(MAX_DECODE_RAM_BYTES);
        reader.limits(limits);

        let mut img = reader.decode().map_err(|e| {
            AppError::ImageProcessing(format!(
                "Failed to decode image safely (pixel limit or invalid format): {}",
                e
            ))
        })?;

        // 3. Apply EXIF orientation rotation if specified
        if let Some(ref meta) = exif
            && let Some(orient) = meta.orientation
        {
            img = Self::apply_orientation(img, orient);
        }

        // 4. Downscale if dimensions exceed threshold
        let (width, height) = img.dimensions();
        if width > MAX_IMAGE_DIMENSION || height > MAX_IMAGE_DIMENSION {
            img = img.resize(
                MAX_IMAGE_DIMENSION,
                MAX_IMAGE_DIMENSION,
                FilterType::Lanczos3,
            );
            debug!(
                original_w = width,
                original_h = height,
                new_w = img.width(),
                new_h = img.height(),
                "Downscaled image to optimize token limits and latency"
            );
        }

        let (final_w, final_h) = img.dimensions();

        // 5. Re-encode to JPEG in-memory buffer
        let mut jpeg_buf = Vec::new();
        img.write_to(&mut Cursor::new(&mut jpeg_buf), ImageFormat::Jpeg)
            .map_err(|e| AppError::ImageProcessing(format!("Failed to re-encode image: {}", e)))?;

        let raw_base64 = STANDARD.encode(&jpeg_buf);
        let data_url = format!("data:image/jpeg;base64,{}", raw_base64);

        Ok(ProcessedImage {
            index,
            data_url,
            raw_base64,
            mime_type: "image/jpeg".to_string(),
            width: final_w,
            height: final_h,
            original_size_bytes: original_size,
            exif,
        })
    }

    fn extract_exif(bytes: &[u8]) -> Option<ExifSummary> {
        let mut cursor = Cursor::new(bytes);
        let exif_data = match ExifReader::new().read_from_container(&mut cursor) {
            Ok(exif) => exif,
            Err(_) => return None,
        };

        let mut summary = ExifSummary {
            camera_make: None,
            camera_model: None,
            date_time: None,
            orientation: None,
        };

        if let Some(field) = exif_data.get_field(Tag::Make, In::PRIMARY) {
            summary.camera_make = Some(field.display_value().to_string().trim().to_string());
        }

        if let Some(field) = exif_data.get_field(Tag::Model, In::PRIMARY) {
            summary.camera_model = Some(field.display_value().to_string().trim().to_string());
        }

        if let Some(field) = exif_data.get_field(Tag::DateTime, In::PRIMARY) {
            summary.date_time = Some(field.display_value().to_string().trim().to_string());
        }

        if let Some(field) = exif_data.get_field(Tag::Orientation, In::PRIMARY)
            && let Some(val) = field.value.get_uint(0)
        {
            summary.orientation = Some(val);
        }

        Some(summary)
    }

    fn apply_orientation(img: DynamicImage, orientation: u32) -> DynamicImage {
        match orientation {
            3 => img.rotate180(),
            6 => img.rotate90(),
            8 => img.rotate270(),
            _ => img,
        }
    }

    /// Fetches remote image via URL into memory with SSRF defenses and download size limits
    pub async fn fetch_and_process_url(
        client: &reqwest::Client,
        raw_url: &str,
        index: usize,
    ) -> Result<ProcessedImage, AppError> {
        let parsed_url = reqwest::Url::parse(raw_url)
            .map_err(|e| AppError::Validation(format!("Invalid image URL '{}': {}", raw_url, e)))?;

        // 1. Protocol check: HTTP / HTTPS only
        if parsed_url.scheme() != "http" && parsed_url.scheme() != "https" {
            return Err(AppError::Validation(
                "Only HTTP and HTTPS URLs are permitted for image fetching".to_string(),
            ));
        }

        // 2. SSRF Protection: Resolve host and block private, loopback, link-local and internal IPs
        let host = parsed_url
            .host_str()
            .ok_or_else(|| AppError::Validation("Image URL missing valid hostname".to_string()))?;

        let port = parsed_url.port_or_known_default().unwrap_or(80);
        let socket_addr_str = format!("{}:{}", host, port);

        let addrs = tokio::net::lookup_host(&socket_addr_str)
            .await
            .map_err(|e| {
                AppError::ImageProcessing(format!(
                    "DNS resolution failed for host '{}': {}",
                    host, e
                ))
            })?;

        for addr in addrs {
            if is_forbidden_ip(addr.ip()) {
                return Err(AppError::Validation(format!(
                    "Forbidden URL target: Host '{}' resolves to private/internal IP ({})",
                    host,
                    addr.ip()
                )));
            }
        }

        // 3. Download stream with strict byte limit and 10s timeout
        let mut response = client
            .get(parsed_url)
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await
            .map_err(|e| {
                AppError::ImageProcessing(format!(
                    "Failed to connect to image URL '{}': {}",
                    raw_url, e
                ))
            })?;

        if response.status().is_redirection() {
            return Err(AppError::Validation(format!(
                "Image URL redirection is forbidden to prevent SSRF bypass (HTTP {})",
                response.status()
            )));
        }

        if !response.status().is_success() {
            return Err(AppError::ImageProcessing(format!(
                "Failed to fetch image from '{}' (HTTP {})",
                raw_url,
                response.status()
            )));
        }

        if let Some(content_length) = response.content_length()
            && content_length as usize > MAX_IMAGE_DOWNLOAD_BYTES
        {
            return Err(AppError::Validation(format!(
                "Image at '{}' exceeds maximum allowed size of 20MB",
                raw_url
            )));
        }

        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|e| {
            AppError::ImageProcessing(format!("Error reading image download stream: {}", e))
        })? {
            if bytes.len() + chunk.len() > MAX_IMAGE_DOWNLOAD_BYTES {
                return Err(AppError::Validation(
                    "Image download aborted: payload exceeded maximum 20MB limit".to_string(),
                ));
            }
            bytes.extend_from_slice(&chunk);
        }

        let _permit = Self::acquire_permit().await?;
        Self::process_bytes(&bytes, index)
    }

    /// Decodes base64 string or data URL into memory and processes it
    pub fn process_base64(raw_str: &str, index: usize) -> Result<ProcessedImage, AppError> {
        let base64_data = if let Some(stripped) = raw_str.strip_prefix("data:") {
            if let Some(idx) = stripped.find(";base64,") {
                &stripped[idx + 8..]
            } else {
                warn!("Malformed data URL, attempting direct base64 decode");
                raw_str
            }
        } else {
            raw_str
        };

        let trimmed = base64_data.trim();
        // Prevent huge base64 string allocations (> 30MB base64)
        if trimmed.len() > 30 * 1024 * 1024 {
            return Err(AppError::Validation(
                "Base64 image payload exceeds maximum allowed size limit".to_string(),
            ));
        }

        let decoded = STANDARD
            .decode(trimmed)
            .map_err(|e| AppError::ImageProcessing(format!("Base64 decode failure: {}", e)))?;

        Self::process_bytes(&decoded, index)
    }
}

/// Identifies private, loopback, link-local, or cloud metadata IP ranges to prevent SSRF
fn is_forbidden_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ipv4) => {
            ipv4.is_loopback()
                || ipv4.is_private()
                || ipv4.is_link_local()
                || ipv4.is_broadcast()
                || ipv4.is_documentation()
                || ipv4.is_unspecified()
        }
        IpAddr::V6(ipv6) => {
            ipv6.is_loopback()
                || ipv6.is_unspecified()
                || ((ipv6.segments()[0] & 0xfe00) == 0xfc00) // Unique local addresses (fc00::/7)
                || ((ipv6.segments()[0] & 0xffc0) == 0xfe80) // Link-local addresses (fe80::/10)
        }
    }
}
