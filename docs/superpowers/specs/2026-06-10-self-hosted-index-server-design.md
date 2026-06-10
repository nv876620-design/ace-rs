# Self-hosted Local-first Indexing/Search Server Design

Date: 2026-06-10

## Summary

Add a self-hosted backend server to this repository so `ace-tool-rs` can index and search code without relying on an external hosted indexing service. The MVP server will provide local-first indexing/search with SQLite FTS5, Docker deployment, bearer-token authentication for client API calls, and a simple Admin UI for issuing and revoking tokens.

The server only covers indexing and `search_context` retrieval. It does not implement prompt enhancement endpoints, third-party LLM integrations, project-level permissions, vector search, or multi-admin account management in the MVP.

## Goals

- Add a server binary, tentatively named `ace-server-rs`, in the existing Rust repository.
- Let the existing `ace-tool-rs` client use the server by setting `--base-url` and `--token`.
- Store indexed blobs locally in SQLite.
- Provide full-text/BM25 retrieval using SQLite FTS5.
- Provide a minimal Admin UI protected by `ACE_ADMIN_PASSWORD`.
- Let admins create, list, and revoke bearer tokens.
- Store only token hashes in the database; show plaintext tokens only once at creation.
- Provide Docker and Docker Compose deployment paths.

## Non-goals

- Implement `--enhance-prompt` backend support.
- Implement Claude, OpenAI, Gemini, Codex, or Augment prompt-enhancer endpoints.
- Implement embedding or vector search in the MVP.
- Implement user accounts or per-project permissions.
- Implement multi-node clustering or distributed indexing.
- Replace the client-side `.ace-tool/index.bin` cache format.

## Architecture

The recommended architecture adds one Rust HTTP server binary to the current crate. The server uses Axum on Tokio, SQLite as the persistent store, and SQLite FTS5 for local-first retrieval.

```text
ace-tool-rs client
  --base-url http://localhost:8080
  --token <issued-token>
        |
        v
ace-server-rs
  ├── Auth middleware
  │   └── validates bearer tokens stored as hashes in SQLite
  ├── Index API
  │   └── receives blob batches and upserts blob + FTS rows
  ├── Search API
  │   └── performs FTS5/BM25 retrieval scoped to requested blob names
  ├── Admin API/UI
  │   └── login with ACE_ADMIN_PASSWORD, create/list/revoke tokens
  └── SQLite database
      ├── tokens
      ├── blobs
      └── blobs_fts
```

Suggested module layout:

```text
src/server/
├── mod.rs
├── config.rs        # env config: bind addr, DB path, admin password, session secret
├── app.rs           # Axum router and shared app state
├── auth.rs          # bearer token auth and admin session auth
├── db.rs            # SQLite pool and migrations
├── tokens.rs        # create/list/revoke/verify tokens
├── blobs.rs         # blob upload/upsert and FTS sync
├── search.rs        # FTS5/BM25 retrieval and formatting
├── admin.rs         # admin routes and HTML rendering
└── errors.rs        # API error mapping
```

The binary entrypoint can live at `src/bin/ace-server-rs.rs`, with a corresponding `[[bin]]` entry in `Cargo.toml` if needed.

## API Compatibility

The MVP should keep the existing client nearly unchanged. Users should be able to run:

```bash
ace-tool-rs --base-url http://localhost:8080 --token <issued-token>
```

The server must implement the concrete endpoint paths and JSON shapes currently used by `IndexManager` for blob upload and retrieval. During implementation, the first step is to inspect the URL construction in `IndexManager` and `service::common::build_api_url`, then implement matching routes.

The expected payload shapes from the current code are:

```json
{
  "blobs": [
    { "path": "src/main.rs", "content": "..." }
  ]
}
```

with response:

```json
{
  "blob_names": ["..."]
}
```

and search requests shaped around:

```json
{
  "information_request": "Where is configuration loaded?",
  "blobs": {
    "checkpoint_id": null,
    "added_blobs": ["..."],
    "deleted_blobs": []
  },
  "dialog": [],
  "max_output_length": 20000,
  "disable_codebase_retrieval": false,
  "enable_commit_retrieval": false
}
```

with response:

```json
{
  "formatted_retrieval": "..."
}
```

If local HTTP is required, the client should be adjusted so `http://localhost` and `http://127.0.0.1` are not forcibly rewritten to HTTPS. This should be a narrow compatibility change for local/self-host deployments, not a broad weakening of production URL handling.

## Indexing Flow

```text
client scans project files
  → client splits files into blobs
  → client uploads new blob batches
  → server validates bearer token
  → server upserts blobs into SQLite
  → server updates FTS5 rows
  → server returns blob_names
  → client stores local .ace-tool/index.bin cache
```

The client remains responsible for scanning files, chunking blobs, mtime caching, and deciding which blobs are new. The server stores and searches the uploaded blobs.

`blob_name` should be stable. For the MVP, the server can derive it from SHA-256 of normalized path and content, unless implementation discovers that the current client expects a different naming convention.

## Search Flow

```text
client calls search_context
  → client sends natural-language query and added_blobs from local index
  → server validates bearer token
  → server performs FTS5 search scoped to requested added_blobs
  → server formats top matches into retrieval text
  → server returns formatted_retrieval
```

Even though MVP tokens are global, search must be scoped to the `added_blobs` list sent by the client. This avoids returning unrelated blobs from other projects indexed on the same server.

When no relevant match is found, return:

```text
No relevant code context found for your query.
```

## SQLite Schema

Use one SQLite database file, defaulting to `/data/ace-server.db` in Docker.

