//! Blob upload handler

use crate::server::{auth, error::ApiError};
use axum::{extract::State, http::HeaderMap, Json};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Deserialize)]
pub struct BatchUploadRequest {
    pub blobs: Vec<Blob>,
}

#[derive(Debug, Deserialize)]
pub struct Blob {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Serialize)]
pub struct BatchUploadResponse {
    pub blob_names: Vec<String>,
}

/// Generate blob_name from path and content (SHA-256)
fn generate_blob_name(path: &str, content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(path.as_bytes());
    hasher.update(b"\0");
    hasher.update(content.as_bytes());
    hex::encode(hasher.finalize())
}

/// Generate content hash (SHA-256)
fn sha256_hex(data: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data.as_bytes());
    hex::encode(hasher.finalize())
}

/// Handle batch blob upload
pub async fn handle_batch_upload(
    State(state): State<crate::server::api::AppState>,
    headers: HeaderMap,
    Json(request): Json<BatchUploadRequest>,
) -> Result<Json<BatchUploadResponse>, ApiError> {
    // Check bearer token
    auth::check_bearer_token(&state.pool, &headers).await?;

    let mut blob_names = Vec::new();

    for blob in request.blobs {
        let blob_name = generate_blob_name(&blob.path, &blob.content);
        let content_hash = sha256_hex(&blob.content);
        let now = Utc::now().to_rfc3339();

        // Upsert blob (insert or update if exists)
        sqlx::query!(
            r#"
            INSERT INTO blobs (blob_name, path, content, content_hash, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?)
            ON CONFLICT(blob_name) DO UPDATE SET
                content = excluded.content,
                content_hash = excluded.content_hash,
                updated_at = excluded.updated_at
            "#,
            blob_name,
            blob.path,
            blob.content,
            content_hash,
            now,
            now
        )
        .execute(&state.pool)
        .await?;

        blob_names.push(blob_name);
    }

    Ok(Json(BatchUploadResponse { blob_names }))
}

