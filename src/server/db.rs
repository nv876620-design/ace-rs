//! Database setup and schema management

use anyhow::Result;
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use std::path::Path;
use tracing::info;

/// Setup database connection pool and initialize schema
pub async fn setup_database(db_path: &Path) -> Result<SqlitePool> {
    // Create parent directory if needed
    if let Some(parent) = db_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let db_url = format!("sqlite://{}", db_path.display());
    info!("Opening database: {}", db_url);

    use std::str::FromStr;
    let options = sqlx::sqlite::SqliteConnectOptions::from_str(&db_url)?
        .create_if_missing(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(10)
        .connect_with(options)
        .await?;

    // Initialize schema
    init_schema(&pool).await?;

    Ok(pool)
}

/// Initialize database schema
async fn init_schema(pool: &SqlitePool) -> Result<()> {
    info!("Initializing database schema");

    // Create tokens table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS tokens (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            token_hash TEXT NOT NULL UNIQUE,
            created_at TEXT NOT NULL,
            revoked_at TEXT NULL,
            last_used_at TEXT NULL
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE INDEX IF NOT EXISTS idx_tokens_hash
        ON tokens(token_hash)
        WHERE revoked_at IS NULL
        "#,
    )
    .execute(pool)
    .await?;

    // Create blobs table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS blobs (
            blob_name TEXT PRIMARY KEY,
            path TEXT NOT NULL,
            content TEXT NOT NULL,
            content_hash TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE INDEX IF NOT EXISTS idx_blobs_path
        ON blobs(path)
        "#,
    )
    .execute(pool)
    .await?;

    // Create FTS5 virtual table
    sqlx::query(
        r#"
        CREATE VIRTUAL TABLE IF NOT EXISTS blobs_fts USING fts5(
            blob_name UNINDEXED,
            path,
            content,
            content='blobs',
            content_rowid='rowid'
        )
        "#,
    )
    .execute(pool)
    .await?;

    // Create FTS sync triggers
    sqlx::query(
        r#"
        CREATE TRIGGER IF NOT EXISTS blobs_ai AFTER INSERT ON blobs BEGIN
            INSERT INTO blobs_fts(rowid, blob_name, path, content)
            VALUES (new.rowid, new.blob_name, new.path, new.content);
        END
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TRIGGER IF NOT EXISTS blobs_ad AFTER DELETE ON blobs BEGIN
            INSERT INTO blobs_fts(blobs_fts, rowid, blob_name, path, content)
            VALUES('delete', old.rowid, old.blob_name, old.path, old.content);
        END
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TRIGGER IF NOT EXISTS blobs_au AFTER UPDATE ON blobs BEGIN
            INSERT INTO blobs_fts(blobs_fts, rowid, blob_name, path, content)
            VALUES('delete', old.rowid, old.blob_name, old.path, old.content);
            INSERT INTO blobs_fts(rowid, blob_name, path, content)
            VALUES (new.rowid, new.blob_name, new.path, new.content);
        END
        "#,
    )
    .execute(pool)
    .await?;

    info!("Database schema initialized");
    Ok(())
}