### `tokens`

```sql
CREATE TABLE tokens (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  token_hash TEXT NOT NULL UNIQUE,
  created_at TEXT NOT NULL,
  revoked_at TEXT NULL,
  last_used_at TEXT NULL
);
```

Plaintext tokens are generated once and shown only at creation time. The database stores `token_hash`, never the plaintext token.

### `blobs`

```sql
CREATE TABLE blobs (
  blob_name TEXT PRIMARY KEY,
  path TEXT NOT NULL,
  content TEXT NOT NULL,
  content_hash TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
```

### `blobs_fts`

```sql
CREATE VIRTUAL TABLE blobs_fts USING fts5(
  blob_name UNINDEXED,
  path,
  content
);
```

FTS rows are inserted or updated whenever `blobs` rows change. Search uses FTS5 with BM25 ordering and returns path/snippet/content excerpts.

## Admin UI

Admin authentication is configured by environment variable:

```text
ACE_ADMIN_PASSWORD=<strong-password>
```

The server should refuse to start if `ACE_ADMIN_PASSWORD` is missing, unless an explicit development mode is introduced later.

Admin routes:

```text
GET  /admin
POST /admin/login
GET  /admin/tokens
POST /admin/tokens
POST /admin/tokens/:id/revoke
POST /admin/logout
```

Admin UI pages:

- Login page with a password field.
- Token list page showing token name, active/revoked status, creation time, and last-used time.
- Create-token form with a token name field.
- One-time plaintext token display after creation.
- Revoke button for active tokens.

The MVP Admin UI should be server-rendered HTML or static HTML served by Rust. It should not introduce a Node/frontend build pipeline.

Admin sessions should use an HTTP-only cookie with `SameSite=Lax`. Session signing or verification should use `ACE_SESSION_SECRET`.

## Configuration

Environment variables:

```text
ACE_BIND_ADDR=0.0.0.0:8080
ACE_DB_PATH=/data/ace-server.db
ACE_ADMIN_PASSWORD=<required>
ACE_SESSION_SECRET=<required>
RUST_LOG=info
```

`ACE_ADMIN_PASSWORD` and `ACE_SESSION_SECRET` must not be logged.

## Docker Deployment

Add a multi-stage `Dockerfile`:

```text
builder image
  → cargo build --release --bin ace-server-rs

runtime image
  → copy ace-server-rs binary
  → expose 8080
  → mount /data
  → run ace-server-rs
```

Add a sample `docker-compose.yml`:

```yaml
services:
  ace-server:
    build: .
    ports:
      - "8080:8080"
    volumes:
      - ace-data:/data
    environment:
      ACE_BIND_ADDR: 0.0.0.0:8080
      ACE_DB_PATH: /data/ace-server.db
      ACE_ADMIN_PASSWORD: change-me
      ACE_SESSION_SECRET: change-me-too
      RUST_LOG: info
volumes:
  ace-data:
```

For public deployment, place the server behind an HTTPS reverse proxy. The Rust server itself only needs to support plain HTTP for local/Docker operation in the MVP.

## Error Handling

API errors should use consistent JSON:

```json
{
  "error": {
    "code": "unauthorized",
    "message": "Invalid or revoked token"
  }
}
```

HTTP status mapping:

- `400 bad_request`: invalid JSON, empty query, invalid blob payload.
- `401 unauthorized`: missing, invalid, or revoked bearer token.
- `413 payload_too_large`: upload batch exceeds configured server limits.
- `500 internal`: database, migration, or unexpected server error.
- `503 unavailable`: transient database lock or overload, if distinguishable.

Admin UI errors can render simple HTML messages and preserve safe navigation back to the token page or login page.

## Security Baseline

- Client API uses `Authorization: Bearer <token>`.
- Token hashes are stored in SQLite; plaintext tokens are never stored.
- Admin password is read from `ACE_ADMIN_PASSWORD` and never logged.
- Admin cookies are HTTP-only and `SameSite=Lax`.
- Admin sessions are signed or otherwise server-verifiable using `ACE_SESSION_SECRET`.
- Revoked tokens cannot upload or search.
- Token `last_used_at` is updated after successful API authentication.
- Search is scoped to `added_blobs` supplied by the current client request.

## Testing Plan

Unit tests:

- Token generation, hashing, verification, and revoke behavior.
- Admin password/session helper behavior.
- FTS query escaping and query normalization.
- API error response mapping.

Integration tests:

- Start server with a temporary SQLite database.
- Create a token through a test helper or admin route.
- Upload blob batches and verify rows in `blobs` and `blobs_fts`.
- Search returns expected path/snippet for uploaded content.
- Search only returns results from requested `added_blobs`.
- Revoked tokens are rejected for upload and search.
- Missing/invalid tokens receive `401`.

Compatibility test:

- Use the existing `IndexManager` against the test server.
- Run `index_project()` on a temporary project.
- Run `search_context()` and verify the formatted retrieval contains expected context.

Docker smoke test instructions:

- Build the image.
- Run the container with `/data` volume and required env vars.
- Visit `/admin`, create a token, then run `ace-tool-rs --base-url http://localhost:8080 --token <token> --index-only`.

## Implementation Notes

The first implementation step should verify exact client endpoint paths before adding server routes. The design intentionally keeps server behavior compatible with the current client where possible.

If Rust proves unexpectedly inconvenient for the Admin UI, the fallback is still to keep the backend in Rust and serve static HTML/JS. A separate Node service is not part of the MVP unless the Rust path becomes a clear blocker.
