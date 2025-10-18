use crate::DbPool;
use crate::error::ApiError;
use bcrypt::{DEFAULT_COST, hash};
use r2d2::PooledConnection;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::OptionalExtension;
use tracing::info;
use uuid::Uuid;

pub(crate) fn init_db(pool: &DbPool, admin_pswd: String) -> anyhow::Result<(), rusqlite::Error> {
    let conn = pool.get().map_err(|_| rusqlite::Error::InvalidQuery)?; // simple map
    conn.execute_batch(
        r#"
        PRAGMA foreign_keys = ON;

        CREATE TABLE IF NOT EXISTS users (
            id TEXT PRIMARY KEY,
            username TEXT UNIQUE NOT NULL,
            password_hash TEXT NOT NULL,
            role TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS categories (
            name TEXT PRIMARY KEY
        );

        CREATE TABLE IF NOT EXISTS trivia (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            content TEXT NOT NULL,
            category TEXT NOT NULL,
            author TEXT NOT NULL,
            status TEXT NOT NULL,
            reviewer TEXT,
            reason TEXT,
            created_at TEXT NOT NULL,
            FOREIGN KEY(category) REFERENCES categories(name)
        );

        -- ensure some default categories
        INSERT OR IGNORE INTO categories(name) VALUES
            ('编程语言'),
            ('操作系统'),
            ('网络'),
            ('硬件'),
            ('历史趣闻');

        -- create an admin test user if not exists
        "#,
    )?;
    // ensure admin user exists: username "admin", password "admin123" (please change in prod)
    let existing: Option<String> = conn
        .query_row(
            "SELECT id FROM users WHERE username = ?1",
            rusqlite::params!["admin"],
            |r| r.get(0),
        )
        .optional()?;
    if existing.is_none() {
        let id = Uuid::new_v4().to_string();
        let pw_hash = hash(&admin_pswd, DEFAULT_COST).map_err(|_| rusqlite::Error::InvalidQuery)?;
        conn.execute(
            "INSERT INTO users (id, username, password_hash, role) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![id, "admin", pw_hash, "admin"],
        )?;
        info!(
            "Created default admin user: username='admin' password='{}' (change in production)",
            admin_pswd
        );
    }
    Ok(())
}

pub(crate) fn get_conn(
    pool: &DbPool,
) -> anyhow::Result<PooledConnection<SqliteConnectionManager>, ApiError> {
    pool.get()
        .map_err(|_| ApiError::DbError(rusqlite::Error::InvalidQuery))
}
