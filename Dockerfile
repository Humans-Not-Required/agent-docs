# Stage 1: Build frontend
FROM node:20-slim AS frontend
WORKDIR /app/frontend
COPY frontend/package.json frontend/package-lock.json* ./
RUN npm install
COPY frontend/ ./
RUN npm run build

# Stage 2: Build backend
FROM rust:1.89-slim AS backend
WORKDIR /app
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
COPY Cargo.toml Cargo.lock* ./
RUN mkdir src && echo 'fn main() {}' > src/main.rs && echo 'pub fn dummy() {}' > src/lib.rs && cargo build --release 2>/dev/null || true
COPY src ./src
# Ensure Cargo rebuilds the real sources (COPY can preserve older mtimes from the build context)
RUN find src -type f -name '*.rs' -exec touch {} + && cargo build --release

# Stage 3: Runtime
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=backend /app/target/release/agent-docs ./agent-docs
COPY --from=frontend /app/frontend/dist ./frontend/dist
RUN mkdir -p data
ENV ROCKET_PORT=3005
ENV ROCKET_ADDRESS=0.0.0.0
ENV DATABASE_PATH=/app/data/agent_docs.db
ENV STATIC_DIR=./frontend/dist
EXPOSE 3005
CMD ["./agent-docs"]
