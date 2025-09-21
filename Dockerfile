# Build stage
FROM rust:latest AS builder

WORKDIR /app

# Copy manifests
COPY Cargo.toml Cargo.lock ./

# Copy source code
COPY src ./src

# Build release binary
RUN cargo build --release

# Runtime stage  
FROM debian:trixie-slim

# Install runtime dependencies
RUN apt-get update && \
    apt-get install -y ca-certificates sqlite3 && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the binary from builder
COPY --from=builder /app/target/release/sprocket /usr/local/bin/sprocket

# Create directory for SQLite database and workspace
RUN mkdir -p /data /app/workflow_workspace && \
    chmod 777 /data /app/workflow_workspace

# Set environment variable for database location
ENV DATABASE_URL=sqlite:///data/sprocket.db

# Expose the port
EXPOSE 3000

# Change to a directory where we can write
WORKDIR /data

# Run the binary
CMD ["sprocket"]