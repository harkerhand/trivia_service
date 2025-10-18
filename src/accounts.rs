use crate::database::get_conn;
use crate::error::ApiError;
use crate::{AppState, Claims};
use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use bcrypt::{DEFAULT_COST, hash, verify};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, TokenData, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use tokio::task;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub(crate) struct SignupRequest {
    username: String,
    password: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct LoginRequest {
    username: String,
    password: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct LoginResponse {
    token: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct MeResponse {
    id: String,
    username: String,
    role: String,
}

pub(crate) async fn signup(
    State(state): State<AppState>,
    Json(payload): Json<SignupRequest>,
) -> anyhow::Result<impl IntoResponse, ApiError> {
    let pool = state.pool.clone();
    let username = payload.username;
    let password = payload.password;
    // simple validations
    if username.trim().is_empty() || password.len() < 4 {
        return Err(ApiError::BadRequest(
            "username empty or password too short".to_string(),
        ));
    }
    let pw_hash = hash(password.as_str(), DEFAULT_COST)?;
    let user_id = Uuid::new_v4().to_string();
    let pool2 = pool.clone();
    let username2 = username.clone();
    task::spawn_blocking(move || -> anyhow::Result<(), ApiError> {
        let conn = get_conn(&pool2)?;
        conn.execute(
            "INSERT INTO users (id, username, password_hash, role) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![user_id, username2, pw_hash, "user"],
        )?;
        Ok(())
    })
    .await
    .map_err(|_| ApiError::DbError(rusqlite::Error::InvalidQuery))??;
    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({"message":"signup successful"})),
    ))
}

pub(crate) async fn login(
    State(state): State<AppState>,
    Json(payload): Json<LoginRequest>,
) -> anyhow::Result<impl IntoResponse, ApiError> {
    let pool = state.pool.clone();
    let username = payload.username;
    let password = payload.password;
    let jwt_secret = state.jwt_secret.clone();

    let row = task::spawn_blocking(
        move || -> anyhow::Result<Option<(String, String)>, ApiError> {
            let conn = get_conn(&pool)?;
            let mut stmt =
                conn.prepare("SELECT id, password_hash FROM users WHERE username = ?1")?;
            let mut rows = stmt.query(rusqlite::params![username])?;
            if let Some(r) = rows.next()? {
                let id: String = r.get(0)?;
                let pw_hash: String = r.get(1)?;
                Ok(Some((id, pw_hash)))
            } else {
                Ok(None)
            }
        },
    )
    .await
    .map_err(|_| ApiError::DbError(rusqlite::Error::InvalidQuery))??;

    if let Some((id, pw_hash)) = row {
        if verify(password.as_str(), &pw_hash)? {
            let exp = (OffsetDateTime::now_utc().unix_timestamp() + 60 * 60 * 24 * 7) as usize; // 7 days
            // fetch role:
            let role: String = {
                let pool = state.pool.clone();
                let id_clone = id.clone();
                task::spawn_blocking(move || -> anyhow::Result<String, ApiError> {
                    let conn = get_conn(&pool)?;
                    let role: String = conn.query_row(
                        "SELECT role FROM users WHERE id = ?1",
                        rusqlite::params![id_clone],
                        |r| r.get(0),
                    )?;
                    Ok(role)
                })
                .await
                .map_err(|_| ApiError::DbError(rusqlite::Error::InvalidQuery))??
            };
            let claims = Claims { sub: id, role, exp };
            let token = encode(
                &Header::default(),
                &claims,
                &EncodingKey::from_secret(jwt_secret.as_ref()),
            )?;
            let resp = LoginResponse { token };
            Ok((StatusCode::OK, Json(resp)))
        } else {
            Err(ApiError::Unauthorized)
        }
    } else {
        Err(ApiError::Unauthorized)
    }
}

pub(crate) fn parse_auth(
    headers: &HeaderMap,
    jwt_secret: &str,
) -> anyhow::Result<Claims, ApiError> {
    // accept header "Authorization: Bearer <token>"
    // Try to get Authorization header
    if let Some(v) = headers.get(axum::http::header::AUTHORIZATION) {
        if let Ok(s) = v.to_str() {
            if s.starts_with("Bearer ") {
                let token = s.trim_start_matches("Bearer ").trim();
                let token_data: TokenData<Claims> = decode(
                    token,
                    &DecodingKey::from_secret(jwt_secret.as_ref()),
                    &Validation::default(),
                )?;
                return Ok(token_data.claims);
            }
        }
    }
    Err(ApiError::Unauthorized)
}

pub(crate) async fn get_me(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> anyhow::Result<impl IntoResponse, ApiError> {
    // parse and validate JWT
    let claims = parse_auth(&headers, &state.jwt_secret)?;
    let user_id = claims.sub;
    let pool = state.pool.clone();
    let res = task::spawn_blocking(move || -> anyhow::Result<Option<MeResponse>, ApiError> {
        let conn = get_conn(&pool)?;
        let mut stmt = conn.prepare("SELECT id, username, role FROM users WHERE id = ?1")?;
        let mut rows = stmt.query(rusqlite::params![user_id])?;
        if let Some(r) = rows.next()? {
            Ok(Some(MeResponse {
                id: r.get(0)?,
                username: r.get(1)?,
                role: r.get(2)?,
            }))
        } else {
            Ok(None)
        }
    })
    .await
    .map_err(|_| ApiError::DbError(rusqlite::Error::InvalidQuery))??;

    if let Some(u) = res {
        Ok((StatusCode::OK, Json(u)))
    } else {
        Err(ApiError::NotFound)
    }
}
