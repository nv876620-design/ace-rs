# Self-hosted ACE Server

This document describes how to run the self-hosted ACE indexing and search server.

## Overview

The `ace-server-rs` binary provides a local-first backend server that:

- Accepts blob uploads from `ace-tool-rs` clients
- Stores indexed code in SQLite with FTS5 full-text search
- Provides BM25-based semantic search for the `search_context` MCP tool
- Includes a simple Admin UI for creating and managing bearer tokens
- Runs entirely self-hosted with no external dependencies

**What this server provides:**
- `POST /batch-upload` - Upload code blobs
- `POST /agents/codebase-retrieval` - Search indexed code
- `/admin` - Token management UI

**What this server does NOT provide:**
- Prompt enhancement (`--enhance-prompt`)
- Vector/embedding search (uses BM25/FTS5 instead)
- Project-level permissions (all tokens can access all indexed code)

## Quick Start with Docker Compose

The easiest way to run the server:

```bash
# Set required passwords
export ACE_ADMIN_PASSWORD=your-strong-admin-password
export ACE_SESSION_SECRET=your-session-secret-key

# Start server (pulls pre-built image from ghcr.io)
docker-compose up

# Server will be available at http://localhost:8080
```

To build locally instead of using the pre-built image:

```bash
# Uncomment 'build: .' and comment 'image:' line in docker-compose.yml
docker-compose up --build
```

## Manual Docker Run

Using pre-built image:

```bash
docker run -d \
  -p 8080:8080 \
  -v ace-data:/data \
  -e ACE_ADMIN_PASSWORD=your-strong-admin-password \
  -e ACE_SESSION_SECRET=your-session-secret-key \
  -e RUST_LOG=info \
  --name ace-server \
  ghcr.io/ndnhatvien/ace-server-rs:latest
```

Or build locally:

```bash
docker build -t ace-server .

docker run -d \
  -p 8080:8080 \
  -v ace-data:/data \
  -e ACE_ADMIN_PASSWORD=your-strong-admin-password \
  -e ACE_SESSION_SECRET=your-session-secret-key \
  -e RUST_LOG=info \
  --name ace-server \
  ace-server
```

## Build and Run from Source

```bash
# Build server binary
cargo build --release --bin ace-server-rs --features server

# Set required environment variables
export ACE_ADMIN_PASSWORD=your-strong-admin-password
export ACE_SESSION_SECRET=your-session-secret-key
export ACE_DB_PATH=./data/ace-server.db
export ACE_BIND_ADDR=127.0.0.1:8080

# Run server
./target/release/ace-server-rs
```

## Environment Variables

| Variable | Required | Default | Description |
|---|---|---|---|
| `ACE_ADMIN_PASSWORD` | Yes | - | Admin UI password (min 8 chars) |
| `ACE_SESSION_SECRET` | Yes | - | Session cookie signing key (min 16 chars) |
| `ACE_BIND_ADDR` | No | `127.0.0.1:8080` | Server bind address |
| `ACE_DB_PATH` | No | `/data/ace-server.db` | SQLite database file path |
| `RUST_LOG` | No | - | Logging level (`info`, `debug`, etc.) |

## Using the Admin UI

1. Visit `http://localhost:8080/admin`
2. Login with your `ACE_ADMIN_PASSWORD`
3. Create a new token:
   - Enter a descriptive name (e.g., "laptop-dev", "ci-server")
   - Click "Create Token"
   - **Copy the token immediately** - it will not be shown again
4. Manage tokens:
   - View all tokens, creation time, and last used time
   - Revoke tokens that are no longer needed

## Using with ace-tool-rs Client

Once you have a token from the Admin UI:

```bash
# Index a project
ace-tool-rs \
  --base-url http://localhost:8080 \
  --token <your-token-here> \
  --index-only

# Use as MCP server (for Claude Code, etc.)
ace-tool-rs \
  --base-url http://localhost:8080 \
  --token <your-token-here>
```

