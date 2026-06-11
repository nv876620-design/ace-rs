FROM rust:1.83 AS builder
WORKDIR /app

# Install build dependencies
RUN apt-get update && \
    apt-get install -y pkg-config libsqlite3-dev && \
    rm -rf /var/lib/apt/lists/*

# Copy manifests
COPY Cargo.toml Cargo.lock ./

# Copy source and sqlx offline query cache
COPY src ./src
COPY .sqlx ./.sqlx

# Set sqlx to offline mode (no database needed at compile time)
ENV SQLX_OFFLINE=true

# Build server binary with server feature
RUN cargo build --release --bin ace-server-rs --features server

# Runtime image
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && \
    apt-get install -y libsqlite3-0 ca-certificates && \
    rm -rf /var/lib/apt/lists/*

# Copy binary
COPY --from=builder /app/target/release/ace-server-rs /usr/local/bin/

# Expose port
EXPOSE 8080

# Volume for database
VOLUME /data

# Run server
CMD ["ace-server-rs"]
