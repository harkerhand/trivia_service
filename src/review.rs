use crate::accounts::parse_auth;
use crate::database::get_conn;
use crate::error::ApiError;
use crate::trivia::{MessageResponse, Trivia, TriviaListQuery};
use crate::{AppState};
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use serde::Deserialize;
use tokio::task;

#[derive(Debug, Deserialize)]
pub(crate) struct ReviewRequest {
    status: String,
    reviewer: String,
    reason: Option<String>,
}

pub(crate) async fn admin_list_review(
    State(state): State<AppState>,
    Query(q): Query<TriviaListQuery>,
    headers: HeaderMap,
) -> anyhow::Result<impl IntoResponse, ApiError> {
    // admin only
    let claims = parse_auth(&headers, &state.jwt_secret)?;
    if claims.role != "admin" {
        return Err(ApiError::Forbidden);
    }
    // we'll reuse list_trivia but force status = pending
    let mut q2 = q; // q is already the inner TriviaListQuery due to destructuring
    q2.status = Some("pending".to_string());
    // call list_trivia internal logic by executing query on DB here
    let pool = state.pool.clone();
    let limit = q2.limit.unwrap_or(50);
    let offset = q2.offset.unwrap_or(0);
    let res = task::spawn_blocking(move || -> anyhow::Result<Vec<Trivia>, ApiError> {
        let conn = get_conn(&pool)?;
        let mut stmt = conn.prepare("SELECT id, title, content, category, author, status, reviewer, reason, created_at FROM trivia WHERE status = 'pending' LIMIT ?1 OFFSET ?2")?;
        let mut rows = stmt.query(rusqlite::params![limit as i64, offset as i64])?;
        let mut out = Vec::new();
        while let Some(r) = rows.next()? {
            out.push(Trivia {
                id: r.get(0)?,
                title: r.get(1)?,
                content: r.get(2)?,
                category: r.get(3)?,
                author: r.get(4)?,
                status: r.get(5)?,
                reviewer: r.get(6)?,
                reason: r.get(7)?,
                created_at: r.get(8)?,
            });
        }
        Ok(out)
    })
        .await
        .map_err(|_| ApiError::DbError(rusqlite::Error::InvalidQuery))??;
    Ok((StatusCode::OK, Json(res)))
}

pub(crate) async fn admin_review_put(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<ReviewRequest>,
) -> anyhow::Result<impl IntoResponse, ApiError> {
    // admin only
    let claims = parse_auth(&headers, &state.jwt_secret)?;
    if claims.role != "admin" {
        return Err(ApiError::Forbidden);
    }
    // Validate status
    let st = payload.status.clone();
    if st != "approved" && st != "rejected" && st != "pending" {
        return Err(ApiError::BadRequest("invalid status".to_string()));
    }
    let pool = state.pool.clone();
    let reviewer = payload.reviewer.clone();
    let reason = payload.reason.clone();
    let id2 = id.clone();
    task::spawn_blocking(move || -> anyhow::Result<(), ApiError> {
        let conn = get_conn(&pool)?;
        let affected = conn.execute(
            "UPDATE trivia SET status = ?1, reviewer = ?2, reason = ?3 WHERE id = ?4",
            rusqlite::params![st, reviewer, reason, id2],
        )?;
        if affected == 0 {
            return Err(ApiError::NotFound);
        }
        Ok(())
    })
    .await
    .map_err(|_| ApiError::DbError(rusqlite::Error::InvalidQuery))??;
    Ok((
        StatusCode::OK,
        Json(MessageResponse {
            message: "冷知识已审核。".to_string(),
        }),
    ))
}
