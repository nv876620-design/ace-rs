//! Search handler using FTS5

use crate::server::{auth, error::ApiError};
use axum::{extract::State, http::HeaderMap, Json};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct SearchRequest {
    pub information_request: String,
    pub blobs: BlobsPayload,
    #[serde(default)]
    pub dialog: Vec<serde_json::Value>,
    #[serde(default)]
    pub max_output_length: i32,
    #[serde(default)]
    pub disable_codebase_retrieval: bool,
    #[serde(default)]
    pub enable_commit_retrieval: bool,
}

#[derive(Debug, Deserialize)]
pub struct BlobsPayload {
    pub checkpoint_id: Option<String>,
    pub added_blobs: Vec<String>,
    pub deleted_blobs: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct SearchResponse {
    pub formatted_retrieval: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct SearchResult {
    blob_name: String,
    path: String,
    snippet: String,
    score: f64,
}

/// Escape FTS5 query - wrap in quotes and escape internal quotes
fn escape_fts_query(query: &str) -> String {
    format!("\"{}\"", query.replace('"', "\"\""))
}

/// Format search results as markdown
fn format_search_results(results: &[SearchResult]) -> String {
    let mut output = String::new();
    output.push_str("# Relevant Code Context\n\n");

    for result in results {
        output.push_str(&format!("## {}\n\n", result.path));
        output.push_str(&format!("```\n{}\n```\n\n", result.snippet));
    }

    output
}

/// Handle search request
pub async fn handle_search(
    State(state): State<crate::server::api::AppState>,
    headers: HeaderMap,
    Json(request): Json<SearchRequest>,
) -> Result<Json<SearchResponse>, ApiError> {
    // Check bearer token
    auth::check_bearer_token(&state.pool, &headers).await?;

    if request.blobs.added_blobs.is_empty() {
        return Ok(Json(SearchResponse {
            formatted_retrieval: Some("No relevant code context found for your query.".to_string()),
        }));
    }

    // Build FTS5 query
    let fts_query = escape_fts_query(&request.information_request);

    // Search only in requested blobs - build safe IN clause
    let placeholders = request
        .blobs
        .added_blobs
        .iter()
        .map(|_| "?")
        .collect::<Vec<_>>()
        .join(",");

    let sql = format!(
        r#"
        SELECT b.blob_name, b.path,
               snippet(blobs_fts, 2, '<mark>', '</mark>', '...', 32) as snippet,
               bm25(blobs_fts) as score
        FROM blobs_fts
        JOIN blobs b ON blobs_fts.blob_name = b.blob_name
        WHERE blobs_fts MATCH ?
          AND b.blob_name IN ({})
        ORDER BY score
        LIMIT 20
        "#,
        placeholders
    );

    // Build query with parameters
    let mut query = sqlx::query_as::<_, SearchResult>(&sql).bind(&fts_query);

    for blob_name in &request.blobs.added_blobs {
        query = query.bind(blob_name);
    }

    let results = query.fetch_all(&state.pool).await?;

    if results.is_empty() {
        return Ok(Json(SearchResponse {
            formatted_retrieval: Some("No relevant code context found for your query.".to_string()),
        }));
    }

    let formatted = format_search_results(&results);

    Ok(Json(SearchResponse {
        formatted_retrieval: Some(formatted),
    }))
}