The client will:
- Upload new/modified file blobs to the server
- Cache local `.ace-tool/index.bin` for fast re-indexing
- Query the server for `search_context` retrieval

## Security Notes

**For local/development use:**
- Default `127.0.0.1` binding is safe for localhost-only access
- HTTP is acceptable for local Docker/development

**For production/network deployment:**
- Use a strong `ACE_ADMIN_PASSWORD` (16+ characters, mixed case, numbers, symbols)
- Use a strong `ACE_SESSION_SECRET` (32+ random characters)
- Deploy behind an HTTPS reverse proxy (nginx, Caddy, Traefik)
- Never bind to `0.0.0.0` on untrusted networks without HTTPS
- Consider firewall rules to restrict access
- Rotate admin password periodically
- Revoke unused tokens

## Database Management

The SQLite database is stored at `ACE_DB_PATH` (default `/data/ace-server.db` in Docker).

**Backup:**
```bash
# Stop server first
docker-compose down

# Copy database
cp /var/lib/docker/volumes/ace-rs_ace-data/_data/ace-server.db ./backup.db

# Or use Docker volume backup
docker run --rm -v ace-rs_ace-data:/data -v $(pwd):/backup alpine cp /data/ace-server.db /backup/
```

**Reset database:**
```bash
docker-compose down
docker volume rm ace-rs_ace-data
docker-compose up
```

## Troubleshooting

**Server won't start:**
- Check `ACE_ADMIN_PASSWORD` and `ACE_SESSION_SECRET` are set and meet minimum length requirements
- Check port 8080 is not already in use
- Check database file location is writable

**Client can't connect:**
- Verify server is running: `curl http://localhost:8080/admin`
- Check client is using `http://localhost:8080` (not `https://`)
- Verify token is valid and not revoked

**Search returns no results:**
- Verify project was indexed successfully (`--index-only` first)
- Check database contains blobs: inspect SQLite file or check admin logs
- Try broader search query

**Admin UI login fails:**
- Verify `ACE_ADMIN_PASSWORD` matches what you're entering
- Note: password attempts have a 1-second delay for basic rate limiting

## API Reference

### POST /batch-upload

Upload code blobs for indexing.

**Request:**
```json
{
  "blobs": [
    {
      "path": "src/main.rs",
      "content": "fn main() { ... }"
    }
  ]
}
```

**Headers:**
```
Authorization: Bearer <token>
Content-Type: application/json
```

**Response:**
```json
{
  "blob_names": ["<sha256-hash>", ...]
}
```

### POST /agents/codebase-retrieval

Search indexed code blobs.

**Request:**
```json
{
  "information_request": "Where is configuration loaded?",
  "blobs": {
    "checkpoint_id": null,
    "added_blobs": ["<hash>", ...],
    "deleted_blobs": []
  },
  "dialog": [],
  "max_output_length": 0,
  "disable_codebase_retrieval": false,
  "enable_commit_retrieval": false
}
```

**Response:**
```json
{
  "formatted_retrieval": "# Relevant Code Context\n\n## src/config.rs\n\n```\n...\n```"
}
```

## Performance

- Small projects (<100 files): instant indexing, <100ms search
- Medium projects (100-1000 files): <5s indexing, <200ms search
- Large projects (1000-10000 files): <30s indexing, <500ms search

FTS5 BM25 search is fast enough for most use cases. If you need higher-quality semantic search, consider upgrading to a vector-based retrieval backend in the future.

## Limitations

- No vector/embedding search (BM25/FTS5 only)
- No per-project access control (all tokens see all indexed code)
- No multi-admin accounts (single admin password only)
- No prompt enhancement backend
- No distributed/multi-node indexing

These are intentional MVP limitations. Future versions may add these features.

## Support

For issues, questions, or feature requests, see the main `ace-tool-rs` repository.
