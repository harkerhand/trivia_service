use axum::http::StatusCode;
use axum::Json;
use axum::response::IntoResponse;
use thiserror::Error;

#[derive(Error, Debug)]
pub(crate) enum ApiError {
    #[error("database error")]
    DbError(#[from] rusqlite::Error),
    #[error("bcrypt error")]
    BcryptError(#[from] bcrypt::BcryptError),
    #[error("jwt error")]
    JwtError(#[from] jsonwebtoken::errors::Error),
    #[error("not found")]
    NotFound,
    #[error("unauthorized")]
    Unauthorized,
    #[error("forbidden")]
    Forbidden,
    #[error("bad request: {0}")]
    BadRequest(String),
}

impl IntoResponse for ApiError {
     fn into_response(self) -> axum::response::Response {
        let code = match &self {
            ApiError::DbError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            ApiError::BcryptError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            ApiError::JwtError(_) => StatusCode::UNAUTHORIZED,
            ApiError::NotFound => StatusCode::NOT_FOUND,
            ApiError::Unauthorized => StatusCode::UNAUTHORIZED,
            ApiError::Forbidden => StatusCode::FORBIDDEN,
            ApiError::BadRequest(_) => StatusCode::BAD_REQUEST,
        };
        let body = Json(serde_json::json!({ "error": self.to_string() }));
        (code, body).into_response()
    }
}
