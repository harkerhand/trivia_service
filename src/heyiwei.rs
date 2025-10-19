use axum::response::IntoResponse;
use crate::error::ApiError;

pub(crate) async fn get_heyiwei(
) -> anyhow::Result<impl IntoResponse, ApiError> {
    let hyw = heyiwei::Heyiwei::random();
    Ok((axum::http::StatusCode::OK, axum::Json(hyw.to_string())))
}