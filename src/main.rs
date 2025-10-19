mod accounts;
mod database;
mod error;
mod review;
mod trivia;
mod heyiwei;

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
use clap::Parser;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::info;
use tracing_subscriber;
use crate::heyiwei::get_heyiwei;

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

#[derive(clap::Parser)]
#[command(
    name = "Trivia API Server",
    version = "1.0",
    author = "harkerhand",
    about = "A trivia question API server"
)]
struct Cli {
    #[clap(short, long, default_value = "127.0.0.1")]
    ip: String,
    #[clap(short, long, default_value = "3000")]
    port: u16,
    #[clap(short, long, default_value = "admin123")]
    admin_pswd: String,
    #[clap(short, long, default_value = "jwt_secret")]
    jwt_secret: String,
    #[clap(short, long, default_value = "trivia.sqlite")]
    database_url: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();
    let database_url = cli.database_url;
    let jwt_secret = cli.jwt_secret;

    let manager = SqliteConnectionManager::file(database_url.clone());
    let pool = Pool::builder()
        .build(manager)
        .expect("failed to create pool");
    init_db(&pool, cli.admin_pswd).expect("failed to init db");

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
        .route("/api/v1/heyiwei", get(get_heyiwei))
        .with_state(state);

    let addr = format!("{}:{}", cli.ip, cli.port);
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
