use actix_web::{HttpResponse, ResponseError, http::StatusCode};
use serde::Serialize;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Image processing error: {0}")]
    ImageProcessing(String),

    #[error("AI provider error ({provider}): {message}")]
    Provider {
        provider: String,
        message: String,
        status_code: Option<u16>,
    },

    #[error(
        "All verification providers failed: primary ({primary_err}), secondary ({secondary_err})"
    )]
    AllProvidersFailed {
        primary_err: String,
        secondary_err: String,
    },

    #[error("Invalid request payload: {0}")]
    BadRequest(String),

    #[error("Internal server error: {0}")]
    Internal(String),
}

#[derive(Serialize)]
struct ErrorResponse {
    success: bool,
    error: String,
    details: Option<String>,
}

impl ResponseError for AppError {
    fn status_code(&self) -> StatusCode {
        match self {
            AppError::Validation(_) | AppError::BadRequest(_) => StatusCode::BAD_REQUEST,
            AppError::ImageProcessing(_) => StatusCode::UNPROCESSABLE_ENTITY,
            AppError::Provider {
                status_code: Some(code),
                ..
            } => StatusCode::from_u16(*code).unwrap_or(StatusCode::BAD_GATEWAY),
            AppError::Provider { .. } | AppError::AllProvidersFailed { .. } => {
                StatusCode::BAD_GATEWAY
            }
            AppError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn error_response(&self) -> HttpResponse {
        let status = self.status_code();
        let body = ErrorResponse {
            success: false,
            error: self.to_string(),
            details: match self {
                AppError::Provider { message, .. } => Some(message.clone()),
                AppError::AllProvidersFailed {
                    primary_err,
                    secondary_err,
                } => Some(format!(
                    "Primary: {} | Secondary: {}",
                    primary_err, secondary_err
                )),
                _ => None,
            },
        };

        HttpResponse::build(status).json(body)
    }
}
