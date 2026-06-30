use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use revtern_core::new_id;
use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("{0}")]
    BadRequest(String),
    #[error("{0}")]
    Unauthorized(String),
    #[error("{0}")]
    Forbidden(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Conflict(String),
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
    #[error(transparent)]
    SerdeJson(#[from] serde_json::Error),
    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),
}

#[derive(Serialize)]
struct ErrorBody {
    error: ErrorPayload,
}

#[derive(Serialize)]
struct ErrorPayload {
    code: &'static str,
    message: String,
    request_id: String,
}

impl ApiError {
    pub fn invalid(message: impl Into<String>) -> Self {
        Self::BadRequest(message.into())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            Self::BadRequest(message) => (StatusCode::BAD_REQUEST, "invalid_request", message),
            Self::Unauthorized(message) => (StatusCode::UNAUTHORIZED, "unauthorized", message),
            Self::Forbidden(message) => (StatusCode::FORBIDDEN, "forbidden", message),
            Self::NotFound(message) => (StatusCode::NOT_FOUND, "not_found", message),
            Self::Conflict(message) => (StatusCode::CONFLICT, "conflict", message),
            Self::Sqlx(error) => {
                tracing::error!(?error, "database error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "database_error",
                    "database request failed".to_string(),
                )
            }
            Self::SerdeJson(error) => {
                tracing::error!(?error, "json error");
                (
                    StatusCode::BAD_REQUEST,
                    "invalid_json",
                    "json payload could not be processed".to_string(),
                )
            }
            Self::Anyhow(error) => {
                tracing::error!(?error, "application error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    "request failed".to_string(),
                )
            }
        };

        (
            status,
            Json(ErrorBody {
                error: ErrorPayload {
                    code,
                    message,
                    request_id: new_id("req"),
                },
            }),
        )
            .into_response()
    }
}

pub type ApiResult<T> = Result<T, ApiError>;
