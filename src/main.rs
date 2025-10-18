mod accounts;
mod database;
mod error;
mod review;
mod trivia;

use crate::accounts::{get_me, login, signup};
use crate::database::init_db;
use crate::review::{admin_list_review, admin_review_put};
use crate::trivia::{
    add_category, create_trivia, get_random_trivia, get_trivia_by_id, list_categories, list_trivia,
};
use anyhow::Result;
use axum::{
    Router,
    routing::{get, post, put},
};
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use serde::{Deserialize, Serialize};
use std::{net::SocketAddr, sync::Arc};
use tracing::info;
use tracing_subscriber;

type DbPool = Pool<SqliteConnectionManager>;

#[derive(Clone)]
struct AppState {
    pool: Arc<DbPool>,
    jwt_secret: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Claims {
    sub: String,
    role: String,
    exp: usize,
}



#[tokio::main]
async fn main() -> Result<()> {
   tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let database_url =
        std::env::var("DATABASE_URL").unwrap_or_else(|_| "trivia.sqlite".to_string());
    let jwt_secret = std::env::var("JWT_SECRET").unwrap_or_else(|_| {
        tracing::warn!(
            "JWT_SECRET not set; using insecure default for testing. Set JWT_SECRET in production!"
        );
        "insecure_test_secret".to_string()
    });

    let manager = SqliteConnectionManager::file(database_url.clone());
    let pool = Pool::builder()
        .build(manager)
        .expect("failed to create pool");
    init_db(&pool).expect("failed to init db");

    let state = AppState {
        pool: Arc::new(pool),
        jwt_secret: jwt_secret.clone(),
    };

    let app = Router::new()
        .route("/api/v1/auth/signup", post(signup))
        .route("/api/v1/auth/login", post(login))
        .route("/api/v1/users/me", get(get_me))
        .route("/api/v1/trivia", post(create_trivia).get(list_trivia))
        .route("/api/v1/trivia/random", get(get_random_trivia))
        .route("/api/v1/trivia/{id}", get(get_trivia_by_id))
        .route(
            "/api/v1/categories",
            get(list_categories).post(add_category),
        )
        .route("/api/v1/admin/review", get(admin_list_review))
        .route("/api/v1/admin/review/{id}", put(admin_review_put))
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    info!("listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let server = axum::serve(listener, app);
    tokio::select! {
        res = server => {
            // 正常服务结束
            res?;
        }
        _ = tokio::signal::ctrl_c() => {
            info!("收到 Ctrl+C，正在退出...");
        }
    }
    Ok(())
}
