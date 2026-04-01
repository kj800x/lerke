use actix_web::{http::StatusCode, HttpResponse, ResponseError};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Discord error: {0}")]
    Discord(String),

    #[error("Serenity error: {0}")]
    Serenity(#[from] serenity::Error),

    #[error("HTTP request error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

pub type AppResult<T> = Result<T, AppError>;

impl ResponseError for AppError {
    fn error_response(&self) -> HttpResponse {
        log::error!("HTTP error response: {}", self);

        let status_code = self.status_code();
        let body = serde_json::json!({
            "error": self.to_string(),
            "status": status_code.as_u16(),
        });

        HttpResponse::build(status_code)
            .content_type("application/json")
            .json(body)
    }

    fn status_code(&self) -> StatusCode {
        match self {
            AppError::NotFound(_) => StatusCode::NOT_FOUND,
            AppError::Config(_) | AppError::Json(_) => StatusCode::BAD_REQUEST,
            AppError::Database(_)
            | AppError::Discord(_)
            | AppError::Serenity(_)
            | AppError::Http(_)
            | AppError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl From<String> for AppError {
    fn from(s: String) -> Self {
        AppError::Internal(s)
    }
}

impl From<&str> for AppError {
    fn from(s: &str) -> Self {
        AppError::Internal(s.to_string())
    }
}
