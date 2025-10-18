use crate::AppState;
use crate::accounts::parse_auth;
use crate::database::get_conn;
use crate::error::ApiError;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use tokio::task;
use uuid::Uuid;

#[derive(Debug, Serialize)]
pub(crate) struct MessageResponse {
    pub(crate) message: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct TriviaListQuery {
    pub(crate) status: Option<String>,
    pub(crate) category: Option<String>,
    pub(crate) author: Option<String>,
    pub(crate) limit: Option<u32>,
    pub(crate) offset: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RandomQuery {
    category: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CreateTriviaRequest {
    id: Option<String>,
    title: String,
    content: String,
    category: String,
    author: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct CreateTriviaResponse {
    id: String,
    status: String,
    message: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Trivia {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) content: String,
    pub(crate) category: String,
    pub(crate) author: String,
    pub(crate) status: String, // pending/approved/rejected
    pub(crate) reviewer: Option<String>,
    pub(crate) reason: Option<String>,
    pub(crate) created_at: String,
}

pub(crate) async fn create_trivia(
    State(state): State<AppState>,
    _headers: HeaderMap,
    Json(payload): Json<CreateTriviaRequest>,
) -> anyhow::Result<impl IntoResponse, ApiError> {
    // anyone can submit; status defaults to pending
    let db = state.pool.clone();
    let id = payload
        .id
        .clone()
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let title = payload.title.clone();
    let content = payload.content.clone();
    let category = payload.category.clone();
    let author = payload.author.clone();
    // use a simple string representation for timestamp
    let created_at = OffsetDateTime::now_utc().to_string();

    // If category not exists, insert into categories
    let db2 = db.clone();
    let id2 = id.clone();
    task::spawn_blocking(move || -> anyhow::Result<(), ApiError> {
        let conn = get_conn(&db2)?;
        conn.execute(
            "INSERT OR IGNORE INTO categories (name) VALUES (?1)",
            rusqlite::params![category],
        )?;
        conn.execute(
            "INSERT INTO trivia (id, title, content, category, author, status, reviewer, reason, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, NULL, ?7)",
            rusqlite::params![id2, title, content, category, author, "pending", created_at],
        )?;
        Ok(())
    })
        .await
        .map_err(|_| ApiError::DbError(rusqlite::Error::InvalidQuery))??;

    Ok((
        StatusCode::CREATED,
        Json(CreateTriviaResponse {
            id,
            status: "pending".to_string(),
            message: "提交成功，等待审核。".to_string(),
        }),
    ))
}

pub(crate) async fn get_trivia_by_id(
    State(state): State<AppState>,
    Path(id): Path<String>,
    _headers: HeaderMap,
) -> anyhow::Result<impl IntoResponse, ApiError> {
    // Only return if status == approved (public). Admin could access other statuses if we want,
    // but spec says only return approved. For admin access to any, we could detect admin token and allow — here we keep it simple.
    let pool = state.pool.clone();
    let row = task::spawn_blocking(move || -> anyhow::Result<Option<Trivia>, ApiError> {
        let conn = get_conn(&pool)?;
        let mut stmt = conn.prepare(
            "SELECT id, title, content, category, author, status, reviewer, reason, created_at
             FROM trivia WHERE id = ?1 AND status = 'approved'",
        )?;
        let mut rows = stmt.query(rusqlite::params![id])?;
        if let Some(r) = rows.next()? {
            Ok(Some(Trivia {
                id: r.get(0)?,
                title: r.get(1)?,
                content: r.get(2)?,
                category: r.get(3)?,
                author: r.get(4)?,
                status: r.get(5)?,
                reviewer: r.get(6)?,
                reason: r.get(7)?,
                created_at: r.get(8)?,
            }))
        } else {
            Ok(None)
        }
    })
    .await
    .map_err(|_| ApiError::DbError(rusqlite::Error::InvalidQuery))??;

    if let Some(t) = row {
        Ok((StatusCode::OK, Json(t)))
    } else {
        Err(ApiError::NotFound)
    }
}

pub(crate) async fn get_random_trivia(
    State(state): State<AppState>,
    Query(q): Query<RandomQuery>,
) -> anyhow::Result<impl IntoResponse, ApiError> {
    let pool = state.pool.clone();
    let category_opt = q.category;
    let res = task::spawn_blocking(move || -> anyhow::Result<Option<Trivia>, ApiError> {
        let conn = get_conn(&pool)?;
        let mut stmt = if let Some(_) = &category_opt {
            conn.prepare("SELECT id, title, content, category, author, status, reviewer, reason, created_at FROM trivia WHERE status = 'approved' AND category = ?1 ORDER BY RANDOM() LIMIT 1")?
        } else {
            conn.prepare("SELECT id, title, content, category, author, status, reviewer, reason, created_at FROM trivia WHERE status = 'approved' ORDER BY RANDOM() LIMIT 1")?
        };
        let mut rows = if let Some(cat2) = category_opt {
            stmt.query(rusqlite::params![cat2])?
        } else {
            stmt.query([])?
        };
        if let Some(r) = rows.next()? {
            Ok(Some(Trivia {
                id: r.get(0)?,
                title: r.get(1)?,
                content: r.get(2)?,
                category: r.get(3)?,
                author: r.get(4)?,
                status: r.get(5)?,
                reviewer: r.get(6)?,
                reason: r.get(7)?,
                created_at: r.get(8)?,
            }))
        } else {
            Ok(None)
        }
    })
        .await
        .map_err(|_| ApiError::DbError(rusqlite::Error::InvalidQuery))??;

    if let Some(t) = res {
        Ok((StatusCode::OK, Json(t)))
    } else {
        Err(ApiError::NotFound)
    }
}

pub(crate) async fn list_trivia(
    State(state): State<AppState>,
    Query(q): Query<TriviaListQuery>,
    headers: HeaderMap,
) -> anyhow::Result<impl IntoResponse, ApiError> {
    let pool = state.pool.clone();
    // If status param present: only admin can use
    if let Some(_) = &q.status {
        // check admin
        let claims = parse_auth(&headers, &state.jwt_secret)?;
        if claims.role != "admin" {
            return Err(ApiError::Forbidden);
        }
    }
    let status_opt = q.status.clone();
    let category_opt = q.category.clone();
    let author_opt = q.author.clone();
    let limit = q.limit.unwrap_or(50);
    let offset = q.offset.unwrap_or(0);

    let res = task::spawn_blocking(move || -> anyhow::Result<Vec<Trivia>, ApiError> {
        let conn = get_conn(&pool)?;
        let mut sql = "SELECT id, title, content, category, author, status, reviewer, reason, created_at FROM trivia WHERE 1=1".to_string();
        let mut params: Vec<rusqlite::types::Value> = vec![];
        if let Some(ref s) = status_opt {
            sql.push_str(" AND status = ?1");
            params.push(rusqlite::types::Value::from(s.clone()));
        }
        if let Some(ref cat) = category_opt {
            let idx = params.len() + 1;
            sql.push_str(&format!(" AND category = ?{}", idx));
            params.push(rusqlite::types::Value::from(cat.clone()));
        }
        if let Some(ref a) = author_opt {
            let idx = params.len() + 1;
            sql.push_str(&format!(" AND author = ?{}", idx));
            params.push(rusqlite::types::Value::from(a.clone()));
        } else {
            // if no status filter and not admin, only show approved
            if status_opt.is_none() {
                sql.push_str(" AND status = 'approved'");
            }
        }
        let idx_limit = params.len() + 1;
        let idx_offset = params.len() + 2;
        sql.push_str(&format!(" LIMIT ?{} OFFSET ?{}", idx_limit, idx_offset));
        params.push(rusqlite::types::Value::from(limit as i64));
        params.push(rusqlite::types::Value::from(offset as i64));

        let mut stmt = conn.prepare(&sql)?;
        let mut rows = stmt.query(rusqlite::params_from_iter(params.iter()))?;
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

pub(crate) async fn list_categories(
    State(state): State<AppState>,
) -> anyhow::Result<impl IntoResponse, ApiError> {
    let pool = state.pool.clone();
    let cats = task::spawn_blocking(move || -> anyhow::Result<Vec<String>, ApiError> {
        let conn = get_conn(&pool)?;
        let mut stmt = conn.prepare("SELECT name FROM categories ORDER BY name")?;
        let mut rows = stmt.query([])?;
        let mut out = Vec::new();
        while let Some(r) = rows.next()? {
            out.push(r.get(0)?);
        }
        Ok(out)
    })
    .await
    .map_err(|_| ApiError::DbError(rusqlite::Error::InvalidQuery))??;
    Ok((StatusCode::OK, Json(cats)))
}

pub(crate) async fn add_category(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<serde_json::Value>,
) -> anyhow::Result<impl IntoResponse, ApiError> {
    // admin only
    let claims = parse_auth(&headers, &state.jwt_secret)?;
    if claims.role != "admin" {
        return Err(ApiError::Forbidden);
    }
    let name = payload
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::BadRequest("missing name".to_string()))?
        .to_string();
    let pool = state.pool.clone();
    task::spawn_blocking(move || -> anyhow::Result<(), ApiError> {
        let conn = get_conn(&pool)?;
        conn.execute(
            "INSERT OR IGNORE INTO categories (name) VALUES (?1)",
            rusqlite::params![name],
        )?;
        Ok(())
    })
    .await
    .map_err(|_| ApiError::DbError(rusqlite::Error::InvalidQuery))??;
    Ok((
        StatusCode::CREATED,
        Json(MessageResponse {
            message: "分类添加成功".to_string(),
        }),
    ))
}
